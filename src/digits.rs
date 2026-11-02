//! Palindrome checks. `is_palindrome` is the slow, obviously-correct reference used to
//! confirm every hit. `DecimalChecker` is the fast path the search loop uses.

/// Reference check: is `n` a palindrome when written in `base`? Zero counts as a palindrome.
pub fn is_palindrome(n: u128, base: u128) -> bool {
    let mut digits = Vec::with_capacity(128);
    let mut v = n;
    while v > 0 {
        digits.push(v % base);
        v /= base;
    }
    digits.iter().eq(digits.iter().rev())
}

/// Reference check for 256-bit values (little-endian u64 limbs): is `l` a palindrome in `base`?
/// Zero counts as a palindrome, as in `is_palindrome`. Deliberately simple and independent of
/// the fast path: the digits come from double-and-add over a digit vector, one bit at a time
/// from the top, with no division of the wide value and no power-of-ten table.
pub fn is_palindrome_wide(l: [u64; 4], base: u32) -> bool {
    let d = digits_wide(l, base);
    d.iter().eq(d.iter().rev())
}

/// Little-endian digits of `l` in `base` (empty for zero), for the reference check.
fn digits_wide(l: [u64; 4], base: u32) -> Vec<u32> {
    let mut digits: Vec<u32> = Vec::with_capacity(256);
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

/// Number of digits of `n` in `base` (0 has one digit).
pub fn digit_count(n: u128, base: u128) -> u32 {
    let (mut d, mut v) = (1, n / base);
    while v > 0 {
        d += 1;
        v /= base;
    }
    d
}

/// Low decimal digits the search loop tracks incrementally: n mod 10^LOW_DIGITS.
pub const LOW_DIGITS: u32 = 6;
pub const LOW_MOD: u64 = 1_000_000;

/// Fast decimal palindrome test for numbers below 10^38.
///
/// The search loop keeps `n mod 10^6` up to date with one add or subtract per step, so the low
/// digits come for free. High digits are found by comparing against a table of k * 10^p, with
/// no division. Each digit pair rejects about 90% of what reaches it, so almost every candidate
/// is rejected after one or two comparisons.
pub struct DecimalChecker {
    /// pow10[p] = 10^p, p in 0..=38.
    pow10: [u128; 39],
    /// mul[p][k] = k * 10^p, k in 0..=9, p in 0..=37.
    mul: Vec<[u128; 10]>,
}

impl Default for DecimalChecker {
    fn default() -> Self {
        Self::new()
    }
}

impl DecimalChecker {
    pub fn new() -> Self {
        let mut pow10 = [0u128; 39];
        pow10[0] = 1;
        for p in 1..39 {
            pow10[p] = pow10[p - 1] * 10;
        }
        let mul = (0..38)
            .map(|p| {
                let mut row = [0u128; 10];
                for (k, cell) in row.iter_mut().enumerate() {
                    *cell = pow10[p] * k as u128;
                }
                row
            })
            .collect();
        Self { pow10, mul }
    }

    /// Decimal digit count of `n`, for 1 <= n < 10^38.
    #[inline]
    pub fn digits(&self, n: u128) -> u32 {
        // 2^126 < 10^38, so n < pow10[38] always holds in the engine's range.
        let mut d = 1;
        while d < 38 && n >= self.pow10[d as usize] {
            d += 1;
        }
        d
    }

    /// Is `n` a decimal palindrome? `low` must equal `n % LOW_MOD`, `d` the digit count.
    /// `first_pair` is set to true when the outermost digit pair matched (used in checksums).
    #[inline]
    pub fn is_palindrome(&self, n: u128, low: u64, d: u32, first_pair: &mut bool) -> bool {
        *first_pair = false;
        let pairs = d / 2;
        let mut rest = n; // n with the high digits already read removed
        let mut low_rest = low;
        for i in 0..pairs {
            let pos = (d - 1 - i) as usize;
            let row = &self.mul[pos];
            // Largest k with k * 10^pos <= rest. Linear scan: digits are uniform, and nine
            // compares of u128 are cheaper than one u128 division.
            let mut k = 9;
            while row[k] > rest {
                k -= 1;
            }
            rest -= row[k];
            let low_digit = if i < LOW_DIGITS {
                let v = low_rest % 10;
                low_rest /= 10;
                v as usize
            } else {
                // Past the tracked residue: fall back to the exact reference for the rest.
                return is_palindrome(n, 10);
            };
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
