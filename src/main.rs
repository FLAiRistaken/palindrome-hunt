use clap::{Parser, Subcommand};
use palhunt::digits::{DecimalChecker, is_palindrome};
use palhunt::layout::{MAX_BITS, layout};
use palhunt::unit::{DEFAULT_UNIT_BITS, UnitId, UnitResult, run_unit, unit_plan};
use palhunt::{frontier, state, status};
use rayon::prelude::*;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::mpsc;
use std::time::Instant;

/// Hunt for numbers that are palindromes in base 2, base 10 and base 16 at once.
#[derive(Parser)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Search every bit length in a range, resuming from the state directory.
    Search {
        #[arg(long, default_value_t = 1)]
        from_bits: u32,
        #[arg(long)]
        to_bits: u32,
        /// log2 of candidates per work unit.
        #[arg(long, default_value_t = DEFAULT_UNIT_BITS)]
        unit_bits: u32,
        #[arg(long, default_value = "palhunt-state")]
        state_dir: PathBuf,
        /// Worker threads (default: all logical cores).
        #[arg(long)]
        threads: Option<usize>,
    },
    /// Show progress, the verified frontier and every hit in the state directory.
    Status {
        #[arg(long, default_value_t = MAX_BITS)]
        to_bits: u32,
        #[arg(long, default_value_t = DEFAULT_UNIT_BITS)]
        unit_bits: u32,
        #[arg(long, default_value = "palhunt-state")]
        state_dir: PathBuf,
    },
    /// Re-run logged units and check they produce identical results.
    Verify {
        /// Re-run this many logged units, spread evenly across the log.
        #[arg(long, default_value_t = 20)]
        sample: usize,
        #[arg(long, default_value = "palhunt-state")]
        state_dir: PathBuf,
    },
    /// Independent check: brute-force every number up to 2^to_bits and compare with the
    /// enumerator's search space. Slow; meant for small bit lengths.
    Brute {
        #[arg(long, default_value_t = 28)]
        to_bits: u32,
    },
}

