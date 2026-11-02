# Palindrome Hunt: orchestrator context

Version 0.1, 2 Nov 2026. `ROADMAP.md` decides sequencing and Definitions of Done and wins over this file. `SPEC.md` says what the search does and the formats machines must agree on. This file says how we work on it.

## 1. What this is

A distributed search for a number that is a palindrome in base 2, base 10 and base 16 at once, running on every machine it's installed on and never losing track of what has been searched. One repo, one Rust crate (`palhunt`), no backend until Phase 2. Single developer (Joe) with coding agents; public repo.

## 2. Decisions

Confirmed (2 Nov 2026):
- Lite tier: one repo, `main` only, protected, squash-merged PRs. Phase 0 first, capped at one evening.
- Name `palindrome-hunt`, public on GitHub from day one (free CI minutes on all three OSes).
- Bases 2, 10 and 16 are required; base 12 is recorded per hit but not required.
- Distribution-ready formats (work units, checksums, append-only results log) from Phase 1; the coordinator itself in Phase 2, behind a gate (see `ROADMAP.md`).
- Rust, one codebase for macOS, Windows and Linux. CPU via rayon first; GPU via `wgpu` later. No NPU: it does low-precision matrix maths, and this is exact integer arithmetic.
- Toolchain pinned in `rust-toolchain.toml` (1.97.0) so CI can't break on a Rust release with no change in the repo.

Measured during the bootstrap (2 Nov 2026), on a 2-core Linux sandbox:
- Enumerator equals brute force for every length up to 28 bits.
- Every base-2+16 palindrome below 2^126 tested in 41 seconds, 150 to 210 M candidates/s. Hits: 1, 3, 5, 7, 9 only. 25 sampled units re-ran identically.

Measured on Joe's M4 Pro (2 Nov 2026, P0-2): 126 bits in 7.2 to 7.8 s; one core about 273 M candidates/s; kill and resume proven (SIGKILL at 648 units, restart redid nothing logged); the default unit size leaves the 14 cores mostly idle because of per-unit log overhead (P1-1, #13). Details in `SPEC.md` section 7.

Proposed, not yet confirmed:
- Orchestrator chats on Opus 5.5 at `medium`; implementation on Sonnet 5.5 (section 5).
- Phase 2 gate replaced by Phase 1's Definition of Done, and Phases 2 and 3 swapped (`ROADMAP.md`, "Open decisions").

Open: see `ROADMAP.md`, "Open decisions".

Process cap: after Phase 0, changes to workflow, templates or CI happen only when something actually hurt, as a `type:chore` issue. Building the process is not the project.

## 3. Repo layout

See `README.md`.

## 4. Invariants (protect these in every review)

1. **A hit is only a hit after the reference check.** Fast paths may reject, never accept: every reported hit is re-confirmed by `digits::is_palindrome` in base 2, 10 and 16.
2. **A unit result is a pure function of its id.** (format version, engine, bits, unit_bits, unit) determines every field except `elapsed_ms`: no dependence on thread count, platform, timing or order of completion. Anything that changes what a field means bumps `FORMAT_VERSION` or `ENGINE` and updates `SPEC.md`.
3. **The cross-checks don't get weaker.** The brute-force equality test, the fast-vs-reference decimal test and the CI smoke run (frontier 80, exactly the five trivial hits) are never loosened, skipped or shortened to get green. A new engine must reproduce the old engine's results where both run.
4. **The frontier is never extrapolated.** It is claimed only from lengths where every unit is logged and verifiable.
5. **The results log is append-only.** Nothing rewrites or reorders `results.jsonl`; repair means appending.
6. **No floating point in the search or the checks.** Exact integer arithmetic only. Floats are fine for reporting speed.
7. **The heuristic stays honest.** Docs and any public page describe the expected number of solutions as tiny and the goal as a verified bound; never "close to a discovery".

## 5. Agents and routing

Adapted from the Pivot routing table (researched 20 Oct 2026; re-check when a model changes). This is a Rust CLI, so unlike an iOS app most agents can build and test it on Linux.

