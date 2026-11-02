//! The u256 engine (#26) as `search` uses it: it reproduces the u128 engine wherever both run,
//! its decimal fast path agrees with the reference, its hits are written without loss, and its
//! records go through the log, `status`, `verify` and the CLI past 126 bits.
//!
//! The slow oracle in `benches/wide/reference.rs` shares no code with `src/`.

#[path = "../benches/wide/mod.rs"]
mod wide;

use palhunt::digits::{DecimalChecker, LOW_MOD, digit_count, is_palindrome, is_palindrome_wide};
use palhunt::layout::{MAX_BITS, MAX_WIDE_BITS, layout, wide_layout};
use palhunt::u256::{self, Limbs, U256};
use palhunt::unit::{
    ENGINE, ENGINE_U256, Hit, Length, UnitId, UnitResult, engine_for, run_unit, run_unit_u256, unit_plan,
};
use palhunt::wide::{Prepared, Wide, WideDecimal};
use palhunt::{frontier, state, status};
use rayon::prelude::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use wide::reference;

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

/// A random value with exactly `bits` bits (top bit set), 1 <= bits <= 256.
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

/// Every field two machines must agree on, apart from the engine tag.
fn assert_same_result(label: &str, wide: &UnitResult, narrow: &UnitResult) {
    assert_eq!(wide.engine, ENGINE_U256, "{label}");
    assert_eq!(narrow.engine, ENGINE, "{label}");
    assert_eq!(wide.v, narrow.v, "{label}: v");
    assert_eq!(wide.bits, narrow.bits, "{label}: bits");
    assert_eq!(wide.unit, narrow.unit, "{label}: unit");
    assert_eq!(wide.units, narrow.units, "{label}: units");
    assert_eq!(wide.unit_bits, narrow.unit_bits, "{label}: unit_bits");
    assert_eq!(wide.candidates, narrow.candidates, "{label}: candidates");
    assert_eq!(wide.pass1, narrow.pass1, "{label}: pass1");
    assert_eq!(wide.fold, narrow.fold, "{label}: fold");
    assert_eq!(wide.hits, narrow.hits, "{label}: hits");
}

/// Both engines for one length at or below 126 bits.
struct Pair {
    narrow: palhunt::layout::Layout,
    wide: Prepared<U256>,
}

fn pair(bits: u32) -> Option<Pair> {
    Some(Pair {
        narrow: layout(bits)?,
        wide: wide_layout(bits)?.prepare(),
    })
}

fn compare_unit(p: &Pair, unit_bits: u32, unit: u64, dec: &DecimalChecker, wdec: &WideDecimal<U256>) {
    let a = run_unit(&p.narrow, unit_bits, unit, dec);
    let b = run_unit_u256(&p.wide, unit_bits, unit, wdec);
    assert_same_result(
        &format!("bits {} unit_bits {unit_bits} unit {unit}", p.narrow.bits),
        &b,
        &a,
    );
}

// ---------------------------------------------------------------------------------------------
// The u256 engine reproduces the u128 engine.

/// Every unit of every length from 1 to 100 bits, at `unit_bits` 8: about 1.1 x 10^8 candidates
/// per engine.
#[test]
fn u256_engine_equals_u128_engine_on_every_unit_to_100_bits() {
    let (dec, wdec) = (DecimalChecker::new(), WideDecimal::<U256>::new());
    let unit_bits = 8;
    let mut total = 0u64;
    for bits in 1..=100u32 {
        let Some(p) = pair(bits) else { continue };
        let (_, units) = unit_plan(&p.narrow, unit_bits);
        (0..units)
            .into_par_iter()
            .for_each(|u| compare_unit(&p, unit_bits, u, &dec, &wdec));
        total += 1u64 << p.narrow.free();
    }
    assert!(total > 100_000_000, "only {total} candidates compared");
}

/// Every length from 101 to 126 bits on sampled units: first, last and random units at
/// `unit_bits` 12, and more and larger units at the heaviest lengths (118 to 126).
#[test]
fn u256_engine_equals_u128_engine_on_sampled_units_101_to_126_bits() {
    let (dec, wdec) = (DecimalChecker::new(), WideDecimal::<U256>::new());
    let mut rng = Rng(0x9e37_79b9_5eed_0126);
    let mut work: Vec<(u32, u32, u64)> = Vec::new(); // (bits, unit_bits, unit)
    let mut lengths = 0;
    for bits in 101..=MAX_BITS {
        let Some(l) = layout(bits) else { continue };
        lengths += 1;
        let (_, units) = unit_plan(&l, 12);
        let mut picks = vec![0, units - 1];
        picks.extend((0..4).map(|_| rng.below(units)));
        work.extend(picks.into_iter().map(|u| (bits, 12, u)));
        if bits >= 118 {
            let (_, units) = unit_plan(&l, 16);
            work.extend((0..16).map(|_| (bits, 16, rng.below(units))));
        }
    }
    assert!(lengths >= 18, "only {lengths} lengths have a layout");
    let pairs: HashMap<u32, Pair> = (101..=MAX_BITS).filter_map(|b| pair(b).map(|p| (b, p))).collect();
    work.par_iter()
        .for_each(|&(bits, ub, u)| compare_unit(&pairs[&bits], ub, u, &dec, &wdec));
}

