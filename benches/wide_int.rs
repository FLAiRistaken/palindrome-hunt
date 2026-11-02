//! Wide-integer spike (#22): candidates per second per core for the engine's hot loop over
//! different integer types. Not part of `search`; nothing in `src/` uses it.
//!
//! ```text
//! cargo bench --bench wide_int -- --label "Apple M4 Pro, 14 cores"
//! cargo bench --bench wide_int -- --label "Ryzen 7 5800X3D" --threads 1
//! cargo bench --bench wide_int -- --threads 14 --label "Apple M4 Pro, 14 cores"
//! ```
//!
//! Options: `--label TEXT`, `--bits 124,128,160,...`, `--unit-bits N` (default 24),
//! `--reps N` (default 5), `--threads N` (default 1). With one thread this is the single-core
//! rate. With more, every thread runs its own units at the same time and the rate shown is still
//! per core (total candidates / wall time / threads), so the two runs can be compared directly.
//!
//! Every type runs the same units from the same code (`wide::engine::run_wide_unit`), and the
//! benchmark stops if any two types disagree on candidates, pass1, fold or hits. At 126 bits and
//! below the production `run_unit` is timed as well and must agree too.

mod wide;

use bnum::types::U256 as BnumU256;
use palhunt::digits::DecimalChecker;
use palhunt::layout::layout;
use palhunt::unit::run_unit;
use std::hint::black_box;
use std::time::Instant;
use wide::engine::{Wide, WideDecimal, WideLayout, WideUnitResult, run_wide_unit, wide_layout};
use wide::u256::U256;

struct Opts {
    label: String,
    bits: Vec<u32>,
    unit_bits: u32,
    reps: usize,
    threads: usize,
}

fn parse_args() -> Opts {
    let mut o = Opts {
        label: "unlabelled machine".into(),
        bits: vec![124, 128, 160, 192, 224, 252],
        unit_bits: 24,
        reps: 5,
        threads: 1,
    };
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        let mut value = |name: &str| args.next().unwrap_or_else(|| panic!("{name} needs a value"));
        match a.as_str() {
            "--bench" => {} // passed by `cargo bench`
            "--label" => o.label = value("--label"),
            "--bits" => {
                o.bits = value("--bits")
                    .split(',')
                    .map(|s| s.trim().parse().unwrap())
                    .collect()
            }
            "--unit-bits" => o.unit_bits = value("--unit-bits").parse().unwrap(),
            "--reps" => o.reps = value("--reps").parse().unwrap(),
            "--threads" => o.threads = value("--threads").parse().unwrap(),
            other => panic!("unknown option {other}"),
        }
    }
    assert!(o.reps >= 1 && o.threads >= 1);
    o
}

struct Measured {
    name: &'static str,
    /// Candidates per second per core, one entry per repetition.
    rates: Vec<f64>,
    results: Vec<WideUnitResult>,
}

impl Measured {
    fn median(&self) -> f64 {
        let mut r = self.rates.clone();
        r.sort_by(|a, b| a.total_cmp(b));
        r[r.len() / 2]
    }
    fn min(&self) -> f64 {
        self.rates.iter().copied().fold(f64::INFINITY, f64::min)
    }
    fn max(&self) -> f64 {
        self.rates.iter().copied().fold(0.0, f64::max)
    }
}

/// Which unit thread `t` runs in repetition `rep`; identical for every type.
fn unit_for(t: usize, rep: usize, reps: usize, units: u128) -> u128 {
    ((t * reps + rep) as u128) % units
}

