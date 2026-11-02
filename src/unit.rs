//! Work units: the unit of distribution, checkpointing and verification.
//!
//! A unit is a contiguous range of Gray-code indices for one bit length. It is fully defined
//! by (format version, engine, bits, unit index, unit_bits), so any machine can run the same
//! unit and must produce the same `UnitResult` (apart from timing). That is what makes results
//! checkable by re-running a sample of units elsewhere.
//!
//! The engine is fixed by the bit length: 1 to 126 bits run the u128 engine (`run_unit`,
//! `ENGINE`), 127 to 255 bits the u256 engine (`run_unit_u256`, `ENGINE_U256`). `Length` picks.

use crate::digits::{DecimalChecker, LOW_MOD, is_palindrome, is_palindrome_wide};
use crate::layout::{Layout, MAX_BITS, MAX_WIDE_BITS, layout, wide_layout};
use crate::u256::{self, U256};
use crate::wide::{Prepared, WideDecimal, run_wide_unit};
use serde::{Deserialize, Serialize};
use std::time::Instant;

/// Bump when the meaning of any field in `UnitResult` changes.
pub const FORMAT_VERSION: u32 = 1;
/// Identifies the algorithm, so results from different engines are never mixed silently.
/// The u128 engine, used for 1 to 126 bits.
pub const ENGINE: &str = "cpu-gray-u128-v1";
/// The u256 engine (four u64 limbs), used for 127 to 255 bits.
pub const ENGINE_U256: &str = "cpu-gray-u256-v1";

/// The engine whose results count for a bit length, or `None` outside 1 to 255 bits.
pub fn engine_for(bits: u32) -> Option<&'static str> {
    match bits {
        1..=MAX_BITS => Some(ENGINE),
        b if (MAX_BITS + 1..=MAX_WIDE_BITS).contains(&b) => Some(ENGINE_U256),
        _ => None,
    }
}
/// Default unit size: 2^22 (about 4 million) candidates, well under a second per unit.
pub const DEFAULT_UNIT_BITS: u32 = 22;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct UnitId {
    pub bits: u32,
    pub index: u64,
}

/// How a bit length is split into units.
pub fn unit_plan(layout: &Layout, unit_bits: u32) -> (u32, u64) {
    unit_plan_free(layout.free(), unit_bits)
}

/// How a length with `free` free classes is split into units. `free` is at most 63 up to 255
/// bits, so the unit count always fits in a u64.
pub fn unit_plan_free(free: u32, unit_bits: u32) -> (u32, u64) {
    let ub = unit_bits.min(free);
    let count = 1u64 << (free - ub);
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

/// The decimal checkers both engines need. Build once and share between units and threads.
pub struct Checkers {
    pub narrow: DecimalChecker,
    pub wide: WideDecimal<U256>,
}

impl Default for Checkers {
    fn default() -> Self {
        Self::new()
    }
}

impl Checkers {
    pub fn new() -> Self {
        Self {
            narrow: DecimalChecker::new(),
            wide: WideDecimal::new(),
        }
    }
}

/// One bit length, ready to run on the engine that `engine_for` assigns to it.
pub enum Length {
    /// 1 to 126 bits: the u128 engine.
    U128(Layout),
    /// 127 to 255 bits: the u256 engine.
    U256(Prepared<U256>),
}

impl Length {
    /// `None` when no number of this length is a base-2+16 palindrome. Panics outside 1..=255.
    pub fn new(bits: u32) -> Option<Self> {
        if bits <= MAX_BITS {
            layout(bits).map(Length::U128)
        } else {
            wide_layout(bits).map(|w| Length::U256(w.prepare()))
        }
    }

    pub fn bits(&self) -> u32 {
        match self {
            Length::U128(l) => l.bits,
            Length::U256(p) => p.bits,
        }
    }

    pub fn free(&self) -> u32 {
        match self {
            Length::U128(l) => l.free(),
            Length::U256(p) => p.free(),
        }
    }

    pub fn engine(&self) -> &'static str {
        match self {
            Length::U128(_) => ENGINE,
            Length::U256(_) => ENGINE_U256,
        }
    }

    pub fn unit_plan(&self, unit_bits: u32) -> (u32, u64) {
        unit_plan_free(self.free(), unit_bits)
    }

    pub fn run(&self, unit_bits: u32, index: u64, c: &Checkers) -> UnitResult {
        match self {
            Length::U128(l) => run_unit(l, unit_bits, index, &c.narrow),
            Length::U256(p) => run_unit_u256(p, unit_bits, index, &c.wide),
        }
    }
}

fn wide_hit(l: u256::Limbs) -> Hit {
    Hit {
        n: u256::to_decimal(l),
        hex: u256::to_hex(l),
        base12: is_palindrome_wide(l, 12),
        trivial: l[1] == 0 && l[2] == 0 && l[3] == 0 && l[0] < 16,
    }
}

/// Run one unit on the u256 engine. Same loop, same fields, same meaning as `run_unit`; every
/// hit has been confirmed by `digits::is_palindrome_wide` in base 2, 10 and 16. Production uses
/// it for 127 to 255 bits; the tests also run it below 127 bits to prove it equals `run_unit`.
pub fn run_unit_u256(
    layout: &Prepared<U256>,
    unit_bits: u32,
    index: u64,
    dec: &WideDecimal<U256>,
) -> UnitResult {
    let started = Instant::now();
    let (ub, units) = unit_plan_free(layout.free(), unit_bits);
    assert!(index < units, "unit {index} out of range ({units} units)");
    let r = run_wide_unit(layout, dec, unit_bits, index as u128);
    UnitResult {
        v: FORMAT_VERSION,
        engine: ENGINE_U256.to_string(),
        bits: layout.bits,
        unit: index,
        units,
        unit_bits: ub,
        candidates: r.candidates,
        pass1: r.pass1,
        fold: format!("{:016x}", r.fold),
        hits: r.hits.into_iter().map(wide_hit).collect(),
        elapsed_ms: started.elapsed().as_millis() as u64,
    }
}
