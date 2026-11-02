# Palindrome Hunt

A distributed search for a number that is a palindrome in base 2, base 10 and base 16 at once. Apart from the single digits 1, 3, 5, 7 and 9, none is known. Every base-2+16 palindrome below 2^126 has been checked (2 Nov 2026).

- **What and why:** [docs/SPEC.md](docs/SPEC.md) (the maths, why a hit is unlikely, and the unit and result formats)
- **Order of work and Definitions of Done:** [docs/ROADMAP.md](docs/ROADMAP.md)
- **Status:** GitHub Issues, labelled `phase:<n>`. Nothing else tracks status.
- **Working on it with Claude:** [docs/PROJECT_INSTRUCTIONS.md](docs/PROJECT_INSTRUCTIONS.md) (the orchestrator chat) and [docs/ORCHESTRATOR_CONTEXT.md](docs/ORCHESTRATOR_CONTEXT.md) (agents, invariants, CI). Coding agents read [CLAUDE.md](CLAUDE.md).

## Layout

| Path | What |
| :--- | :--- |
| `src/layout.rs` | Which bits are free once a number is a palindrome in base 2 and 16 |
| `src/digits.rs` | Reference palindrome check and the fast decimal check |
| `src/unit.rs` | Work units and their results (the distribution format) |
| `src/state.rs` | Append-only results log, resume after a crash |
| `src/lib.rs` | Progress and the verified frontier |
| `src/main.rs` | The `palhunt` CLI |
| `tests/engine.rs` | Brute-force and reference cross-checks |
| `docs/` | Spec, roadmap, orchestrator docs, handoffs |
| `scripts/` | One-off GitHub setup and seed issues |

## Build and run

Needs Rust (`rustup`); the toolchain version is pinned in `rust-toolchain.toml` and installs itself.

```bash
cargo build --release
./target/release/palhunt search --to-bits 126   # uses every core; safe to stop and re-run
./target/release/palhunt status                 # frontier and hits
./target/release/palhunt verify --sample 50     # re-run logged units, must match exactly
./target/release/palhunt brute --to-bits 28     # check the enumerator against brute force
cargo test --release
```

Results go to `./palhunt-state/results.jsonl` (change with `--state-dir`). Stopping the search at any point loses at most the units in flight.

## Setting up the repo (once)

```bash
gh repo create FLAiRistaken/palindrome-hunt --public --source=. --push
scripts/bootstrap-github.sh --dry-run    # preview labels, merge settings and seed issues
scripts/bootstrap-github.sh              # apply them
```

Then protect `main` (issue P0-4 has the click path) and create the claude.ai Project from `docs/PROJECT_INSTRUCTIONS.md`.

## License

Not chosen yet (open decision in the roadmap). Until one is added, the code is all rights reserved.