/// Time `run` over `reps` repetitions on `threads` threads. `run(unit)` returns the result.
fn time_runs<F>(o: &Opts, units: u128, name: &'static str, run: F) -> Measured
where
    F: Fn(u128) -> WideUnitResult + Sync,
{
    black_box(run(unit_for(0, 0, o.reps, units))); // warm-up
    let mut rates = Vec::new();
    let mut results = Vec::new();
    for rep in 0..o.reps {
        let started = Instant::now();
        let round: Vec<WideUnitResult> = if o.threads == 1 {
            vec![run(unit_for(0, rep, o.reps, units))]
        } else {
            std::thread::scope(|s| {
                let handles: Vec<_> = (0..o.threads)
                    .map(|t| {
                        let (run, unit) = (&run, unit_for(t, rep, o.reps, units));
                        s.spawn(move || run(unit))
                    })
                    .collect();
                handles.into_iter().map(|h| h.join().unwrap()).collect()
            })
        };
        let secs = started.elapsed().as_secs_f64();
        // Per core: the work one thread did, divided by the wall time of the whole round.
        rates.push(round[0].candidates as f64 / secs);
        results.extend(round);
    }
    black_box(&results);
    Measured { name, rates, results }
}

fn measure<T: Wide + Send + Sync>(wl: &WideLayout, o: &Opts, units: u128) -> Measured {
    let prepared = wl.prepare::<T>();
    let dec = WideDecimal::<T>::new();
    time_runs(o, units, T::NAME, |unit| {
        run_wide_unit(&prepared, &dec, o.unit_bits, unit)
    })
}

/// The production engine, converted to the common result shape.
fn measure_production(bits: u32, o: &Opts, units: u128) -> Measured {
    let pl = layout(bits).unwrap();
    let dec = DecimalChecker::new();
    time_runs(o, units, "production run_unit (u128)", |unit| {
        let r = run_unit(&pl, o.unit_bits, unit as u64, &dec);
        WideUnitResult {
            bits,
            unit_bits: r.unit_bits,
            unit,
            candidates: r.candidates,
            pass1: r.pass1,
            fold: u64::from_str_radix(&r.fold, 16).unwrap(),
            hits: r
                .hits
                .iter()
                .map(|h| wide::reference::parse_decimal(&h.n))
                .collect(),
        }
    })
}

fn main() {
    let o = parse_args();
    println!("palhunt wide-int spike (#22)");
    println!("machine : {}", o.label);
    println!("os/arch : {} / {}", std::env::consts::OS, std::env::consts::ARCH);
    println!("build   : cargo bench profile (release settings: opt-level 3, fat LTO, 1 codegen unit)");
    println!(
        "run     : {} thread(s), unit = 2^{} candidates, median of {} runs after 1 warm-up",
        o.threads, o.unit_bits, o.reps
    );
    println!("rate    : million candidates per second per core\n");
    println!(
        "| bits | free | type | M cand/s/core (median) | min to max | vs hand-rolled | pass1 (unit 0) | fold (unit 0) |"
    );
    println!("| ---: | ---: | :--- | ---: | ---: | ---: | ---: | :--- |");

    for &bits in &o.bits {
        let Some(wl) = wide_layout(bits) else {
            println!("| {bits} | - | no base-2 and base-16 palindromes of this length | | | | | |");
            continue;
        };
        let ub = o.unit_bits.min(wl.free());
        let units = 1u128 << (wl.free() - ub);
        let mut all = Vec::new();
        if bits <= 126 {
            all.push(measure_production(bits, &o, units));
            all.push(measure::<u128>(&wl, &o, units));
        }
        let hand = measure::<U256>(&wl, &o, units);
        let bn = measure::<BnumU256>(&wl, &o, units);
        let hand_rate = hand.median();
        all.push(hand);
        all.push(bn);

        // Every type must have produced identical results for every unit.
        for m in &all[1..] {
            assert_eq!(
                m.results, all[0].results,
                "{} disagrees with {} at {bits} bits",
                m.name, all[0].name
            );
        }
        for m in &all {
            let r = &m.results[0];
            println!(
                "| {bits} | {} | {} | {:.1} | {:.1} to {:.1} | {:.2}x | {} | {:016x} |",
                wl.free(),
                m.name,
                m.median() / 1e6,
                m.min() / 1e6,
                m.max() / 1e6,
                m.median() / hand_rate,
                r.pass1,
                r.fold
            );
        }
    }
    println!("\nAll types produced identical candidates, pass1, fold and hits for every unit run.");
}
