//! The wide engine: the hot loop of `unit::run_unit`, written once over any integer type that
//! implements `Wide`. Production runs it with `U256` for 127 to 255 bits (engine
//! `cpu-gray-u256-v1`); the tests also run it with `u128` and, at 126 bits and below, compare it
//! with the u128 engine unit by unit. The benchmark (`benches/wide_int.rs`) times this exact code.
//!
//! Operations, identical to `run_unit` and `DecimalChecker`:
//!
//! 1. Gray-code step: `n ^= masks[tz(i)]`, after testing `n & mask != 0` to pick add or subtract.
//! 2. Residue update: `low = (low +/- mask_mod[j]) mod 10^6`, plain u64 arithmetic.
//! 3. Digit length: `n >= 10^d` to pick between two decimal lengths.
//! 4. Pass-1 reject: the table-driven decimal digit-pair compare (`row[k] > rest`, `rest -= row[k]`).
//! 5. Fold: `fold = (rotl(fold, 5) ^ every 64-bit limb of n) * 0x9E3779B97F4A7C15`. Limbs above
//!    bit 127 are zero below 128 bits, so the fold equals the u128 engine's there.
//!
//! The loop does not allocate and uses no floating point. The only allocations are in setup
//! (`Prepared`, `WideDecimal`) and on the cold path that confirms a hit with the reference
//! `digits::is_palindrome_wide`.

use crate::digits::{LOW_DIGITS, LOW_MOD, is_palindrome_wide};
use crate::layout::WideLayout;
use crate::u256::{Limbs, bit, mul_small, rem_small};

/// 2^255 < 10^77 < 2^256, so every candidate of up to 255 bits has at most 77 decimal digits.
pub const MAX_DIGITS: u32 = 77;

/// What the hot loop needs from an integer type (implemented for `U256` in `src/u256.rs`). Each method is one primitive step; an
/// implementation must be pure limb arithmetic (no allocation, no dispatch).
pub trait Wide: Copy {
    const NAME: &'static str;
    fn from_limbs(l: Limbs) -> Self;
    fn to_limbs(self) -> Limbs;
    fn xor(self, o: Self) -> Self;
    /// `self & o != 0`.
    fn intersects(self, o: Self) -> bool;
    /// `self < o`.
    fn less(self, o: Self) -> bool;
    /// `self - o`; the caller guarantees `self >= o`.
    fn sub(self, o: Self) -> Self;
}

/// Native u128, only valid below 128 bits. Used by tests to cross-check, never by `search`.
impl Wide for u128 {
    const NAME: &'static str = "native u128";

    #[inline(always)]
    fn from_limbs(l: Limbs) -> Self {
        debug_assert!(l[2] == 0 && l[3] == 0);
        (l[0] as u128) | ((l[1] as u128) << 64)
    }
    #[inline(always)]
    fn to_limbs(self) -> Limbs {
        [self as u64, (self >> 64) as u64, 0, 0]
    }
    #[inline(always)]
    fn xor(self, o: Self) -> Self {
        self ^ o
    }
    #[inline(always)]
    fn intersects(self, o: Self) -> bool {
        self & o != 0
    }
    #[inline(always)]
    fn less(self, o: Self) -> bool {
        self < o
    }
    #[inline(always)]
    fn sub(self, o: Self) -> Self {
        self - o
    }
}

/// Allocation-free exact decimal palindrome test (stack buffer, repeated division by 10^19).
/// The fallback once the six tracked low digits are used up; it runs about once per million
/// candidates. It only decides whether a candidate is passed on: a `true` here is still
/// confirmed by the reference before it counts as a hit.
pub fn decimal_palindrome_noalloc(mut l: Limbs) -> bool {
    const CHUNK: u128 = 10_000_000_000_000_000_000; // 10^19
    let mut digits = [0u8; 96]; // 5 chunks of 19 cover 77 digits
    let mut len = 0usize;
    while l != [0; 4] {
        let mut rem = 0u128;
        for i in (0..4).rev() {
            let cur = (rem << 64) | l[i] as u128;
            l[i] = (cur / CHUNK) as u64;
            rem = cur % CHUNK;
        }
        let mut r = rem as u64;
        for _ in 0..19 {
            digits[len] = (r % 10) as u8;
            r /= 10;
            len += 1;
        }
    }
    while len > 0 && digits[len - 1] == 0 {
        len -= 1;
    }
    (0..len / 2).all(|i| digits[i] == digits[len - 1 - i])
}

impl WideLayout {
    /// Convert to one integer type, with the per-mask residues precomputed so that the unit
    /// loop itself allocates nothing.
    pub fn prepare<T: Wide>(&self) -> Prepared<T> {
        Prepared {
            bits: self.bits,
            base: T::from_limbs(self.base),
            masks: self.masks.iter().map(|&m| T::from_limbs(m)).collect(),
            mask_mod: self.masks.iter().map(|&m| rem_small(m, LOW_MOD)).collect(),
        }
    }
}

/// A layout converted to one integer type.
#[derive(Debug, Clone)]
pub struct Prepared<T> {
    pub bits: u32,
    pub base: T,
    pub masks: Vec<T>,
    pub mask_mod: Vec<u64>,
}

impl<T: Wide> Prepared<T> {
    pub fn free(&self) -> u32 {
        self.masks.len() as u32
    }

    /// The number at Gray-code index `g`. Masks are disjoint from each other and from `base`,
    /// so xor is the same as or.
    pub fn value_at(&self, g: u128) -> T {
        let mut n = self.base;
        for (j, &m) in self.masks.iter().enumerate() {
            if (g >> j) & 1 == 1 {
                n = n.xor(m);
            }
        }
        n
    }
}

