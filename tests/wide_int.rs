//! Checks for the wide-integer types (spike #22). Since #26 the hand-rolled u256 and the hot
//! loop are the production u256 engine in `src/`; `benches/wide/` re-exports them and adds the
//! bnum comparison type, so what is tested is what is timed and what `search` runs.
//!
//! Three kinds of check:
//! * the existing 126-bit engine's results are re-derived by every candidate type;
//! * random values of 127 to 255 bits are compared with a slow reference that shares no code;
//! * the hot loop allocates nothing.

#[path = "../benches/wide/mod.rs"]
mod wide;

use palhunt::digits::DecimalChecker;
use palhunt::layout::layout;
use palhunt::unit::{UnitResult, run_unit};
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::hint::black_box;
use wide::engine::{
    Bnum as BnumU256, Limbs, Wide, WideDecimal, WideLayout, WideUnitResult, run_wide_unit, wide_layout,
};
use wide::reference;
use wide::u256::U256;

// ---------------------------------------------------------------------------------------------
// Counting allocator: counts allocations made by the current thread, so parallel tests do not
// disturb each other.

struct Counting;

thread_local! {
    static ALLOCS: Cell<usize> = const { Cell::new(0) };
}

fn bump() {
    ALLOCS.with(|c| c.set(c.get() + 1));
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        bump();
        unsafe { System.alloc(l) }
    }
    unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
        bump();
        unsafe { System.alloc_zeroed(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        unsafe { System.dealloc(p, l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, new: usize) -> *mut u8 {
        bump();
        unsafe { System.realloc(p, l, new) }
    }
}

#[global_allocator]
static COUNTING: Counting = Counting;

fn allocations_during<R>(f: impl FnOnce() -> R) -> (R, usize) {
    let before = ALLOCS.with(|c| c.get());
    let r = f();
    (r, ALLOCS.with(|c| c.get()) - before)
}

// ---------------------------------------------------------------------------------------------

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

/// A random value with exactly `bits` bits (top bit set).
fn random_with_bits(rng: &mut Rng, bits: u32) -> Limbs {
    let mut l = [rng.next(), rng.next(), rng.next(), rng.next()];
    for (i, limb) in l.iter_mut().enumerate() {
        let lo = 64 * i as u32;
        if bits <= lo {
            *limb = 0;
        } else if bits < lo + 64 {
            *limb &= (1u64 << (bits - lo)) - 1;
        }
    }
    let top = bits - 1;
    l[(top / 64) as usize] |= 1u64 << (top % 64);
    l
}

fn two_pow_255_decimal() -> String {
    reference::to_decimal([0, 0, 0, 1 << 63])
}

fn fits_below_2_255(s: &str) -> bool {
    let limit = two_pow_255_decimal();
    s.len() < limit.len() || (s.len() == limit.len() && s < limit.as_str())
}

/// Compare a wide result with the production `UnitResult` field by field.
fn assert_matches_production(label: &str, w: &WideUnitResult, p: &UnitResult) {
    assert_eq!(w.candidates, p.candidates, "{label}: candidates");
    assert_eq!(w.pass1, p.pass1, "{label}: pass1");
    assert_eq!(format!("{:016x}", w.fold), p.fold, "{label}: fold");
    let hits: Vec<String> = w.hits.iter().map(|&l| reference::to_decimal(l)).collect();
    let want: Vec<String> = p.hits.iter().map(|h| h.n.clone()).collect();
    assert_eq!(hits, want, "{label}: hits");
}

fn run_all_types(wl: &WideLayout, unit_bits: u32, index: u128) -> [(&'static str, WideUnitResult); 2] {
    let a = run_wide_unit(
        &wl.prepare::<U256>(),
        &WideDecimal::<U256>::new(),
        unit_bits,
        index,
    );
    let b = run_wide_unit(
        &wl.prepare::<BnumU256>(),
        &WideDecimal::<BnumU256>::new(),
        unit_bits,
        index,
    );
    [(U256::NAME, a), (BnumU256::NAME, b)]
}

// ---------------------------------------------------------------------------------------------
// Layout

#[test]
fn wide_layout_equals_production_layout_below_127_bits() {
    for bits in 1..=126u32 {
        match (layout(bits), wide_layout(bits)) {
            (None, None) => {}
            (Some(p), Some(w)) => {
                assert_eq!(w.free(), p.free(), "bits {bits}");
                assert_eq!(
                    w.base,
                    [p.base as u64, (p.base >> 64) as u64, 0, 0],
                    "bits {bits}"
                );
                for (wm, pm) in w.masks.iter().zip(&p.masks) {
                    assert_eq!(*wm, [*pm as u64, (*pm >> 64) as u64, 0, 0], "bits {bits}");
                }
            }
            (p, w) => panic!("bits {bits}: production {p:?} vs wide {w:?}"),
        }
    }
}

#[test]
fn wide_layout_values_are_palindromes_127_to_255_bits() {
    let mut rng = Rng(0x1234_5678_9abc_def1);
    let mut with_layout = 0;
    for bits in 127..=255u32 {
        let Some(wl) = wide_layout(bits) else { continue };
        with_layout += 1;
        for _ in 0..6 {
            let g = if wl.free() == 0 {
                0
            } else {
                ((rng.next() as u128) << 64 | rng.next() as u128) & ((1u128 << wl.free()) - 1)
            };
            let n = reference::value_at(&wl, g);
            assert_eq!(reference::bit_length(n), bits);
            assert!(reference::is_palindrome(n, 2), "bits {bits} base 2");
            assert!(reference::is_palindrome(n, 16), "bits {bits} base 16");
        }
    }
    assert!(with_layout > 60, "only {with_layout} lengths had a layout");
}

// ---------------------------------------------------------------------------------------------
// Re-deriving the existing u128 results

/// Every record in the committed fixture of old results is re-derived by all three types.
#[test]
fn fixture_results_are_rederived_by_every_type() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/old-code-results.jsonl"
    );
    let text = std::fs::read_to_string(path).unwrap();
    let mut records = 0;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let r: UnitResult = serde_json::from_str(line).unwrap();
        let wl = wide_layout(r.bits).expect("fixture lengths have a layout");
        let label = format!("fixture bits {} unit {}", r.bits, r.unit);
        let native = run_wide_unit(
            &wl.prepare::<u128>(),
            &WideDecimal::<u128>::new(),
            r.unit_bits,
            r.unit as u128,
        );
        assert_matches_production(&format!("{label} [native u128]"), &native, &r);
        for (name, w) in run_all_types(&wl, r.unit_bits, r.unit as u128) {
            assert_matches_production(&format!("{label} [{name}]"), &w, &r);
        }
        records += 1;
    }
    assert!(records >= 100, "fixture has only {records} records");
}

