//! Which bits of an L-bit number are free once it must be a palindrome in base 2 and base 16.
//!
//! A binary palindrome ties bit i to bit L-1-i. A hex palindrome ties bit 4j+b to bit 4k+b,
//! where hex digit j mirrors hex digit k = H-1-j (H = number of hex digits). When L is not a
//! multiple of 4, the top hex digit is padded with zero bits above L-1, so the low bits that
//! mirror that padding are forced to zero. Joining both sets of ties gives equivalence classes
//! of bits: every number in the search space is "the top-bit class, plus any subset of the
//! free classes". That is about 2^(L/4) numbers instead of 2^L.

/// Largest bit length the u128 engine supports. 2^126 has 38 decimal digits, so every power
/// of ten the decimal check needs fits in a u128. Wider numbers need a big-integer engine.
pub const MAX_BITS: u32 = 126;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    pub bits: u32,
    /// Bits that are always set: the class containing the top bit (and so bit 0).
    pub base: u128,
    /// One mask per free class, ordered by lowest bit ascending.
    pub masks: Vec<u128>,
}

impl Layout {
    /// Number of free classes. The search space for this bit length has 2^free() numbers.
    pub fn free(&self) -> u32 {
        self.masks.len() as u32
    }

    /// The number at Gray-code index `g` (bit j of `g` selects mask j).
    pub fn value_at(&self, g: u128) -> u128 {
        let mut n = self.base;
        for (j, m) in self.masks.iter().enumerate() {
            if (g >> j) & 1 == 1 {
                n |= m;
            }
        }
        n
    }
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

/// Layout of all L-bit numbers (top bit set) that are palindromes in base 2 and 16.
/// Returns `None` when no such number exists at this length.
pub fn layout(bits: u32) -> Option<Layout> {
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
    let mut classes: Vec<(usize, u128)> = Vec::new(); // (root, mask)
    for i in 0..l {
        let r = dsu.find(i);
        match classes.iter_mut().find(|(root, _)| *root == r) {
            Some((_, m)) => *m |= 1u128 << i,
            None => classes.push((r, 1u128 << i)),
        }
    }
    let mut base = 0u128;
    let mut masks = Vec::new();
    for (root, mask) in classes {
        if root == top {
            base = mask;
        } else if !zero_roots.contains(&root) {
            masks.push(mask);
        }
    }
    masks.sort_by_key(|m| m.trailing_zeros());
    Some(Layout { bits, base, masks })
}
