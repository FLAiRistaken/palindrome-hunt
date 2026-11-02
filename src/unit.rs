//! Work units: the unit of distribution, checkpointing and verification.
//!
//! A unit is a contiguous range of Gray-code indices for one bit length. It is fully defined
//! by (format version, bits, unit index, unit_bits), so any machine can run the same unit and
//! must produce the same `UnitResult` (apart from timing). That is what makes results
//! checkable by re-running a sample of units elsewhere.

use crate::digits::{DecimalChecker, LOW_MOD, is_palindrome};
use crate::layout::Layout;
use serde::{Deserialize, Serialize};
use std::time::Instant;

/// Bump when the meaning of any field in `UnitResult` changes.
pub const FORMAT_VERSION: u32 = 1;
/// Identifies the algorithm, so results from different engines are never mixed silently.
pub const ENGINE: &str = "cpu-gray-u128-v1";
/// Default unit size: 2^22 (about 4 million) candidates, well under a second per unit.
pub const DEFAULT_UNIT_BITS: u32 = 22;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct UnitId {
    pub bits: u32,
    pub index: u64,
}

/// How a bit length is split into units.
pub fn unit_plan(layout: &Layout, unit_bits: u32) -> (u32, u64) {
    let ub = unit_bits.min(layout.free());
    let count = 1u64 << (layout.free() - ub);
    (ub, count)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hit {
    /// Decimal string (JSON numbers can't hold u128).
    pub n: String,
    pub hex: String,
    pub base12: bool,
    /// Single digit in base 10 or base 16: palindromic for free, not what we're looking for.
    pub trivial: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnitResult {
    pub v: u32,
    pub engine: String,
    pub bits: u32,
    pub unit: u64,
    pub units: u64,
    pub unit_bits: u32,
    /// Candidates tested (must equal 2^unit_bits).
    pub candidates: u64,
    /// Candidates whose outer decimal digits matched.
    pub pass1: u64,
    /// Order-dependent hash of every candidate that passed the first pair, as hex.
    /// A machine that skipped work can't produce it.
    pub fold: String,
    pub hits: Vec<Hit>,
    /// Wall time; excluded from comparisons between machines.
    #[serde(default)]
    pub elapsed_ms: u64,
}

impl UnitResult {
    /// Equality ignoring timing: what two machines must agree on.
    pub fn same_work(&self, other: &UnitResult) -> bool {
        let mut a = self.clone();
        let mut b = other.clone();
        a.elapsed_ms = 0;
        b.elapsed_ms = 0;
        a == b
    }
}

fn hit(n: u128) -> Hit {
    Hit {
        n: n.to_string(),
        hex: format!("{n:x}"),
        base12: is_palindrome(n, 12),
        trivial: n < 16,
    }
}

/// Run one unit: test every candidate in Gray-code order.
pub fn run_unit(layout: &Layout, unit_bits: u32, index: u64, dec: &DecimalChecker) -> UnitResult {
    let started = Instant::now();
    let (ub, units) = unit_plan(layout, unit_bits);
    assert!(index < units, "unit {index} out of range ({units} units)");
    let start = (index as u128) << ub;
    let end = start + (1u128 << ub);

    let gray = |i: u128| i ^ (i >> 1);
    let mut n = layout.value_at(gray(start));
    let mask_mod: Vec<u64> = layout
        .masks
        .iter()
        .map(|m| (m % LOW_MOD as u128) as u64)
        .collect();
    let mut low = (n % LOW_MOD as u128) as u64;
    // Every candidate has the same top bit, so at most two decimal lengths occur.
    let lo = 1u128 << (layout.bits - 1);
    let d_lo = dec.digits(lo);
    let ten_d_lo = 10u128.pow(d_lo);

    let (mut pass1, mut fold) = (0u64, 0u64);
    let mut hits = Vec::new();
    let mut i = start;
    loop {
        let d = if n >= ten_d_lo { d_lo + 1 } else { d_lo };
        let mut first = false;
        if dec.is_palindrome(n, low, d, &mut first) {
            // Confirm with the reference in every base before believing it.
            assert!(is_palindrome(n, 2) && is_palindrome(n, 16) && is_palindrome(n, 10));
            hits.push(hit(n));
        }
        if first {
            pass1 += 1;
            fold = fold.rotate_left(5) ^ (n as u64) ^ ((n >> 64) as u64);
            fold = fold.wrapping_mul(0x9E37_79B9_7F4A_7C15);
        }
        i += 1;
        if i == end {
            break;
        }
        // Gray code: index i differs from i-1 in exactly one mask, the one at tz(i).
        let j = i.trailing_zeros() as usize;
        let m = layout.masks[j];
        if n & m != 0 {
            n ^= m;
            low = (low + LOW_MOD - mask_mod[j]) % LOW_MOD;
        } else {
            n ^= m;
            low = (low + mask_mod[j]) % LOW_MOD;
        }
    }
    UnitResult {
        v: crate::unit::FORMAT_VERSION,
        engine: ENGINE.to_string(),
        bits: layout.bits,
        unit: index,
        units,
        unit_bits: ub,
        candidates: (end - start) as u64,
        pass1,
        fold: format!("{fold:016x}"),
        hits,
        elapsed_ms: started.elapsed().as_millis() as u64,
    }
}
