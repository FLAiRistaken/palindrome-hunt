use palhunt::digits::{DecimalChecker, LOW_MOD, digit_count, is_palindrome};
use palhunt::layout::layout;
use palhunt::unit::{run_unit, unit_plan};
use palhunt::{frontier, state, status};
use std::collections::HashSet;

/// The enumerator's search space equals brute force for every length up to 22 bits.
#[test]
fn layout_matches_brute_force() {
    for bits in 1..=22u32 {
        let lo = 1u128 << (bits - 1);
        let brute: HashSet<u128> = (lo..lo << 1)
            .filter(|&n| is_palindrome(n, 2) && is_palindrome(n, 16))
            .collect();
        let enumerated: HashSet<u128> = match layout(bits) {
            None => HashSet::new(),
            Some(l) => (0..1u128 << l.free()).map(|g| l.value_at(g)).collect(),
        };
        assert_eq!(brute, enumerated, "bits {bits}");
    }
}

/// Every enumerated value really is an L-bit base-2 and base-16 palindrome, at large sizes too.
#[test]
fn layout_values_are_palindromes_up_to_126_bits() {
    let mut x: u128 = 0x1234_5678_9abc_def0_0fed_cba9_8765_4321;
    for bits in 1..=126u32 {
        let Some(l) = layout(bits) else { continue };
        for _ in 0..50 {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            let g = if l.free() == 0 {
                0
            } else {
                x & ((1u128 << l.free()) - 1)
            };
            let n = l.value_at(g);
            assert_eq!(128 - n.leading_zeros(), bits);
            assert!(is_palindrome(n, 2) && is_palindrome(n, 16), "bits {bits} n {n:x}");
        }
    }
}

/// Free classes are about a quarter of the bits (what makes the search feasible).
#[test]
fn free_bits_scale_as_a_quarter() {
    assert_eq!(layout(64).unwrap().free(), 15);
    assert_eq!(layout(100).unwrap().free(), 25);
}

/// The fast decimal check agrees with the reference on palindromes and near-misses.
#[test]
fn fast_decimal_check_agrees_with_reference() {
    let dec = DecimalChecker::new();
    let mut cases: Vec<u128> = vec![1, 9, 11, 121, 7447, 9009, 1_234_554_321, 7_451_111_547];
    // Long palindromes and one-digit mutations of them, past the 6 tracked low digits.
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
        let mut first = false;
        let fast = dec.is_palindrome(n, (n % LOW_MOD as u128) as u64, digit_count(n, 10), &mut first);
        assert_eq!(fast, is_palindrome(n, 10), "n = {n}");
    }
}

/// Known numbers palindromic in bases 2 and 10 (OEIS A007632) pass the reference check.
#[test]
fn reference_check_on_known_base_2_and_10_palindromes() {
    for n in [
        33u128,
        99,
        313,
        585,
        717,
        7447,
        9009,
        15351,
        32223,
        39993,
        7_451_111_547,
    ] {
        assert!(is_palindrome(n, 2) && is_palindrome(n, 10), "{n}");
    }
}

/// Units are deterministic, cover every candidate once, and find exactly 1, 3, 5, 7, 9 below 2^40.
#[test]
fn units_are_deterministic_and_find_the_trivial_hits() {
    let dec = DecimalChecker::new();
    let mut hits = Vec::new();
    for bits in 1..=40 {
        let Some(l) = layout(bits) else { continue };
        let (ub, units) = unit_plan(&l, 3);
        let mut total = 0;
        for i in 0..units {
            let a = run_unit(&l, 3, i, &dec);
            let b = run_unit(&l, 3, i, &dec);
            assert!(a.same_work(&b));
            assert_eq!(a.candidates, 1 << ub);
            total += a.candidates;
            hits.extend(a.hits.into_iter().map(|h| h.n));
        }
        assert_eq!(total, 1u64 << l.free());
    }
    hits.sort();
    assert_eq!(hits, ["1", "3", "5", "7", "9"]);
}

/// The unit split doesn't change the answer: same hits and pass counts with different unit sizes.
#[test]
fn unit_size_does_not_change_totals() {
    let dec = DecimalChecker::new();
    let l = layout(61).unwrap();
    let total = |ub: u32| {
        let (_, units) = unit_plan(&l, ub);
        (0..units)
            .map(|i| run_unit(&l, ub, i, &dec))
            .fold((0, 0), |(c, p), r| (c + r.candidates, p + r.pass1))
    };
    assert_eq!(total(4), total(10));
}

/// The log survives a torn last line and reports the frontier.
#[test]
fn log_resume_and_frontier() {
    let dir = std::env::temp_dir().join(format!("palhunt-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let dec = DecimalChecker::new();
    let mut log = state::Log::open(&dir).unwrap();
    for bits in 1..=12 {
        if let Some(l) = layout(bits) {
            let (_, units) = unit_plan(&l, 2);
            for i in 0..units {
                log.append(&run_unit(&l, 2, i, &dec)).unwrap();
            }
        }
    }
    drop(log);
    // Simulate a crash mid-write.
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .open(state::log_path(&dir))
        .unwrap();
    f.write_all(b"{\"v\":1,\"engine\":\"cpu-gr").unwrap();
    drop(f);
    let done = state::load(&dir).unwrap();
    assert_eq!(frontier(&status(&done, 20, 2)), 12);
    // Reopening repairs the torn line so new records still parse.
    let mut log = state::Log::open(&dir).unwrap();
    let l = layout(13).unwrap();
    let (_, units) = unit_plan(&l, 2);
    for i in 0..units {
        log.append(&run_unit(&l, 2, i, &dec)).unwrap();
    }
    let done = state::load(&dir).unwrap();
    assert!(frontier(&status(&done, 20, 2)) >= 13);
    let _ = std::fs::remove_dir_all(&dir);
}
