//! Local checkpoint: an append-only JSON Lines log of finished units.
//!
//! Each finished unit is one line, flushed and synced before the next is written, so a crash or
//! a closed laptop loses at most the units in flight. A torn final line is ignored on load. The
//! same records are what Phase 2 uploads to the coordinator.

use crate::unit::{ENGINE, FORMAT_VERSION, UnitId, UnitResult};
use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

pub struct Log {
    path: PathBuf,
    file: File,
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
    for line in BufReader::new(File::open(&path)?).lines() {
        let line = line?;
        // A torn last line (crash mid-write) fails to parse and is skipped.
        let Ok(r) = serde_json::from_str::<UnitResult>(&line) else {
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
        let mut file = OpenOptions::new().create(true).append(true).open(&path)?;
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
        Ok(Self { path, file })
    }

    pub fn append(&mut self, r: &UnitResult) -> std::io::Result<()> {
        let mut line = serde_json::to_string(r).expect("UnitResult serialises");
        line.push('\n');
        self.file.write_all(line.as_bytes())?;
        self.file.sync_data()
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}
