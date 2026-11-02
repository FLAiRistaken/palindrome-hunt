//! Slow, obviously-correct reference for the wide types.
//!
//! Shares no code with the hot loop or with either candidate type: digits in any base come from
//! double-and-add over a little-endian digit vector (no division, no limb tricks), values come
//! from OR-ing masks, and a whole unit is replayed one candidate at a time.

use super::engine::{Limbs, WideLayout, WideUnitResult};

/// Little-endian digits of `l` in `base` (empty for zero), built by doubling from the top bit.
pub fn digits_le(l: Limbs, base: u32) -> Vec<u32> {
    let mut digits: Vec<u32> = Vec::new();
    for bit in (0..256).rev() {
        let mut carry = ((l[bit / 64] >> (bit % 64)) & 1) as u32;
        for d in digits.iter_mut() {
            let v = *d * 2 + carry;
            *d = v % base;
            carry = v / base;
        }
        if carry > 0 {
            digits.push(carry);
        }
    }
    digits
}

/// Is `l` a palindrome in `base`? Zero counts as one (same as `digits::is_palindrome`).
pub fn is_palindrome(l: Limbs, base: u32) -> bool {
    let d = digits_le(l, base);
    d.iter().eq(d.iter().rev())
}

pub fn to_decimal(l: Limbs) -> String {
    let d = digits_le(l, 10);
    if d.is_empty() {
        return "0".into();
    }
    d.iter().rev().map(|&x| char::from(b'0' + x as u8)).collect()
}

pub fn parse_decimal(s: &str) -> Limbs {
    let mut l = [0u64; 4];
    for c in s.bytes() {
        assert!(c.is_ascii_digit());
        let mut carry = (c - b'0') as u128;
        for limb in l.iter_mut() {
            let t = *limb as u128 * 10 + carry;
            *limb = t as u64;
            carry = t >> 64;
        }
        assert_eq!(carry, 0, "value does not fit in 256 bits");
    }
    l
}

pub fn bit_length(l: Limbs) -> u32 {
    (0..4)
        .rev()
        .find(|&i| l[i] != 0)
        .map_or(0, |i| 64 * i as u32 + 64 - l[i].leading_zeros())
}

/// The number at Gray-code index `g`: base OR the masks selected by the bits of `g`.
pub fn value_at(layout: &WideLayout, g: u128) -> Limbs {
    let mut n = layout.base;
    for (j, m) in layout.masks.iter().enumerate() {
        if (g >> j) & 1 == 1 {
            for k in 0..4 {
                n[k] |= m[k];
            }
        }
    }
    n
}

/// Replay one unit candidate by candidate. Every candidate is rebuilt from its Gray-code index
/// (no incremental step) and checked in base 2 and 16 as well, so this also proves the layout.
pub fn run_unit(layout: &WideLayout, unit_bits: u32, index: u128) -> WideUnitResult {
    let ub = unit_bits.min(layout.free());
    let start = index << ub;
    let end = start + (1u128 << ub);
    let (mut pass1, mut fold) = (0u64, 0u64);
    let mut hits = Vec::new();
    for i in start..end {
        let n = value_at(layout, i ^ (i >> 1));
        assert_eq!(bit_length(n), layout.bits, "bits {} index {i}", layout.bits);
        assert!(
            is_palindrome(n, 2) && is_palindrome(n, 16),
            "bits {} index {i}",
            layout.bits
        );
        let dec = digits_le(n, 10);
        let d = dec.len();
        let first = d == 1 || dec[0] == dec[d - 1];
        if dec.iter().eq(dec.iter().rev()) {
            hits.push(n);
        }
        if first {
            pass1 += 1;
            fold = fold.rotate_left(5) ^ n[0] ^ n[1] ^ n[2] ^ n[3];
            fold = fold.wrapping_mul(0x9E37_79B9_7F4A_7C15);
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