| Agent and model | Use it here for | Effort |
| :--- | :--- | :--- |
| Opus 5.5 | Orchestrator chats; the wide-integer engine, the pruning engine, the coordinator's lease and trust logic, reviews of anything touching invariants 1 to 4 | `medium`; `high` for engine correctness |
| Sonnet 5.5 (Claude Code, local or cloud) | Well-specified features, CLI, tests, CI fixes, docs | `medium`; `high` for longer tasks; `low` only for read-only verification with each check's output pasted |
| Haiku 5.5 | Tiny mechanical chores | `medium` |
| Antigravity, Gemini 3.8 Flash | Mechanical multi-file sweeps with grep-checkable acceptance | `low` or `medium` |
| Antigravity, Gemini 4 Argon | Read-only audits | `medium` |
| Jules | Dependency and action-version bumps CI fully verifies | n/a |
| Joe | Anything on his Mac or Windows PC: benchmarks, cross-machine verification, GPU runs | n/a |

What each can verify:
- Build, `cargo test`, clippy and the smoke run: any agent whose sandbox has a Rust toolchain. Cloud Claude Code had one at bootstrap (it can't download a pinned toolchain through the proxy, so run `cargo +stable` there). Antigravity and Jules: unverified, check on first use.
- Real throughput, all-cores behaviour on Apple Silicon, Windows behaviour and GPU: only Joe's machines (or local Claude Code on the Mac). CI proves correctness on all three OSes, not speed.

Every agent prompt states agent, model, effort and plan vs direct mode; what must change and how it is proven; `Closes #N`, the `phase:` label and the PR template; that the agent never merges and never touches secrets or deployment settings; and that it runs as one session per task.

Navigation tooling (token-savior): works on Rust (checked 2 Nov 2026, P0-5). `find_symbol Layout` returned `src/layout.rs` lines 15-21 (the struct) and `get_function_source run_unit` returned the full function from `src/unit.rs`. `.mcp.json` uses an absolute Mac path for `uvx`, so the server only starts on Joe's Mac; on the PC, in cloud Claude Code and in Jules prompts use plain Read/Grep.

## 6. Verification and CI

- CI is the source of truth. Required checks: "lint", "test (ubuntu-latest)", "test (macos-latest)" and "test (windows-latest)". Each job always runs, so each reports under its real name; on a docs-only PR the `changes` job outputs code=false and every step of `lint` and `test` is skipped, so the jobs finish green in seconds. Never skip at job level: a job-level `if:` on the matrix job reports one check literally named `test (${{ matrix.os }})`, the three real names never appear and the PR is blocked forever (found on PR #6, fixed in #8). `pr-hygiene` reports a missing `Closes #N` or `phase:` label but is not required.
- The orchestrator's review comment on a PR counts as the verify pass for Antigravity and Jules branches. It reads the diff, confirms CI is green and lists anything CI can't see.
- Joe verifies anything only his machines can show (speed, GPU, two-machine runs). Nothing is "done" until it has run there.

Branch protection for `main` (Joe sets it; orchestrator sessions can't change repo settings): pull request required; approvals 0 (GitHub doesn't let an author approve their own PR); required checks as above; require branches up to date; block force pushes; restrict deletions. A check only appears in the picker after it has run once. Merge settings are squash-only with head branches deleted (`scripts/bootstrap-github.sh` sets them).

## 7. Manual steps only Joe can do

- Create the GitHub repo and run `scripts/bootstrap-github.sh` from his machine.
- Protect `main` after the first green CI run (P0-4).
- Create the claude.ai Project and paste `docs/PROJECT_INSTRUCTIONS.md`.
- Run the engine on the Mac and the Windows PC (P0-2), and anything else that needs his hardware.
- Pick a license (open decision).
- Phase 2 onward: create the Railway project and database, and any tokens. Never paste tokens into a chat.

## Changelog

- 0.1, 2 Nov 2026: created from the plan agreed in chat and the Pivot workflow (skill `new-project-bootstrap`). Rust profile derived for this project; the skill has no Rust profile yet.
- 0.2, 2 Nov 2026: section 6 corrected: docs-only PRs skip steps, not jobs (see above).
- 0.3, 2 Nov 2026: section 5: token-savior checked on Rust in P0-5 (works on the Mac only).
- 0.4, 2 Nov 2026: Phase 0 measurements recorded (section 2); PC half of P0-2 moved to Phase 1.