/// Units at 100 to 126 bits, re-run with the production engine and with each candidate type.
#[test]
fn units_at_100_to_126_bits_match_production() {
    let dec = DecimalChecker::new();
    let mut rng = Rng(0x0dd_ba11_cafe_f00d);
    let unit_bits = 12;
    let mut compared = 0;
    for bits in 100..=126u32 {
        let (Some(pl), Some(wl)) = (layout(bits), wide_layout(bits)) else {
            continue;
        };
        let units = 1u64 << (pl.free() - unit_bits.min(pl.free()));
        let mut picks = vec![0, units - 1, rng.below(units), rng.below(units)];
        picks.dedup();
        for unit in picks {
            let p = run_unit(&pl, unit_bits, unit, &dec);
            let label = format!("bits {bits} unit {unit}");
            let native = run_wide_unit(
                &wl.prepare::<u128>(),
                &WideDecimal::<u128>::new(),
                unit_bits,
                unit as u128,
            );
            assert_matches_production(&format!("{label} [native u128]"), &native, &p);
            for (name, w) in run_all_types(&wl, unit_bits, unit as u128) {
                assert_matches_production(&format!("{label} [{name}]"), &w, &p);
            }
            compared += 1;
        }
    }
    assert!(compared > 40, "only {compared} units compared");
}

// ---------------------------------------------------------------------------------------------
// 127 to 255 bits against the slow reference

