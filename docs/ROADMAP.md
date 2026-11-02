# Palindrome Hunt: roadmap

Sequencing and Definitions of Done only. **Status lives in GitHub Issues** labelled `phase:<n>`, not here. Phases run in order; nothing is pulled forward from a later phase without Joe's say-so.

Phases 1 to 5 come from Joe's brief and the plan agreed in chat on 2 Nov 2026 (engine on every machine using all its hardware, never losing track, built for distribution from the start). Phase 0 was added by the orchestrator. The bootstrap already landed the u128 engine, so Phase 0's skeleton is done and Phase 1 starts past 126 bits.

## Gate between phases

Confirmed by Joe (2 Nov 2026): the coordinator (Phase 2) starts only once the single-machine engine has verified every candidate to 100 bits and matches brute force exactly. **That gate was met during the bootstrap** (126 bits in 41 seconds, brute force matched to 28 bits), so it no longer holds anything back. Proposed replacement, not yet confirmed: Phase 2 starts only when Phase 1's Definition of Done is met, including results reproducing exactly on Joe's Mac and PC.

Tooling, process and extra features are not a way around the gate: a change that isn't needed to finish the current phase waits for it, or gets filed as a `type:chore` issue.

## Phase 0: workflow, skeleton and the riskiest-unknown spike (one evening)

Done when:
- The repo builds and tests from the documented commands, locally and in CI.
- All four required checks are green on a PR, and `main` is protected with squash-only merges.
- The engine runs on Joe's Apple Silicon Mac, resumes after being killed, reaches the 126-bit frontier with only the trivial hits, and its log verifies (P0-2, Mac half; done 2 Nov 2026). **Amended by Joe, 2 Nov 2026:** the Windows PC is not in use yet, so the PC run and the cross-machine verification (both directions) moved to the start of Phase 1. The Mac run also found that the engine uses about 4 of 14 cores at the default unit size because of per-unit log overhead; that fix is P1-1 (#13) and is not a Phase 0 blocker.
- token-savior is checked against Rust and the result is in `ORCHESTRATOR_CONTEXT.md`.

Issues: P0-1 to P0-5 (`scripts/seed-issues/`).

## Phase 1: wide engine and distribution-ready formats

Done when:
- The engine searches past 126 bits with a wider integer type, and reproduces every u128 unit result exactly (same `candidates`, `pass1`, `fold`, `hits`) for all lengths up to 126 bits. Verified by a test.
- Results are tagged with a new `engine` id; the format change, if any, is documented in `SPEC.md` with a bumped `v`.
- `palhunt search` reaches 160 bits on Joe's Mac (Joe verifies; candidates per second per core recorded in `SPEC.md` section 7).
- A unit can be handed to and run on another machine from its id alone (`palhunt run-unit` or equivalent), and its result verifies on the first machine. This is the client half of Phase 2.
- `palhunt status` and the log work unchanged for lengths past 126.
- Carried from Phase 0: the engine runs on the Windows PC, and results logs from the Mac and the PC verify exactly on each other in both directions (`verify --sample 200`).
- P1-1 (#13) is done: at the default unit size the Mac search is no longer limited by the results log (target: near the 2.6 G/s measured at `unit_bits` 24; Joe measures).

Issues: P1-1 (#13) exists; the rest are created when Phase 1 starts.

## Phase 2 and later

- **Phase 2: coordinator and multi-device client.** A small API plus Postgres hands out units as leases with an expiry; clients checkpoint locally, submit results, and pick up expired leases. A random 5% of units are re-issued to a second machine and must match. The coordinator tracks the verified frontier. Done when Joe's Mac and PC work the same search together, a killed client's units are reassigned automatically, and a deliberately wrong result is caught.
- **Phase 3: both-ends pruning engine.** Top bits fix the top decimal digits, which (decimal palindrome) fix the bottom decimal digits, which fix the low bits, which (binary palindrome) fix the top bits again; inconsistencies cut whole subtrees. Done when it gives identical hits and frontier to the Gray-code engine on every length it can both reach, and its candidates-per-second-equivalent is measured.
- **Phase 4: GPU backend.** `wgpu` compute shaders (Metal on the Mac, DirectX or Vulkan on the PC). Done when GPU unit results are identical to CPU results for the same units, and the speedup is measured on both machines.
- **Phase 5: public release.** Installers for macOS, Windows and Linux, accounts so one person's devices add up, a public progress page, and a short write-up of the heuristic. Done when someone other than Joe installs it and their results count toward the frontier.

## Open decisions

| Decision | Proposal | Needed by |
| :--- | :--- | :--- |
| Order of Phases 2 and 3 | Swap them: pruning first. Brute force costs about 2x per extra 4 bits, so 10x more machines buys about 13 bits; pruning should roughly double the reachable bits. A heavy length at 200 bits is about 2^49 candidates, days on one Mac; at 256 bits about 2^63, centuries even across a large fleet. | End of Phase 1 |
| Wide integer type | Hand-rolled fixed-width (u256 to start) over a crate, so the hot loop stays allocation-free and the GPU port has the same arithmetic. Decide after a quick benchmark against a crate such as `bnum`. | Start of Phase 1 |
| Replacement for the met Phase 2 gate | Phase 1 Definition of Done, including cross-machine reproduction (see "Gate" above). | Start of Phase 1 |
| License | TBD (decide in Phase 0). The repo is public; without a license nobody else may use the code. Rust convention is MIT OR Apache-2.0. | Before anyone else contributes, at the latest Phase 5 |
| Coordinator hosting | Railway plus Postgres (Joe already uses both). | Start of Phase 2 |

Small questions found while writing the issues (answer before the issue starts):
- Which CPU is in the Windows PC, and is it on the same network as the Mac? (P0-2 copies a results log between them.)
- Should the default state directory move from `./palhunt-state` to the platform data directory before Phase 2? Not needed for Phase 0.
