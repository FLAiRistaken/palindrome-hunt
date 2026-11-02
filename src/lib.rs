//! Search for numbers that are palindromes in base 2, base 10 and base 16 at once.
//!
//! See `docs/SPEC.md` for the maths and the search-space reduction.

pub mod digits;
pub mod layout;
pub mod state;
pub mod u256;
pub mod unit;
pub mod wide;

use layout::wide_layout;
use std::collections::HashMap;
use unit::{UnitId, UnitResult, unit_plan_free};

/// Progress of one bit length in a set of results.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BitsStatus {
    pub bits: u32,
    /// 0 when no number of this length is a base-2+16 palindrome.
    pub free: u32,
    pub units: u64,
    pub done: u64,
}

impl BitsStatus {
    pub fn complete(&self) -> bool {
        self.done == self.units
    }
}

/// Status of every bit length from 1 to `to_bits`, given results logged with `unit_bits`.
/// Only records whose `unit_bits` matches this plan count.
pub fn status(done: &HashMap<UnitId, UnitResult>, to_bits: u32, unit_bits: u32) -> Vec<BitsStatus> {
    let mut counts: HashMap<u32, u64> = HashMap::new();
    for r in done.values() {
        // Lengths outside the report are never read; skipping them also keeps a stray record
        // (bits 0 or past the widest engine) from reaching the layout.
        if !(1..=to_bits).contains(&r.bits) {
            continue;
        }
        if let Some(l) = wide_layout(r.bits) {
            let (ub, units) = unit_plan_free(l.free(), unit_bits);
            if r.unit_bits == ub && r.units == units && r.unit < units {
                *counts.entry(r.bits).or_default() += 1;
            }
        }
    }
    (1..=to_bits)
        .map(|bits| match wide_layout(bits) {
            None => BitsStatus {
                bits,
                free: 0,
                units: 0,
                done: 0,
            },
            Some(l) => {
                let (_, units) = unit_plan_free(l.free(), unit_bits);
                let done = counts.get(&bits).copied().unwrap_or(0);
                BitsStatus {
                    bits,
                    free: l.free(),
                    units,
                    done,
                }
            }
        })
        .collect()
}

/// Largest L such that every length 1..=L is complete (0 if length 1 isn't).
pub fn frontier(statuses: &[BitsStatus]) -> u32 {
    statuses
        .iter()
        .take_while(|s| s.complete())
        .last()
        .map_or(0, |s| s.bits)
}