#[test]
fn whole_units_at_127_to_255_bits_match_slow_reference() {
    let mut rng = Rng(0xfeed_face_dead_beef);
    let unit_bits = 8;
    let mut compared = 0;
    for bits in 127..=255u32 {
        let Some(wl) = wide_layout(bits) else { continue };
        let units = 1u128 << (wl.free() - unit_bits.min(wl.free()));
        let random_unit = ((rng.next() as u128) << 64 | rng.next() as u128) % units;
        let mut picks = vec![0, units - 1, random_unit];
        picks.dedup();
        for unit in picks {
            let want = reference::run_unit(&wl, unit_bits, unit);
            for (name, got) in run_all_types(&wl, unit_bits, unit) {
                assert_eq!(got, want, "bits {bits} unit {unit} [{name}]");
            }
            compared += 1;
        }
    }
    assert!(compared > 150, "only {compared} units compared");
}

/// The fast decimal check, for both types, against the reference on random and constructed
/// values of 127 to 255 bits (and on shorter palindromes, which exercise the deep fallback).
fn check_decimal_checker<T: Wide>(values: &[Limbs]) {
    let dec = WideDecimal::<T>::new();
    for &l in values {
        let digits = reference::digits_le(l, 10);
        let d = digits.len() as u32;
        let low = digits.iter().take(6).rev().fold(0u64, |a, &x| a * 10 + x as u64);
        let n = T::from_limbs(l);
        assert_eq!(
            dec.digits(n),
            d,
            "{} digit count of {}",
            T::NAME,
            reference::to_decimal(l)
        );
        let want_pal = digits.iter().eq(digits.iter().rev());
        let want_first = d == 1 || digits[0] == digits[d as usize - 1];
        let mut first = false;
        let got = dec.is_palindrome(n, low, d, &mut first);
        let s = reference::to_decimal(l);
        assert_eq!(got, want_pal, "{} palindrome of {s}", T::NAME);
        assert_eq!(first, want_first, "{} first pair of {s}", T::NAME);
    }
}

fn decimal_test_values() -> Vec<Limbs> {
    let mut rng = Rng(0x600d_f00d_1bad_cafe);
    let mut values: Vec<Limbs> = Vec::new();
    // Random values of every length from 127 to 255 bits.
    for bits in 127..=255u32 {
        for _ in 0..40 {
            values.push(random_with_bits(&mut rng, bits));
        }
    }
    // Decimal palindromes of 1 to 77 digits, and single-digit mutations of them.
    for len in 1..=77usize {
        for _ in 0..30 {
            let half = len.div_ceil(2);
            let mut s: Vec<u8> = (0..half).map(|_| b'0' + rng.below(10) as u8).collect();
            s[0] = b'1' + rng.below(9) as u8;
            let mut full = s.clone();
            full.extend(s[..len / 2].iter().rev());
            let text = String::from_utf8(full.clone()).unwrap();
            if !fits_below_2_255(&text) {
                continue;
            }
            values.push(reference::parse_decimal(&text));
            // Mutate one digit: outer pair, an inner pair, or the middle.
            let pos = rng.below(len as u64) as usize;
            full[pos] = b'0' + ((full[pos] - b'0' + 1 + rng.below(9) as u8) % 10);
            if full[0] == b'0' {
                continue;
            }
            let text = String::from_utf8(full).unwrap();
            if fits_below_2_255(&text) {
                values.push(reference::parse_decimal(&text));
            }
        }
    }
    values
}

#[test]
fn fast_decimal_check_matches_reference_for_both_types() {
    let values = decimal_test_values();
    assert!(values.len() > 8000);
    check_decimal_checker::<U256>(&values);
    check_decimal_checker::<BnumU256>(&values);
}

