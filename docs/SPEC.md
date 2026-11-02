# Palindrome Hunt: spec

Version 0.1, 2 Nov 2026. What the search does, why it is shaped the way it is, and the formats other machines must agree on. `ROADMAP.md` decides order of work.

## 1. The question

Is there a number with more than one digit in each base that is a palindrome in base 2, base 10 and base 16 at once?

- Single digits 1, 3, 5, 7 and 9 qualify trivially (one digit in base 10 and 16, binary 1, 11, 101, 111, 1001). They are reported as `trivial`.
- Base 12 is not required. Every hit records whether it is also a base-12 palindrome.
- Base 2 + 10 alone is well studied (OEIS A007632; Charlton Harrison's 83-bit record, 2001). Adding base 16 is much more restrictive, and that is what makes it searchable.

## 2. Search-space reduction

A binary palindrome ties bit i to bit L-1-i. A hex palindrome ties bit 4j+b to bit 4k+b, where hex digit j mirrors k = H-1-j. When L is not a multiple of 4 the top hex digit is padded with zeros, which forces the mirrored low bits to zero. Joining both sets of ties splits the L bits into classes. Every L-bit number that is a palindrome in base 2 and 16 is "the class containing the top bit, plus any subset of the free classes". There are about L/4 free classes, so about 2^(L/4) candidates per length instead of 2^L (exactly 15 at 64 bits, 25 at 100). Lengths with L mod 4 = 1 or 2 have far fewer.

Code: `src/layout.rs`. Proven equal to brute force for every length up to 28 bits (`palhunt brute`), and the tests check it to 22 bits on every CI run.

## 3. Why a hit is unlikely (and what success means)

A random d-digit number is a decimal palindrome with probability about 10^(-d/2), roughly 2^(-L/2) for L bits. Expected hits per length are about 2^(L/4) x 2^(-L/2) = 2^(-L/4). Summed over every length past ~70 bits, the expected number of solutions is about one in a million. This is a heuristic, not a proof.

So the realistic result is a verified bound ("no solution below 2^N"), pushed as far as possible. Success is measured by N and by how trustworthy the bound is, not by a find. Base 2 + 10 alone has expected hits around a constant per length, which is why those keep turning up.

## 4. The search loop

For one length, the free classes are walked in Gray-code order, so each step flips one class: one XOR on the candidate. Alongside, `n mod 10^6` is updated with one add or subtract, which gives the six lowest decimal digits for free. The high decimal digits are read by comparing against a table of k x 10^p (no division). Each digit pair rejects about 90% of what reaches it. Past six pairs, the exact reference check decides. Every hit is re-confirmed in base 2, 10 and 16 by the reference `is_palindrome` before it is reported.

Throughput measured on the bootstrap machine (2 cores, Linux x86-64): 150 to 210 million candidates per second. Everything below 2^126 took 41 seconds.

The engine uses u128, so it stops at 126 bits (10^38 is the largest power of ten that fits). Beyond that needs a wider integer type (Phase 1).

## 5. Work units (the distribution format)

A unit is a contiguous range of Gray-code indices for one length:

- `bits`: L.
- `unit_bits`: log2 of candidates per unit, `min(requested, free classes)`. Default 22 (about 4 million candidates, well under a second).
- `unit`: index, 0 to `units - 1`, where `units = 2^(free - unit_bits)`.

A unit is fully defined by (format version, engine, bits, unit_bits, unit). Its result must be identical on every machine except `elapsed_ms`.

### Unit result (one JSON object per line in `results.jsonl`)

| Field | Meaning |
| :--- | :--- |
| `v` | Format version, currently 1. Bump when any field's meaning changes. |
| `engine` | Algorithm id, currently `cpu-gray-u128-v1`. Results from different engines are never mixed silently. |
| `bits`, `unit`, `units`, `unit_bits` | Which unit. |
| `candidates` | Candidates tested; must equal 2^unit_bits. |
| `pass1` | Candidates whose outermost decimal digits matched. |
| `fold` | 64-bit hash, hex, over every candidate that passed the first pair, in Gray-code order: `fold = (rotl(fold, 5) ^ lo64(n) ^ hi64(n)) * 0x9E3779B97F4A7C15` (wrapping). A machine that skipped work can't produce it. A wider engine must XOR in every 64-bit limb, so results below 128 bits stay identical. |
| `hits` | Each with `n` (decimal string), `hex`, `base12`, `trivial` (n < 16). |
| `elapsed_ms` | Wall time; ignored when comparing machines. |

### Checkpointing

`results.jsonl` is append-only. Each finished unit is written and synced before the next, so a crash or a closed laptop loses at most the units in flight. A torn last line is skipped on load and the file is repaired on the next open. A restart skips every unit already logged.

### Verification

`palhunt verify --sample N` re-runs N logged units and requires identical results. Phase 2 uses the same comparison to check a random share of work from other machines.

### Frontier

`palhunt status` reports the largest N such that every length up to N is complete. Partially done lengths never move the frontier.

## 6. CLI

```
palhunt search --to-bits N [--from-bits M] [--unit-bits B] [--state-dir DIR] [--threads T]
palhunt status [--to-bits N] [--state-dir DIR]
palhunt verify [--sample N] [--state-dir DIR]
palhunt brute  [--to-bits N]        # independent cross-check, up to 36 bits
```

The default state directory is `./palhunt-state`.

## 7. Results so far

| Date | Machine | Verified below | Hits |
| :--- | :--- | :--- | :--- |
| 2 Nov 2026 | Bootstrap sandbox, 2 cores | 2^126 (about 8.5 x 10^37) | 1, 3, 5, 7, 9 (all trivial) |
| 2 Nov 2026 | Apple M4 Pro, 14 logical cores (Joe's Mac) | 2^126 | 1, 3, 5, 7, 9 (all trivial); 1812 units; 200 sampled units re-ran identically; a run killed with SIGKILL at 648 units resumed to identical totals |

Search 126 bits end to end on the M4 Pro at the default unit size (`unit_bits` 22): 7.2 to 7.8 s, about 1.0 G candidates/s.

### Thread scaling (M4 Pro, bits 123 to 124, 2^32 candidates, `unit_bits` 22)

| Threads | Wall time | Rate | User CPU |
| ---: | ---: | ---: | ---: |
| 1 | 15.7 s | 273 M/s | 15.6 s |
| 2 | 8.2 s | 524 M/s | 16.2 s |
| 4 | 4.5 s | 954 M/s | 16.8 s |
| 7 | 4.0 s | 1074 M/s | 17.0 s |
| 14 | 3.9 s | 1101 M/s | 20.8 s |

### Unit size at 14 threads (same work)

| `unit_bits` | Units | Wall time | Rate | User CPU / wall |
| ---: | ---: | ---: | ---: | ---: |
| 22 | 1024 | 4.3 s | 1.0 G/s | about 4.9 |
| 24 | 256 | 1.6 s | 2.6 G/s | about 12.6 |
| 26 | 64 | 1.8 s | 2.3 G/s | about 10.9 |

Reading: one core does about 273 M candidates/s, and the engine scales linearly to 4 threads, then flattens at about 1.1 G/s at the default unit size. With 4x fewer units the same work runs 2.7x faster and uses about 13 of 14 cores, so the limit at the default is per-unit overhead, most likely the per-unit log sync done by one thread. This is a strong inference from timings, not a direct measurement of the sync. Fix: issue #13 (P1-1). `unit_bits` 26 is slower than 24 because 64 units leave the last cores idle.
