//! The hot loop under test is production code: `palhunt::wide` (the generic loop and the `Wide`
//! trait, implemented there for `u128` and the hand-rolled `U256`) and `palhunt::layout`. This
//! module re-exports it under the names the benchmark and `tests/wide_int.rs` have used since
//! the spike (#22), and adds the one type that stays outside `src/`: `bnum`'s U256, kept as a
//! comparison in the benchmark.

pub use palhunt::layout::{WideLayout, wide_layout};
pub use palhunt::u256::Limbs;
pub use palhunt::wide::{Wide, WideDecimal, WideUnitResult, run_wide_unit};

/// The `bnum` crate's 256-bit unsigned integer. A newtype, because the orphan rule does not
/// allow implementing `palhunt`'s trait for `bnum`'s type directly.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Bnum(pub bnum::types::U256);

impl Wide for Bnum {
    const NAME: &'static str = "bnum U256";

    #[inline(always)]
    fn from_limbs(l: Limbs) -> Self {
        let mut b = [0u8; 32];
        for (i, x) in l.iter().enumerate() {
            b[8 * i..8 * i + 8].copy_from_slice(&x.to_le_bytes());
        }
        Bnum(bnum::types::U256::from_le_bytes(b))
    }
    #[inline(always)]
    fn to_limbs(self) -> Limbs {
        let b = self.0.to_le_bytes();
        let limb = |i: usize| u64::from_le_bytes(b[8 * i..8 * i + 8].try_into().unwrap());
        [limb(0), limb(1), limb(2), limb(3)]
    }
    #[inline(always)]
    fn xor(self, o: Self) -> Self {
        Bnum(self.0 ^ o.0)
    }
    #[inline(always)]
    fn intersects(self, o: Self) -> bool {
        !(self.0 & o.0).is_zero()
    }
    #[inline(always)]
    fn less(self, o: Self) -> bool {
        self.0 < o.0
    }
    #[inline(always)]
    fn sub(self, o: Self) -> Self {
        Bnum(self.0 - o.0)
    }
}
