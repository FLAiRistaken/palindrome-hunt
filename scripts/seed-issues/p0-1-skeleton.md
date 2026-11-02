---
title: P0-1 Skeleton: confirm the bootstrap engine builds and tests from a fresh clone
labels: phase:0,type:chore
---
## Task
The bootstrap commit already contains the u128 engine, tests and CLI. Confirm it builds and tests from a fresh clone with the documented commands, and that the commands in README "Build and run" and CLAUDE.md "Commands" are exactly right. No features.

## Acceptance
- From a fresh clone: `cargo test --release` passes (8 integration tests), `cargo fmt --check` and `cargo clippy --all-targets --release -- -D warnings` are clean. Output pasted.
- `cargo run --release -- brute --to-bits 20` ends with "enumerator matches brute force".
- `target/`, `palhunt-state/` and `ci-state/` are gitignored.
- Any command in README or CLAUDE.md that doesn't work as written is fixed in the PR.

## Route
agent: claude-cloud   model: Sonnet 5.5   effort: low   mode: direct (read-only verification plus doc fixes; paste each check's output)

## Depends on
none
