//! Hand-rolled fixed-width u256: four u64 limbs, little endian.
//!
//! Only what the hot loop needs: xor, "do these share a bit", less-than and subtract. Every
//! operation is straight-line limb arithmetic with an explicit borrow chain: no heap, no
//! dynamic dispatch, no branches on the data. That is also the shape a GPU shader wants (see the
//! note in docs/SPEC.md section 7 on 64-bit limbs versus 32-bit limbs).

use super::engine::{Limbs, Wide};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
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