/// Decimal checker: the same tables and scan as `digits::DecimalChecker`, over `T`.
pub struct WideDecimal<T> {
    /// pow10[p] = 10^p, p in 0..=77.
    pub pow10: Vec<T>,
    /// mul[p][k] = k * 10^p, k in 0..=9, p in 0..=76.
    mul: Vec<[T; 10]>,
}

impl<T: Wide> Default for WideDecimal<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Wide> WideDecimal<T> {
    pub fn new() -> Self {
        let mut p10 = vec![[1u64, 0, 0, 0]];
        for p in 1..=MAX_DIGITS as usize {
            let prev = p10[p - 1];
            p10.push(mul_small(prev, 10));
        }
        let mul = (0..MAX_DIGITS as usize)
            .map(|p| std::array::from_fn(|k| T::from_limbs(mul_small(p10[p], k as u64))))
            .collect();
        Self {
            pow10: p10.into_iter().map(T::from_limbs).collect(),
            mul,
        }
    }

    /// Decimal digit count of `n`, for 1 <= n < 2^256.
    #[inline]
    pub fn digits(&self, n: T) -> u32 {
        let mut d = 1;
        while d < MAX_DIGITS && !n.less(self.pow10[d as usize]) {
            d += 1;
        }
        d
    }

    /// Is `n` a decimal palindrome? `low` must equal `n % LOW_MOD`, `d` the digit count.
    /// `first_pair` is set when the outermost digit pair matched. Same contract as
    /// `DecimalChecker::is_palindrome`.
    #[inline]
    pub fn is_palindrome(&self, n: T, low: u64, d: u32, first_pair: &mut bool) -> bool {
        *first_pair = false;
        let pairs = d / 2;
        let mut rest = n;
        let mut low_rest = low;
        for i in 0..pairs {
            let pos = (d - 1 - i) as usize;
            let row = &self.mul[pos];
            // Largest k with k * 10^pos <= rest (row[k] > rest  <=>  rest < row[k]). Entry 0
            // is zero, so the scan always stops.
            let mut k = 9;
            while rest.less(row[k]) {
                k -= 1;
            }
            rest = rest.sub(row[k]);
            if i >= LOW_DIGITS {
                // Past the tracked residue: finish with the exact allocation-free check.
                return decimal_palindrome_noalloc(n.to_limbs());
            }
            let low_digit = (low_rest % 10) as usize;
            low_rest /= 10;
            if low_digit != k {
                return false;
            }
            if i == 0 {
                *first_pair = true;
            }
        }
        if pairs == 0 {
            *first_pair = true;
        }
        true
    }
}

/// What one unit produced, before it is turned into a `UnitResult`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WideUnitResult {
    pub bits: u32,
    pub unit_bits: u32,
    pub unit: u128,
    pub candidates: u64,
    pub pass1: u64,
    pub fold: u64,
    pub hits: Vec<Limbs>,
}

/// Run one unit: every candidate in Gray-code order, exactly as `run_unit` does.
pub fn run_wide_unit<T: Wide>(
    layout: &Prepared<T>,
    dec: &WideDecimal<T>,
    unit_bits: u32,
    index: u128,
) -> WideUnitResult {
    let ub = unit_bits.min(layout.free());
    let units = 1u128 << (layout.free() - ub);
    assert!(index < units, "unit {index} out of range ({units} units)");
    let start = index << ub;
    let end = start + (1u128 << ub);

    let gray = |i: u128| i ^ (i >> 1);
    let mut n = layout.value_at(gray(start));
    let mut low = rem_small(n.to_limbs(), LOW_MOD);
    // Every candidate has the same top bit, so at most two decimal lengths occur.
    let lo = T::from_limbs(bit(layout.bits - 1));
    let d_lo = dec.digits(lo);
    let ten_d_lo = dec.pow10[d_lo as usize];

    let (mut pass1, mut fold) = (0u64, 0u64);
    let mut hits = Vec::new();
    let mut i = start;
    loop {
        let d = if n.less(ten_d_lo) { d_lo } else { d_lo + 1 };
        let mut first = false;
        if dec.is_palindrome(n, low, d, &mut first) {
            let l = n.to_limbs();
            // Cold path (allocates): confirm with the reference in every base before believing it.
            assert!(is_palindrome_wide(l, 2) && is_palindrome_wide(l, 16) && is_palindrome_wide(l, 10));
            hits.push(l);
        }
        if first {
            pass1 += 1;
            let l = n.to_limbs();
            fold = fold.rotate_left(5) ^ l[0] ^ l[1] ^ l[2] ^ l[3];
            fold = fold.wrapping_mul(0x9E37_79B9_7F4A_7C15);
        }
        i += 1;
        if i == end {
            break;
        }
        // Gray code: index i differs from i-1 in exactly one mask, the one at tz(i).
        let j = i.trailing_zeros() as usize;
        let m = layout.masks[j];
        if n.intersects(m) {
            n = n.xor(m);
            low = (low + LOW_MOD - layout.mask_mod[j]) % LOW_MOD;
        } else {
            n = n.xor(m);
            low = (low + layout.mask_mod[j]) % LOW_MOD;
        }
    }
    WideUnitResult {
        bits: layout.bits,
        unit_bits: ub,
        unit: index,
        candidates: (end - start) as u64,
        pass1,
        fold,
        hits,
    }
}
