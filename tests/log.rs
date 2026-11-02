//! Results-log tests: group commit, crash safety, and compatibility between old and new logs.

use palhunt::digits::DecimalChecker;
use palhunt::layout::layout;
use palhunt::state::{self, CommitPolicy};
use palhunt::unit::{UnitId, UnitResult, run_unit, unit_plan};
use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

const BIN: &str = env!("CARGO_BIN_EXE_palhunt");

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("palhunt-log-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

/// Every unit of bit lengths `1..=to_bits` at `unit_bits`, computed in-process.
fn all_units(to_bits: u32, unit_bits: u32) -> Vec<UnitResult> {
    let dec = DecimalChecker::new();
    let mut out = Vec::new();
    for bits in 1..=to_bits {
        if let Some(l) = layout(bits) {
            let (_, units) = unit_plan(&l, unit_bits);
            out.extend((0..units).map(|i| run_unit(&l, unit_bits, i, &dec)));
        }
    }
    out
}

fn assert_same_work(a: &HashMap<UnitId, UnitResult>, b: &HashMap<UnitId, UnitResult>) {
    assert_eq!(a.len(), b.len(), "different number of units");
    for (id, r) in a {
        let other = b.get(id).unwrap_or_else(|| panic!("unit {id:?} missing"));
        assert!(r.same_work(other), "unit {id:?} differs");
    }
}

/// Many finished units share one sync, and every one of them is in the file afterwards.
#[test]
fn group_commit_shares_syncs_between_units() {
    let dir = temp_dir("syncs");
    let results = all_units(40, 3);
    let n = results.len();
    assert!(n > 100, "need enough units to batch, got {n}");

    let (tx, rx) = mpsc::channel();
    for r in &results {
        tx.send(r.clone()).unwrap();
    }
    drop(tx);

    let mut log = state::Log::open(&dir).unwrap();
    let policy = CommitPolicy {
        interval: Duration::from_secs(60),
        max_units: 32,
    };
    let mut seen = 0;
    state::commit_loop(&mut log, rx, policy, |_| seen += 1).unwrap();

    assert_eq!(seen, n, "every result is handed to the callback exactly once");
    assert_eq!(log.syncs(), n.div_ceil(32) as u64, "one sync per full batch");
    let done = state::load(&dir).unwrap();
    assert_eq!(done.len(), n);
    for r in &results {
        let id = UnitId {
            bits: r.bits,
            index: r.unit,
        };
        assert!(done[&id].same_work(r));
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// A lone finished unit is not held back until more arrive: it reaches disk within the interval
/// even while the workers are still running (sender alive).
#[test]
fn a_lone_unit_is_flushed_within_the_interval() {
    let dir = temp_dir("interval");
    let first = all_units(1, 0).remove(0);
    let (tx, rx) = mpsc::channel();
    let policy = CommitPolicy {
        interval: Duration::from_millis(50),
        max_units: 1000,
    };
    let writer_dir = dir.clone();
    let writer = std::thread::spawn(move || {
        let mut log = state::Log::open(&writer_dir).unwrap();
        state::commit_loop(&mut log, rx, policy, |_| {}).unwrap();
        log.syncs()
    });
    tx.send(first).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while state::load(&dir).map(|d| d.len()).unwrap_or(0) < 1 {
        assert!(
            Instant::now() < deadline,
            "unit was not flushed while the sender was alive"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    drop(tx);
    assert_eq!(writer.join().unwrap(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

/// The loader repairs any damage a crash can leave: a torn line, NUL padding or non-UTF-8 bytes
/// in the middle of the file must be skipped, not abort the load.
#[test]
fn loader_skips_damaged_lines_anywhere() {
    let dir = temp_dir("damage");
    let results = all_units(12, 2);
    let mut log = state::Log::open(&dir).unwrap();
    let (head, tail) = results.split_at(results.len() / 2);
    log.append_batch(head).unwrap();
    drop(log);
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .open(state::log_path(&dir))
        .unwrap();
    f.write_all(b"{\"v\":1,\"engine\":\"cpu-gr").unwrap(); // torn, no newline
    drop(f);
    let log = state::Log::open(&dir).unwrap(); // repairs by starting a fresh line
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .open(state::log_path(&dir))
        .unwrap();
    f.write_all(b"\0\0\0\0\xff\xfe\xc3\x28 not json\n").unwrap();
    f.write_all(&[0u8; 64]).unwrap(); // zero-filled tail of an unsynced page, no newline
    drop(f);
    drop(log);
    let mut log = state::Log::open(&dir).unwrap();
    log.append_batch(tail).unwrap();
    let done = state::load(&dir).unwrap();
    assert_eq!(done.len(), results.len());
    let _ = std::fs::remove_dir_all(&dir);
}

fn palhunt(args: &[&str]) -> std::process::Output {
    let out = Command::new(BIN).args(args).output().unwrap();
    assert!(
        out.status.success(),
        "palhunt {args:?} failed:\n{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    out
}

fn path_str(p: &Path) -> &str {
    p.to_str().unwrap()
}

/// `tests/fixtures/old-code-results.jsonl` was written by the per-unit-sync code on `main`
/// before group commit (`search --to-bits 44 --unit-bits 6 --threads 1`). Logs written by that
/// code and by the new code must verify against each other.
#[test]
fn old_code_log_and_new_code_log_verify_against_each_other() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/old-code-results.jsonl");
    let old_dir = temp_dir("old");
    std::fs::create_dir_all(&old_dir).unwrap();
    std::fs::copy(&fixture, state::log_path(&old_dir)).unwrap();
    let old = state::load(&old_dir).unwrap();
    assert!(old.len() > 100);

    // New code, same work, through the real binary and its group-commit writer.
    let new_dir = temp_dir("new");
    palhunt(&[
        "search",
        "--to-bits",
        "44",
        "--unit-bits",
        "6",
        "--state-dir",
        path_str(&new_dir),
    ]);
    let new = state::load(&new_dir).unwrap();
    assert_same_work(&old, &new);

    // Each log verifies under the new binary, every unit, and so does their concatenation.
    let n = old.len().to_string();
    palhunt(&["verify", "--sample", &n, "--state-dir", path_str(&old_dir)]);
    palhunt(&["verify", "--sample", &n, "--state-dir", path_str(&new_dir)]);
    let mixed = temp_dir("mixed");
    std::fs::create_dir_all(&mixed).unwrap();
    let mut both = std::fs::read(&fixture).unwrap();
    both.extend(std::fs::read(state::log_path(&new_dir)).unwrap());
    std::fs::write(state::log_path(&mixed), both).unwrap();
    palhunt(&["verify", "--sample", &n, "--state-dir", path_str(&mixed)]);

    // A restart on the old log with the new code appends nothing: every unit is already logged.
    let before = std::fs::read(state::log_path(&old_dir)).unwrap();
    palhunt(&[
        "search",
        "--to-bits",
        "44",
        "--unit-bits",
        "6",
        "--state-dir",
        path_str(&old_dir),
    ]);
    assert_eq!(before, std::fs::read(state::log_path(&old_dir)).unwrap());

    for d in [old_dir, new_dir, mixed] {
        let _ = std::fs::remove_dir_all(d);
    }
}

/// SIGKILL the real search mid-run, then resume: the totals end up identical to an
/// uninterrupted run, nothing is lost, and the file holds no unit twice with different content.
#[test]
fn killed_search_resumes_to_identical_totals() {
    const ARGS: [&str; 8] = [
        "search",
        "--from-bits",
        "100",
        "--to-bits",
        "113",
        "--unit-bits",
        "14",
        "--threads",
    ];
    let run_args = |dir: &Path| -> Vec<String> {
        ARGS.iter()
            .map(|s| s.to_string())
            .chain(["2".into(), "--state-dir".into(), path_str(dir).into()])
            .collect()
    };

    let killed = temp_dir("killed");
    let mut child = Command::new(BIN)
        .args(run_args(&killed))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    // Wait until some units are on disk, then kill without warning.
    let deadline = Instant::now() + Duration::from_secs(120);
    let log_file = state::log_path(&killed);
    loop {
        let len = std::fs::metadata(&log_file).map(|m| m.len()).unwrap_or(0);
        if len > 20_000 {
            break;
        }
        assert!(
            child.try_wait().unwrap().is_none(),
            "search finished before it could be killed"
        );
        assert!(Instant::now() < deadline, "search wrote nothing");
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        child.try_wait().unwrap().is_none(),
        "search finished before it could be killed; make the workload bigger"
    );
    child.kill().unwrap(); // SIGKILL on Unix
    child.wait().unwrap();
    let partial = state::load(&killed).unwrap();
    assert!(!partial.is_empty());

    // Resume, then compare with a clean run in a fresh directory.
    let args = run_args(&killed);
    palhunt(&args.iter().map(String::as_str).collect::<Vec<_>>());
    let clean = temp_dir("clean");
    let args = run_args(&clean);
    palhunt(&args.iter().map(String::as_str).collect::<Vec<_>>());

    let resumed = state::load(&killed).unwrap();
    let uninterrupted = state::load(&clean).unwrap();
    assert!(
        resumed.len() > partial.len(),
        "the resume did no work, so the kill proved nothing"
    );
    assert_same_work(&resumed, &uninterrupted);
    for (id, r) in &partial {
        assert!(resumed[id].same_work(r), "unit {id:?} changed across the restart");
    }
    // Resuming again finds everything done and appends nothing.
    let before = std::fs::read(&log_file).unwrap();
    let args = run_args(&killed);
    palhunt(&args.iter().map(String::as_str).collect::<Vec<_>>());
    assert_eq!(before, std::fs::read(&log_file).unwrap());

    for d in [killed, clean] {
        let _ = std::fs::remove_dir_all(d);
    }
}