// ---------------------------------------------------------------------------------------------
// Engine selection and the record format.

#[test]
fn each_length_has_one_engine_and_unit_ids_fit_the_format() {
    assert_eq!(engine_for(0), None);
    assert_eq!(engine_for(MAX_WIDE_BITS + 1), None);
    let checkers = palhunt::unit::Checkers::new();
    let mut max_free = 0;
    for bits in 1..=MAX_WIDE_BITS {
        let want = if bits <= 126 { ENGINE } else { ENGINE_U256 };
        assert_eq!(engine_for(bits), Some(want), "bits {bits}");
        let Some(len) = Length::new(bits) else {
            assert!(wide_layout(bits).is_none());
            continue;
        };
        assert_eq!(len.engine(), want, "bits {bits}");
        assert_eq!(len.bits(), bits);
        assert_eq!(len.free(), wide_layout(bits).unwrap().free());
        max_free = max_free.max(len.free());
        // Every record the engine writes carries its tag. Unit 0 of a light length is cheap;
        // heavy lengths are covered by the CLI test and the sampled comparisons.
        if len.free() <= 18 {
            let r = len.run(10, 0, &checkers);
            assert_eq!(r.engine, want, "bits {bits}");
            assert_eq!(r.v, palhunt::unit::FORMAT_VERSION);
        }
    }
    // `unit` and `units` are u64 in the record: 2^63 units at most, so no field changes.
    assert!(max_free <= 63, "max free classes {max_free}");
}

// ---------------------------------------------------------------------------------------------
// Hits are written without loss.

fn parse_hex(s: &str) -> Limbs {
    let mut l = [0u64; 4];
    for c in s.chars() {
        let d = c.to_digit(16).expect("hex digit") as u64;
        assert!(l[3] >> 60 == 0, "hex value does not fit in 256 bits");
        for i in (1..4).rev() {
            l[i] = (l[i] << 4) | (l[i - 1] >> 60);
        }
        l[0] = (l[0] << 4) | d;
    }
    l
}

#[test]
fn hit_strings_round_trip_exactly() {
    let mut rng = Rng(0x0bad_5eed_cafe_0001);
    let mut values: Vec<Limbs> = vec![[0; 4], [1, 0, 0, 0], [u64::MAX; 4], [0, 0, 0, 1 << 63]];
    for bits in 1..=256u32 {
        for _ in 0..20 {
            values.push(random_with_bits(&mut rng, bits));
        }
    }
    for l in values {
        let dec = u256::to_decimal(l);
        let hex = u256::to_hex(l);
        assert_eq!(dec, reference::to_decimal(l));
        assert_eq!(reference::parse_decimal(&dec), l, "{dec}");
        assert_eq!(parse_hex(&hex), l, "{hex}");
        assert!(dec == "0" || !dec.starts_with('0'));
        assert!(hex == "0" || !hex.starts_with('0'));
        // Below 2^128 the strings are exactly what the u128 engine writes.
        if l[2] == 0 && l[3] == 0 {
            let n = (l[0] as u128) | ((l[1] as u128) << 64);
            assert_eq!(dec, n.to_string());
            assert_eq!(hex, format!("{n:x}"));
        }
    }
    // A record holding a 255-bit hit survives the log format unchanged.
    let big = [u64::MAX, u64::MAX, u64::MAX, u64::MAX >> 1];
    let r = UnitResult {
        v: palhunt::unit::FORMAT_VERSION,
        engine: ENGINE_U256.into(),
        bits: 255,
        unit: (1 << 41) - 1,
        units: 1 << 41,
        unit_bits: 22,
        candidates: 1 << 22,
        pass1: 7,
        fold: "0123456789abcdef".into(),
        hits: vec![Hit {
            n: u256::to_decimal(big),
            hex: u256::to_hex(big),
            base12: false,
            trivial: false,
        }],
        elapsed_ms: 5,
    };
    let line = serde_json::to_string(&r).unwrap();
    let back: UnitResult = serde_json::from_str(&line).unwrap();
    assert_eq!(back, r);
    assert_eq!(reference::parse_decimal(&back.hits[0].n), big);
    assert_eq!(parse_hex(&back.hits[0].hex), big);
}

