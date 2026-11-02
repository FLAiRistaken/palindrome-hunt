# Handoff: 2 Nov 2026, Phase 0 done

Previous handoff: `docs/plans/HANDOFF_2026-11-02.md` (history only).

## Read first
- **Live state:** open issues for Phase 1 (`gh api 'repos/FLAiRistaken/palindrome-hunt/issues?state=open&labels=phase:1'`) and `docs/ROADMAP.md`.
- **Sources of truth:** `docs/ROADMAP.md`, `docs/ORCHESTRATOR_CONTEXT.md` (0.4), `docs/SPEC.md` (section 7 has the new measurements).

## Done
- Phase 0 is complete on the Mac. #1 (verified from a fresh clone; its PR #6 was closed because an empty diff exposed a CI bug), #3 (PR #8, CI reports all four required checks on docs-only PRs by skipping steps, not jobs; proven by docs-only PR #9), #4 (ruleset `protect-main` read back and matches), #5 (PR #11, token-savior works on Rust on the Mac; `.mcp.json` committed), #7 (PR #10, `AGENTS.md` symlink to `CLAUDE.md`), #2 (Mac half).
- P0-2 results (Apple M4 Pro, 14 cores): 126 bits in 7.2 to 7.8 s; `verify --sample 200` 200/200; SIGKILL at 648 units resumed to identical totals. One core does about 273 M/s but the default unit size caps the whole machine at about 1.0 G/s, because of per-unit log overhead; at `unit_bits` 24 it runs 2.6 G/s on about 13 cores. Tables in `SPEC.md` section 7. Filed as P1-1 (#13).
- Decisions (2 Nov 2026): the Windows PC is not in use yet, so the PC run and Mac/PC cross-verification moved from Phase 0 to the start of Phase 1 (Joe). Pinning the `token-savior-recall` version is parked, not wanted now (#12, Joe). `AGENTS.md` is a symlink to `CLAUDE.md` (Joe); `.codex/` is not committed and Codex is not a routed agent.

## Open now
- Nothing in flight. Open issues: #12 (low priority, parked), #13 (P1-1, ready).
- Joe: when the PC is available, run the engine there and cross-verify both ways (now a Phase 1 Definition of Done item). Which CPU the PC has, and whether the two machines share a network, are still unanswered.
- Joe: delete the stale remote branch `origin/p0/1-verify-skeleton` if it is still there (the orchestrator's delete was blocked by the tool's safety check).

## Next, in order
1. P1-1 (#13): take the results log off the critical path. Cloud Claude Code, Sonnet 5.5, `high`, direct mode. Orchestrator reviews the diff against invariants 1 to 4. Joe measures the speedup on the M4 Pro (CI can't).
2. Decide, before the wide-integer work starts: the Phase 1 gate and the order of Phases 2 and 3 (proposals in `ROADMAP.md` "Open decisions"), the wide integer type (hand-rolled u256 vs a crate, after a quick benchmark), and the license. Ask Joe; these are not decided.
3. Create the remaining Phase 1 issues once those are answered.

## Ways of working
- The orchestrator reads and writes GitHub with `gh api` REST only; `gh pr` / `gh issue` commands fail (GraphQL blocked). It can merge a PR when Joe says so, and it posts the review as a PR comment first.
- Required checks must report under their real names: never skip a matrix job at job level (found on #6, fixed in #8). The `PR has Closes` check fails once per PR because it fires on `opened` before the label is added; harmless, not required.
- The new-project-bootstrap skill still says to skip required checks at job level; its patch for matrix jobs, and `templates/mcp.json` with the token-savior ignore lines, are pending (proposed once, token-savior part only).
- This chat's orchestrator ran on Sonnet 5.5; `ORCHESTRATOR_CONTEXT.md` section 5 still lists Opus 5.5 for orchestrator chats. Joe to say whether to change it.
- `PROJECT_INSTRUCTIONS.md` did not change; no need to re-paste it.