/// The primitive operations agree between the two types and with plain limb arithmetic.
#[test]
fn primitive_operations_agree_between_types() {
    fn check<T: Wide>(a: Limbs, b: Limbs) {
        let (x, y) = (T::from_limbs(a), T::from_limbs(b));
        assert_eq!(x.to_limbs(), a, "{} round trip", T::NAME);
        let xor = [a[0] ^ b[0], a[1] ^ b[1], a[2] ^ b[2], a[3] ^ b[3]];
        assert_eq!(x.xor(y).to_limbs(), xor, "{} xor", T::NAME);
        let shares = (0..4).any(|i| a[i] & b[i] != 0);
        assert_eq!(x.intersects(y), shares, "{} intersects", T::NAME);
        let less = (0..4)
            .rev()
            .map(|i| a[i].cmp(&b[i]))
            .find(|o| o.is_ne())
            .is_some_and(|o| o.is_lt());
        assert_eq!(x.less(y), less, "{} less", T::NAME);
        if !less {
            // (a - b) + b == a, with the addition done limb by limb in u128.
            let d = x.sub(y).to_limbs();
            let (mut carry, mut back) = (0u128, [0u64; 4]);
            for i in 0..4 {
                let t = d[i] as u128 + b[i] as u128 + carry;
                back[i] = t as u64;
                carry = t >> 64;
            }
            assert_eq!(back, a, "{} sub", T::NAME);
        }
    }
    let mut rng = Rng(0xabad_1dea_0ddc_0ffe);
    for round in 0..20_000 {
        let a = [rng.next(), rng.next(), rng.next(), rng.next()];
        let mut b = [rng.next(), rng.next(), rng.next(), rng.next()];
        match round % 4 {
            // Equal high limbs and borrows that ripple through every limb.
            0 => b[3] = a[3],
            1 => {
                b[3] = a[3];
                b[2] = a[2];
                b[1] = a[1];
            }
            2 => b = [a[0].wrapping_add(1), a[1], a[2], a[3]],
            _ => {}
        }
        check::<U256>(a, b);
        check::<BnumU256>(a, b);
        check::<U256>(b, a);
        check::<BnumU256>(b, a);
    }
}

// ---------------------------------------------------------------------------------------------
// Allocation-free

#[test]
fn hot_loop_does_not_allocate() {
    // The counter itself works: a Vec and a String are seen.
    let (_, seen) = allocations_during(|| (black_box(vec![1u8; 64]), black_box(String::from("x"))));
    assert!(seen >= 2, "allocation counter saw {seen}");

    fn check<T: Wide>(bits: u32) {
        let wl = wide_layout(bits).unwrap();
        let prepared = wl.prepare::<T>();
        let dec = WideDecimal::<T>::new();
        let (r, allocs) = allocations_during(|| run_wide_unit(&prepared, &dec, 16, 3));
        assert_eq!(r.candidates, 1 << 16);
        assert!(
            r.pass1 > 1000,
            "pass1 {} too low to exercise the checker",
            r.pass1
        );
        assert!(r.hits.is_empty(), "a hit would take the (allocating) cold path");
        assert_eq!(allocs, 0, "{} at {bits} bits allocated in the hot loop", T::NAME);
    }
    for bits in [140, 200, 252] {
        check::<U256>(bits);
        check::<BnumU256>(bits);
    }

    // The deep fallback (all six tracked digit pairs match) is allocation-free too.
    fn deep<T: Wide>() {
        let dec = WideDecimal::<T>::new();
        // A 60-digit palindrome, and the same with one inner digit changed.
        let half = "123456789012345678901234567890";
        let text = format!("{half}{}", half.chars().rev().collect::<String>());
        let pal = reference::parse_decimal(&text);
        let mut near = text.clone().into_bytes();
        near[20] = b'7';
        let near = reference::parse_decimal(std::str::from_utf8(&near).unwrap());
        for (l, want) in [(pal, true), (near, false)] {
            let digits = reference::digits_le(l, 10);
            let low = digits.iter().take(6).rev().fold(0u64, |a, &x| a * 10 + x as u64);
            let mut first = false;
            let (got, allocs) = allocations_during(|| {
                dec.is_palindrome(T::from_limbs(l), low, digits.len() as u32, &mut first)
            });
            assert_eq!(got, want, "{}", T::NAME);
            assert_eq!(allocs, 0, "{} deep fallback allocated", T::NAME);
        }
    }
    deep::<U256>();
    deep::<BnumU256>();
}
