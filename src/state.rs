//! Local checkpoint: an append-only JSON Lines log of finished units.
//!
//! Each finished unit is one line. Lines are written in batches (group commit): a writer thread
//! collects the units that finish within `CommitPolicy::interval` (or `max_units`, whichever
//! comes first), appends them with one write and makes them durable with one sync. A crash or a
//! closed laptop therefore loses the units finished since the last sync, which are simply redone
//! on restart. Damaged lines (torn, zero-filled or non-UTF-8) are skipped on load. The same
//! records are what Phase 2 uploads to the coordinator.

use crate::unit::{ENGINE, FORMAT_VERSION, UnitId, UnitResult};
use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

pub struct Log {
    path: PathBuf,
    file: File,
    syncs: u64,
}

/// When the writer closes a batch and syncs it. This is the durability window: finished work
/// that has not reached a sync yet is lost by a crash and redone on restart.
#[derive(Debug, Clone, Copy)]
pub struct CommitPolicy {
    /// Longest a finished unit waits for company before its batch is synced.
    pub interval: Duration,
    /// A batch is synced as soon as it holds this many units.
    pub max_units: usize,
}

impl Default for CommitPolicy {
    fn default() -> Self {
        Self {
            interval: Duration::from_millis(200),
            max_units: 256,
        }
    }
}

pub fn log_path(dir: &Path) -> PathBuf {
    dir.join("results.jsonl")
}

/// Every complete, current-format record in the log, keyed by unit.
pub fn load(dir: &Path) -> std::io::Result<HashMap<UnitId, UnitResult>> {
    let mut done = HashMap::new();
    let path = log_path(dir);
    if !path.exists() {
        return Ok(done);
    }
    let mut reader = BufReader::new(File::open(&path)?);
    let mut line = Vec::new();
    loop {
        line.clear();
        if reader.read_until(b'\n', &mut line)? == 0 {
            break;
        }
        // Anything a crash can leave (a torn line, zero-filled or non-UTF-8 bytes) fails to
        // parse and is skipped; it never aborts the load.
        let Ok(r) = serde_json::from_slice::<UnitResult>(&line) else {
            continue;
        };
        if r.v == FORMAT_VERSION && r.engine == ENGINE {
            done.insert(
                UnitId {
                    bits: r.bits,
                    index: r.unit,
                },
                r,
            );
        }
    }
    Ok(done)
}

impl Log {
    pub fn open(dir: &Path) -> std::io::Result<Self> {
        fs::create_dir_all(dir)?;
        let path = log_path(dir);
        let existed = path.exists();
        let mut file = OpenOptions::new().create(true).append(true).open(&path)?;
        if !existed {
            sync_dir(dir)?; // make the new file's directory entry durable too
        }
        // If the previous run died mid-line, start ours on a fresh line.
        if file.metadata()?.len() > 0 {
            let mut last = [0u8; 1];
            let mut reader = File::open(&path)?;
            reader.seek(SeekFrom::End(-1))?;
            reader.read_exact(&mut last)?;
            if last[0] != b'\n' {
                file.write_all(b"\n")?;
            }
        }
        Ok(Self { path, file, syncs: 0 })
    }

    /// Append one record and sync it.
    pub fn append(&mut self, r: &UnitResult) -> std::io::Result<()> {
        self.append_batch(std::slice::from_ref(r))
    }

    /// Append records with a single write and a single sync. When this returns, every record
    /// in `batch` is durable.
    pub fn append_batch(&mut self, batch: &[UnitResult]) -> std::io::Result<()> {
        if batch.is_empty() {
            return Ok(());
        }
        let mut buf = String::new();
        for r in batch {
            buf.push_str(&serde_json::to_string(r).expect("UnitResult serialises"));
            buf.push('\n');
        }
        self.file.write_all(buf.as_bytes())?;
        self.file.sync_data()?;
        self.syncs += 1;
        Ok(())
    }

    /// How many syncs this handle has issued.
    pub fn syncs(&self) -> u64 {
        self.syncs
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

#[cfg(unix)]
fn sync_dir(dir: &Path) -> std::io::Result<()> {
    File::open(dir)?.sync_all()
}

#[cfg(not(unix))]
fn sync_dir(_dir: &Path) -> std::io::Result<()> {
    Ok(()) // Windows cannot open a directory as a file; NTFS journals the entry.
}

/// Group commit. Receives finished units until every sender is gone, calls `on_result` for each
/// one as it arrives, and appends them to `log` in batches: a batch closes `policy.interval`
/// after its first unit arrives or at `policy.max_units`, then costs one write and one sync.
/// Units that finish while a sync is running join the next batch, so the workers never wait for
/// the disk. Returns once everything received is durable, or at the first write error (dropping
/// the receiver, which tells the senders to stop).
pub fn commit_loop(
    log: &mut Log,
    rx: Receiver<UnitResult>,
    policy: CommitPolicy,
    mut on_result: impl FnMut(&UnitResult),
) -> std::io::Result<()> {
    let max_units = policy.max_units.max(1);
    let mut batch: Vec<UnitResult> = Vec::with_capacity(max_units);
    let mut senders_gone = false;
    while !senders_gone {
        let Ok(first) = rx.recv() else { break };
        on_result(&first);
        batch.push(first);
        let deadline = Instant::now() + policy.interval;
        while batch.len() < max_units {
            let left = deadline.saturating_duration_since(Instant::now());
            match rx.recv_timeout(left) {
                Ok(r) => {
                    on_result(&r);
                    batch.push(r);
                }
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => {
                    senders_gone = true;
                    break;
                }
            }
        }
        log.append_batch(&batch)?;
        batch.clear();
    }
    Ok(())
}
