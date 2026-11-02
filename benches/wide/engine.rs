//! The hot loop of `src/unit.rs::run_unit`, written once over any type that implements `Wide`.
//!
//! Operations taken from `run_unit` and `DecimalChecker` (and nothing else):
//!
//! 1. Gray-code step: `n ^= masks[tz(i)]`, after testing `n & mask != 0` to pick add or subtract.
//! 2. Residue update: `low = (low +/- mask_mod[j]) mod 10^6`, plain u64 arithmetic (same for every type).
//! 3. Digit length: `n >= 10^d` to pick between two decimal lengths.
//! 4. Pass-1 reject: the table-driven decimal digit-pair compare (`row[k] > rest`, `rest -= row[k]`).
//! 5. Fold: `fold = (rotl(fold, 5) ^ every 64-bit limb of n) * 0x9E3779B97F4A7C15`.
//!
//! The loop does not allocate and uses no floating point. The only allocations are in setup
//! (`Prepared`, `WideDecimal`) and on the cold path that confirms a hit with the reference.

use super::reference;

pub type Limbs = [u64; 4];

/// Same constants as `src/digits.rs`.
pub const LOW_MOD: u64 = 1_000_000;
pub const LOW_DIGITS: u32 = 6;
/// 2^255 < 10^77 < 2^256, so every candidate of up to 255 bits has at most 77 decimal digits.
pub const MAX_DIGITS: u32 = 77;
pub const MAX_BITS: u32 = 255;

/// What the hot loop needs from an integer type. Each method is one primitive step; an
/// implementation must be pure limb arithmetic (no allocation, no dispatch) to qualify.
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

/// Native u128: the existing engine's type, only valid below 128 bits (callers use <= 126).
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

/// The `bnum` crate's 256-bit unsigned integer.
impl Wide for bnum::types::U256 {
    const NAME: &'static str = "bnum U256";

    #[inline(always)]
    fn from_limbs(l: Limbs) -> Self {
        let mut b = [0u8; 32];
        for (i, x) in l.iter().enumerate() {
            b[8 * i..8 * i + 8].copy_from_slice(&x.to_le_bytes());
        }
        Self::from_le_bytes(b)
    }
    #[inline(always)]
    fn to_limbs(self) -> Limbs {
        let b = self.to_le_bytes();
        let limb = |i: usize| u64::from_le_bytes(b[8 * i..8 * i + 8].try_into().unwrap());
        [limb(0), limb(1), limb(2), limb(3)]
    }
    #[inline(always)]
    fn xor(self, o: Self) -> Self {
        self ^ o
    }
    #[inline(always)]
    fn intersects(self, o: Self) -> bool {
        !(self & o).is_zero()
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

// ---------------------------------------------------------------------------------------------
// Setup helpers on plain limbs (type independent, used to build tables and to cross-check).

fn bit_limbs(bit: u32) -> Limbs {
    let mut l = [0u64; 4];
    l[(bit / 64) as usize] = 1u64 << (bit % 64);
    l
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

/// `l mod m` for a small modulus.
pub fn rem_small(l: Limbs, m: u64) -> u64 {
    let mut r = 0u128;
    for i in (0..4).rev() {
        r = ((r << 64) | l[i] as u128) % m as u128;
    }
    r as u64
}

/// Allocation-free exact decimal palindrome test (stack buffer, repeated division by 10^19).
/// The cold fallback once the six tracked low digits are used up. Not shader-friendly (it uses
/// u128 division), but it runs about once per million candidates.
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

// ---------------------------------------------------------------------------------------------
// Layout for up to 255 bits. Same algorithm as src/layout.rs (union-find over bit ties), with
// masks stored as limbs. src/layout.rs stays as it is; the tests prove the two agree below 127.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WideLayout {
    pub bits: u32,
    pub base: Limbs,
    pub masks: Vec<Limbs>,
}

struct Dsu(Vec<usize>);

impl Dsu {
    fn find(&mut self, mut x: usize) -> usize {
        while self.0[x] != x {
            self.0[x] = self.0[self.0[x]];
            x = self.0[x];
        }
        x
    }
    fn union(&mut self, a: usize, b: usize) {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra != rb {
            self.0[ra] = rb;
        }
    }
}

fn low_bit(l: Limbs) -> u32 {
    (0..4)
        .find(|&i| l[i] != 0)
        .map_or(256, |i| 64 * i as u32 + l[i].trailing_zeros())
}

pub fn wide_layout(bits: u32) -> Option<WideLayout> {
    assert!((1..=MAX_BITS).contains(&bits), "bits must be in 1..={MAX_BITS}");
    let l = bits as usize;
    let mut dsu = Dsu((0..l).collect());
    for i in 0..l {
        dsu.union(i, l - 1 - i);
    }
    let h = l.div_ceil(4);
    let mut zero_pos = Vec::new();
    for j in 0..h {
        let k = h - 1 - j;
        for b in 0..4 {
            let (a, c) = (4 * j + b, 4 * k + b);
            match (a < l, c < l) {
                (true, true) => dsu.union(a, c),
                (true, false) => zero_pos.push(a),
                (false, true) => zero_pos.push(c),
                (false, false) => {}
            }
        }
    }
    let zero_roots: Vec<usize> = zero_pos.iter().map(|&p| dsu.find(p)).collect();
    let top = dsu.find(l - 1);
    if zero_roots.contains(&top) {
        return None;
    }
    let mut classes: Vec<(usize, Limbs)> = Vec::new();
    for i in 0..l {
        let r = dsu.find(i);
        let bit = bit_limbs(i as u32);
        match classes.iter_mut().find(|(root, _)| *root == r) {
            Some((_, m)) => (0..4).for_each(|k| m[k] |= bit[k]),
            None => classes.push((r, bit)),
        }
    }
    let mut base = [0u64; 4];
    let mut masks = Vec::new();
    for (root, mask) in classes {
        if root == top {
            base = mask;
        } else if !zero_roots.contains(&root) {
            masks.push(mask);
        }
    }
    masks.sort_by_key(|&m| low_bit(m));
    Some(WideLayout { bits, base, masks })
}

impl WideLayout {
    pub fn free(&self) -> u32 {
        self.masks.len() as u32
    }

    pub fn prepare<T: Wide>(&self) -> Prepared<T> {
        Prepared {
            bits: self.bits,
            base: T::from_limbs(self.base),
            masks: self.masks.iter().map(|&m| T::from_limbs(m)).collect(),
            mask_mod: self.masks.iter().map(|&m| rem_small(m, LOW_MOD)).collect(),
        }
    }
}

/// A layout converted to one integer type, with the per-mask residues precomputed so that the
/// unit loop itself allocates nothing.
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

// ---------------------------------------------------------------------------------------------
// Decimal checker: the same tables and scan as src/digits.rs::DecimalChecker, over `T`.

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

    /// Decimal digit count of `n`, for 1 <= n < 2^255.
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

// ---------------------------------------------------------------------------------------------

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
    let lo = T::from_limbs(bit_limbs(layout.bits - 1));
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
            assert!(
                reference::is_palindrome(l, 2)
                    && reference::is_palindrome(l, 16)
                    && reference::is_palindrome(l, 10)
            );
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