/// Refuse lengths whose unit count would make the local plan unmanageable: those are
/// coordinator territory (Phase 2) or need the pruning engine (Phase 3).
const MAX_LOCAL_UNITS: u64 = 1 << 24;

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.cmd {
        Cmd::Search {
            from_bits,
            to_bits,
            unit_bits,
            state_dir,
            threads,
        } => search(from_bits, to_bits, unit_bits, state_dir, threads),
        Cmd::Status {
            to_bits,
            unit_bits,
            state_dir,
        } => show_status(to_bits, unit_bits, state_dir),
        Cmd::Verify { sample, state_dir } => verify(sample, state_dir),
        Cmd::Brute { to_bits } => brute(to_bits),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

type Res = Result<(), String>;

fn check_bits(from: u32, to: u32) -> Res {
    if from < 1 || to > MAX_BITS || from > to {
        return Err(format!("bit range must be within 1..={MAX_BITS} and from <= to"));
    }
    Ok(())
}

fn search(from: u32, to: u32, unit_bits: u32, dir: PathBuf, threads: Option<usize>) -> Res {
    check_bits(from, to)?;
    if let Some(t) = threads {
        rayon::ThreadPoolBuilder::new()
            .num_threads(t)
            .build_global()
            .map_err(|e| e.to_string())?;
    }
    let done = state::load(&dir).map_err(|e| e.to_string())?;
    let mut log = state::Log::open(&dir).map_err(|e| e.to_string())?;
    let dec = DecimalChecker::new();
    println!(
        "palhunt {}: bits {from}..={to}, {} threads, log {}",
        env!("CARGO_PKG_VERSION"),
        rayon::current_num_threads(),
        log.path().display()
    );
    let started = Instant::now();
    let mut tested: u64 = 0;
    for bits in from..=to {
        let Some(l) = layout(bits) else {
            continue;
        };
        let (ub, units) = unit_plan(&l, unit_bits);
        if units > MAX_LOCAL_UNITS {
            return Err(format!(
                "{bits} bits needs {units} units of 2^{ub}; that is beyond a local run. \
                 Stopping here (frontier work continues in later phases)."
            ));
        }
        let todo: Vec<u64> = (0..units)
            .filter(|&i| !done.contains_key(&UnitId { bits, index: i }))
            .collect();
        if todo.is_empty() {
            continue;
        }
        let t0 = Instant::now();
        let (tx, rx) = mpsc::channel::<UnitResult>();
        let mut write_err = None;
        std::thread::scope(|s| {
            let (l, dec, todo) = (&l, &dec, &todo);
            s.spawn(move || {
                // Stops early if the log writer gave up (the receiver is dropped).
                let _ = todo.par_iter().try_for_each_with(tx, |tx, &i| {
                    tx.send(run_unit(l, unit_bits, i, dec)).map_err(|_| ())
                });
            });
            // Group commit: this thread batches finished units and syncs each batch once, while
            // the workers keep computing.
            write_err = state::commit_loop(&mut log, rx, state::CommitPolicy::default(), |r| {
                tested += r.candidates;
                for h in &r.hits {
                    let tag = if h.trivial { "trivial" } else { "NON-TRIVIAL HIT" };
                    println!(
                        "  {tag}: {} (hex {}, base 12 palindrome: {})",
                        h.n, h.hex, h.base12
                    );
                }
            })
            .err();
        });
        if let Some(e) = write_err {
            return Err(format!("could not write the log: {e}"));
        }
        let secs = t0.elapsed().as_secs_f64();
        let cands = (todo.len() as f64) * (1u64 << ub) as f64;
        println!(
            "bits {bits:>3}: {} units of 2^{ub}, {:.3e} candidates in {secs:.2}s ({:.1} M/s)",
            todo.len(),
            cands,
            cands / secs.max(1e-9) / 1e6
        );
    }
    println!(
        "done: {tested} candidates in {:.1}s. Run `palhunt status` for the verified frontier.",
        started.elapsed().as_secs_f64()
    );
    Ok(())
}

fn show_status(to: u32, unit_bits: u32, dir: PathBuf) -> Res {
    check_bits(1, to)?;
    let done = state::load(&dir).map_err(|e| e.to_string())?;
    let st = status(&done, to, unit_bits);
    let f = frontier(&st);
    let candidates: u64 = done.values().map(|r| r.candidates).sum();
    println!("{} units logged, {candidates} candidates tested", done.len());
    for s in st.iter().filter(|s| s.done > 0 && !s.complete()) {
        println!("  bits {}: {}/{} units", s.bits, s.done, s.units);
    }
    if f == 0 {
        println!("verified frontier: none yet");
    } else {
        println!("verified frontier: every base-2+16 palindrome below 2^{f} has been tested");
    }
    let mut hits: Vec<_> = done.values().flat_map(|r| r.hits.iter()).collect();
    hits.sort_by_key(|h| h.n.parse::<u128>().unwrap_or(0));
    let non_trivial = hits.iter().filter(|h| !h.trivial).count();
    let shown: Vec<_> = hits.iter().map(|h| h.n.as_str()).collect();
    println!(
        "hits: {} ({non_trivial} non-trivial): {}",
        hits.len(),
        shown.join(", ")
    );
    Ok(())
}

fn verify(sample: usize, dir: PathBuf) -> Res {
    let done = state::load(&dir).map_err(|e| e.to_string())?;
    if done.is_empty() {
        return Err("nothing logged yet".into());
    }
    let mut ids: Vec<_> = done.keys().copied().collect();
    ids.sort_by_key(|u| (u.bits, u.index));
    let step = (ids.len() / sample.max(1)).max(1);
    let picked: Vec<_> = ids.iter().step_by(step).take(sample).collect();
    let dec = DecimalChecker::new();
    let mut bad = 0;
    for id in &picked {
        let logged = &done[id];
        let l = layout(id.bits).ok_or("logged unit for an empty bit length")?;
        let again = run_unit(&l, logged.unit_bits, id.index, &dec);
        if again.same_work(logged) {
            println!("ok   bits {} unit {} fold {}", id.bits, id.index, again.fold);
        } else {
            println!(
                "FAIL bits {} unit {}: logged fold {}, recomputed {}",
                id.bits, id.index, logged.fold, again.fold
            );
            bad += 1;
        }
    }
    if bad > 0 {
        return Err(format!("{bad} of {} units did not reproduce", picked.len()));
    }
    println!("{} units reproduced exactly", picked.len());
    Ok(())
}

fn brute(to: u32) -> Res {
    if !(1..=36).contains(&to) {
        return Err("brute force is for small lengths: --to-bits must be in 1..=36".into());
    }
    let mut ok = true;
    for bits in 1..=to {
        let lo = 1u128 << (bits - 1);
        let hi = 1u128 << bits;
        // Plain loop over every odd number of this length, reference checks only.
        let brute: Vec<u128> = (lo..hi)
            .into_par_iter()
            .filter(|&n| (n & 1 == 1 || n == 0) && is_palindrome(n, 2) && is_palindrome(n, 16))
            .collect();
        let enumerated: Vec<u128> = match layout(bits) {
            None => vec![],
            Some(l) => {
                let mut v: Vec<u128> = (0..1u128 << l.free()).map(|g| l.value_at(g)).collect();
                v.sort_unstable();
                v
            }
        };
        let mut brute_sorted = brute;
        brute_sorted.sort_unstable();
        let same = brute_sorted == enumerated;
        ok &= same;
        let dec_hits = brute_sorted.iter().filter(|&&n| is_palindrome(n, 10)).count();
        println!(
            "bits {bits:>2}: brute {:>7}, enumerator {:>7}, {} | also decimal palindromes: {dec_hits}",
            brute_sorted.len(),
            enumerated.len(),
            if same { "match" } else { "MISMATCH" }
        );
    }
    if ok {
        println!("enumerator matches brute force for every length up to {to} bits");
        Ok(())
    } else {
        Err("enumerator and brute force disagree".into())
    }
}