// ---------------------------------------------------------------------------------------------
// Fast decimal check against the reference, for the wide type.

/// The reference here is `digits::is_palindrome_wide` (what confirms hits in `src/`), checked in
/// turn against the independent oracle in `benches/wide/reference.rs`.
fn check_wide_decimal(dec: &WideDecimal<U256>, l: Limbs) {
    let digits = reference::digits_le(l, 10);
    let d = digits.len() as u32;
    let low = u256::rem_small(l, LOW_MOD);
    let n = U256::from_limbs(l);
    let s = reference::to_decimal(l);
    assert_eq!(dec.digits(n), d, "digit count of {s}");
    let want = is_palindrome_wide(l, 10);
    assert_eq!(want, digits.iter().eq(digits.iter().rev()), "reference on {s}");
    let want_first = d == 1 || digits[0] == digits[d as usize - 1];
    let mut first = false;
    assert_eq!(
        dec.is_palindrome(n, low, d, &mut first),
        want,
        "fast check on {s}"
    );
    assert_eq!(first, want_first, "first pair of {s}");
}

#[test]
fn fast_decimal_check_agrees_with_reference_wide() {
    let dec = WideDecimal::<U256>::new();
    let mut rng = Rng(0x600d_f00d_0000_0126);
    let mut checked = 0;
    // Random values of every length from 1 to 255 bits.
    for bits in 1..=255u32 {
        for _ in 0..40 {
            check_wide_decimal(&dec, random_with_bits(&mut rng, bits));
            checked += 1;
        }
    }
    // Decimal palindromes of 1 to 77 digits (10^77 < 2^256, so all fit), and one-digit mutations
    // of them (outer pair, inner pairs past the six tracked digits, the middle).
    for len in 1..=77usize {
        for _ in 0..40 {
            let half = len.div_ceil(2);
            let mut s: Vec<u8> = (0..half).map(|_| b'0' + rng.below(10) as u8).collect();
            s[0] = b'1' + rng.below(9) as u8;
            let mut full = s.clone();
            full.extend(s[..len / 2].iter().rev());
            let text = String::from_utf8(full.clone()).unwrap();
            let l = reference::parse_decimal(&text);
            assert!(is_palindrome_wide(l, 10), "{text}");
            check_wide_decimal(&dec, l);
            let pos = rng.below(len as u64) as usize;
            full[pos] = b'0' + ((full[pos] - b'0' + 1 + rng.below(9) as u8) % 10);
            if full[0] != b'0' {
                let text = String::from_utf8(full).unwrap();
                check_wide_decimal(&dec, reference::parse_decimal(&text));
            }
            checked += 2;
        }
    }
    // Every case of the u128 test (`tests/engine.rs`), so the wide check is at least as strong
    // below 10^38, and the u128 reference agrees with the wide reference on them.
    let mut cases: Vec<u128> = vec![1, 9, 11, 121, 7447, 9009, 1_234_554_321, 7_451_111_547];
    for half in [12345678901234567u128, 98765432109876543, 10000000000000001] {
        let s = half.to_string();
        let rev: String = s.chars().rev().collect();
        for p in [format!("{s}{rev}"), format!("{s}{}", &rev[1..])] {
            let n: u128 = p.parse().unwrap();
            cases.push(n);
            cases.push(n + 10u128.pow(digit_count(n, 10) / 2));
            cases.push(n + 1);
        }
    }
    let mut x: u128 = 0x9e37_79b9_7f4a_7c15_f39c_c060_5ced_c834;
    for _ in 0..200_000 {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        cases.push(x >> (x % 100));
    }
    for n in cases.into_iter().filter(|&n| n > 0 && n < 10u128.pow(38)) {
        let l = u256::from_u128(n);
        assert_eq!(is_palindrome_wide(l, 10), is_palindrome(n, 10), "n = {n}");
        check_wide_decimal(&dec, l);
        checked += 1;
    }
    assert!(checked > 200_000, "only {checked} values checked");
}

