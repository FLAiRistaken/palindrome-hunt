# Palindrome Hunt: agent guide

A distributed search for a number that is a palindrome in base 2, base 10 and base 16 at once. Your task is the GitHub issue you were given; its acceptance checks are the definition of done. Read `docs/ORCHESTRATOR_CONTEXT.md` section 4 (Invariants) and `docs/SPEC.md` before you change behaviour.

## Rules

- One task, one branch, one PR into `main`. Branch name `p<phase>/<issue>-<slug>`. The PR body uses the template, contains `Closes #N`, and the PR has the `phase:<n>` label.
- Never merge. Never change secrets, deployment or production settings. Never skip, delete or loosen a test to get green.
- Never put a secret or token in code, tests, fixtures, logs or a PR.
- A hit is reported only after `digits::is_palindrome` confirms it in base 2, 10 and 16. Fast paths may reject, never accept.
- A unit result must depend only on (format version, engine, bits, unit_bits, unit): not on threads, platform, timing or completion order. Changing what a result field means bumps `FORMAT_VERSION` or `ENGINE` and updates `docs/SPEC.md`; only do that when the issue says so.
- The brute-force test, the fast-vs-reference decimal test and the CI smoke run are never weakened. A new engine must reproduce the old engine's results where both run.
- `results.jsonl` is append-only. No floating point in the search or the checks.

## Commands

```bash
cargo test --release                                  # unit and integration tests
cargo fmt --check && cargo clippy --all-targets --release -- -D warnings
cargo run --release -- brute --to-bits 20             # enumerator vs brute force
cargo run --release -- search --to-bits 80 --state-dir /tmp/ph
cargo run --release -- verify --sample 25 --state-dir /tmp/ph
cargo run --release -- status --to-bits 80 --state-dir /tmp/ph
```

The toolchain is pinned in `rust-toolchain.toml`. If your sandbox can't download it, use `cargo +stable` and say so in the PR.

## When you can't verify

Say so in the PR and list the checks you could not run; do not claim them. CI runs on Linux, macOS and Windows on every PR; a red check blocks the merge. Speed and GPU results can only be measured on Joe's machines: list them as unverified.
