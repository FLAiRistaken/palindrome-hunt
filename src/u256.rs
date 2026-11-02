//! Hand-rolled fixed-width u256: four u64 limbs, little endian. Chosen in P1-2 (#22, see
//! docs/SPEC.md section 7) over the `bnum` crate.
//!
//! Only what the hot loop needs: xor, "do these share a bit", less-than and subtract. Every
//! operation is straight-line limb arithmetic with an explicit borrow chain: no heap, no
//! dynamic dispatch, no branches on the data. That is also the shape a GPU shader wants.
//!
//! The setup helpers below (`mul_small`, `rem_small`, `to_decimal`, `to_hex`) work on plain
//! limbs and are not used inside the hot loop.

use crate::wide::Wide;

/// Little-endian 64-bit limbs.
pub type Limbs = [u64; 4];

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct U256(pub Limbs);

impl Wide for U256 {
    const NAME: &'static str = "hand-rolled u256 (4 x u64)";

    #[inline(always)]
    fn from_limbs(l: Limbs) -> Self {
        U256(l)
    }

    #[inline(always)]
    fn to_limbs(self) -> Limbs {
        self.0
    }

    #[inline(always)]
    fn xor(self, o: Self) -> Self {
        let (a, b) = (self.0, o.0);
        U256([a[0] ^ b[0], a[1] ^ b[1], a[2] ^ b[2], a[3] ^ b[3]])
    }

    #[inline(always)]
    fn intersects(self, o: Self) -> bool {
        let (a, b) = (self.0, o.0);
        ((a[0] & b[0]) | (a[1] & b[1]) | (a[2] & b[2]) | (a[3] & b[3])) != 0
    }

    #[inline(always)]
    fn less(self, o: Self) -> bool {
        // self < o exactly when self - o borrows out of the top limb.
        let mut borrow = false;
        for i in 0..4 {
            let (t, b1) = self.0[i].overflowing_sub(o.0[i]);
            let (_, b2) = t.overflowing_sub(borrow as u64);
            borrow = b1 | b2;
        }
        borrow
    }

    #[inline(always)]
    fn sub(self, o: Self) -> Self {
        let mut out = [0u64; 4];
        let mut borrow = false;
        for (i, limb) in out.iter_mut().enumerate() {
            let (t, b1) = self.0[i].overflowing_sub(o.0[i]);
            let (t, b2) = t.overflowing_sub(borrow as u64);
            *limb = t;
            borrow = b1 | b2;
        }
        U256(out)
    }
}

// ---------------------------------------------------------------------------------------------
// Limb helpers (setup and output only).

/// The value with only bit `bit` set (`bit < 256`).
pub fn bit(bit: u32) -> Limbs {
    let mut l = [0u64; 4];
    l[(bit / 64) as usize] = 1u64 << (bit % 64);
    l
}

pub fn from_u128(n: u128) -> Limbs {
    [n as u64, (n >> 64) as u64, 0, 0]
}

/// `l * k`; panics if the product does not fit in 256 bits.
pub fn mul_small(l: Limbs, k: u64) -> Limbs {
    let mut out = [0u64; 4];
    let mut carry = 0u128;
    for i in 0..4 {
        let t = l[i] as u128 * k as u128 + carry;
        out[i] = t as u64;
        carry = t >> 64;
    }
    assert_eq!(carry, 0, "product does not fit in 256 bits");
    out
}

/// `l mod m` for a small nonzero modulus.
pub fn rem_small(l: Limbs, m: u64) -> u64 {
    let mut r = 0u128;
    for i in (0..4).rev() {
        r = ((r << 64) | l[i] as u128) % m as u128;
    }
    r as u64
}

/// Exact decimal string (no leading zeros; "0" for zero).
pub fn to_decimal(mut l: Limbs) -> String {
    const CHUNK: u128 = 10_000_000_000_000_000_000; // 10^19
    let mut chunks = Vec::new();
    while l != [0; 4] {
        let mut rem = 0u128;
        for i in (0..4).rev() {
            let cur = (rem << 64) | l[i] as u128;
            l[i] = (cur / CHUNK) as u64;
            rem = cur % CHUNK;
        }
        chunks.push(rem as u64);
    }
    match chunks.split_last() {
        None => "0".into(),
        Some((top, rest)) => {
            let mut s = top.to_string();
            for c in rest.iter().rev() {
                s.push_str(&format!("{c:019}"));
            }
            s
        }
    }
}

/// Exact lowercase hex string without prefix or leading zeros ("0" for zero), the same form as
/// `format!("{n:x}")` on a u128.
pub fn to_hex(l: Limbs) -> String {
    let Some(top) = (0..4).rev().find(|&i| l[i] != 0) else {
        return "0".into();
    };
    let mut s = format!("{:x}", l[top]);
    for i in (0..top).rev() {
        s.push_str(&format!("{:016x}", l[i]));
    }
    s
}