/// The wide reference agrees with the u128 reference in every base the search uses.
#[test]
fn wide_reference_agrees_with_u128_reference() {
    let mut x: u128 = 0x0123_4567_89ab_cdef_fedc_ba98_7654_3210;
    for i in 0..50_000u32 {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        let n = if i < 2000 { i as u128 } else { x >> (x % 128) };
        for base in [2u32, 10, 12, 16] {
            assert_eq!(
                is_palindrome_wide(u256::from_u128(n), base),
                is_palindrome(n, base as u128),
                "n = {n} base {base}"
            );
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The log, status and the CLI past 126 bits.

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("palhunt-wide-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

#[test]
fn log_holds_both_engines_and_rejects_the_wrong_tag() {
    let dir = temp_dir("log");
    let checkers = palhunt::unit::Checkers::new();
    let mut log = state::Log::open(&dir).unwrap();
    let unit_bits = 12;
    let mut written = 0;
    for bits in (1..=12).chain([129, 130]) {
        let Some(len) = Length::new(bits) else { continue };
        let (_, units) = len.unit_plan(unit_bits);
        for i in 0..units {
            log.append(&len.run(unit_bits, i, &checkers)).unwrap();
            written += 1;
        }
    }
    // Records tagged with the wrong engine for their length must be ignored, never counted.
    let mut wrong_narrow = Length::new(129).unwrap().run(unit_bits, 0, &checkers);
    wrong_narrow.engine = ENGINE.into();
    let mut wrong_wide = Length::new(5).unwrap().run(unit_bits, 0, &checkers);
    wrong_wide.engine = ENGINE_U256.into();
    wrong_wide.fold = "ffffffffffffffff".into();
    log.append(&wrong_narrow).unwrap();
    log.append(&wrong_wide).unwrap();
    drop(log);

    let done = state::load(&dir).unwrap();
    assert_eq!(done.len(), written);
    for (id, r) in &done {
        assert_eq!(Some(r.engine.as_str()), engine_for(id.bits), "{id:?}");
    }
    assert_ne!(done[&UnitId { bits: 5, index: 0 }].fold, "ffffffffffffffff");
    let st = status(&done, 130, unit_bits);
    assert_eq!(frontier(&st), 12);
    for bits in [129, 130] {
        let s = &st[bits as usize - 1];
        assert!(s.complete() && s.units > 0, "bits {bits}: {s:?}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

const BIN: &str = env!("CARGO_BIN_EXE_palhunt");

fn palhunt(args: &[&str]) -> (bool, String) {
    let out = std::process::Command::new(BIN).args(args).output().unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.success(), text)
}

fn path_str(p: &Path) -> &str {
    p.to_str().unwrap()
}

/// `search`, `verify` and `status` past 126 bits through the real binary. 129 and 130 bits have
/// 16 free classes each, so this is fast enough for every CI run.
#[test]
fn cli_searches_verifies_and_reports_past_126_bits() {
    let dir = temp_dir("cli");
    let d = path_str(&dir);
    let (ok, out) = palhunt(&[
        "search",
        "--from-bits",
        "129",
        "--to-bits",
        "130",
        "--unit-bits",
        "10",
        "--state-dir",
        d,
    ]);
    assert!(ok, "{out}");
    assert!(out.contains(ENGINE_U256), "{out}");
    let done = state::load(&dir).unwrap();
    assert_eq!(done.len(), 2 * 64);
    assert!(
        done.values()
            .all(|r| r.engine == ENGINE_U256 && r.candidates == 1024)
    );

    let (ok, out) = palhunt(&["verify", "--sample", "1000", "--state-dir", d]);
    assert!(ok, "{out}");
    assert!(out.contains("128 units reproduced exactly"), "{out}");

    let (ok, out) = palhunt(&[
        "status",
        "--to-bits",
        "130",
        "--unit-bits",
        "10",
        "--state-dir",
        d,
    ]);
    assert!(ok, "{out}");
    assert!(out.contains("128 units logged"), "{out}");
    assert!(!out.contains("bits 129:") && !out.contains("bits 130:"), "{out}");
    // Lengths below 129 were not searched, so the frontier does not move.
    assert!(out.contains("verified frontier: none yet"), "{out}");

    // A re-run resumes: nothing left to do.
    let (ok, out) = palhunt(&[
        "search",
        "--from-bits",
        "129",
        "--to-bits",
        "130",
        "--unit-bits",
        "10",
        "--state-dir",
        d,
    ]);
    assert!(ok, "{out}");
    assert!(!out.contains("bits 129:"), "{out}");
    assert_eq!(state::load(&dir).unwrap().len(), 128);

    // The widest engine stops at 255 bits.
    let (ok, out) = palhunt(&["search", "--to-bits", "256", "--state-dir", d]);
    assert!(!ok && out.contains("1..=255"), "{out}");
    let (ok, _) = palhunt(&["status", "--to-bits", "256", "--state-dir", d]);
    assert!(!ok);
    let _ = std::fs::remove_dir_all(&dir);
}
