# Handoff: 2 Nov 2026, Phase 1 under way

Previous handoff: `docs/plans/HANDOFF_2026-11-02_phase0-done.md` (history only). This is the newest handoff; the file name sorts last on purpose.

## Read first
- **Live state:** open issues for Phase 1 (`gh api 'repos/FLAiRistaken/palindrome-hunt/issues?state=open&labels=phase:1'`) and `docs/ROADMAP.md`.
- **Sources of truth:** `docs/ROADMAP.md`, `docs/ORCHESTRATOR_CONTEXT.md` (0.6), `docs/SPEC.md` (section 7 has all measurements).

## Done
- **P1-1 (#13, PR #19):** the results log is off the critical path (group commit: one writer thread, a sync per batch of up to 256 units or 200 ms). Unit results, `FORMAT_VERSION` and `ENGINE` unchanged. Joe measured on the M4 Pro at the default `unit_bits` 22: 1.56 to 1.70 s for bits 123 to 124, about 2.7 G/s, about 13 of 14 cores; same as `unit_bits` 24. Numbers are in `SPEC.md` section 7 (PR #21).
- **P1-2 (#22, PR #23):** wide-integer benchmark (`benches/wide_int.rs`, `tests/wide_int.rs`, `bnum` as a dev-dependency only). Hand-rolled u256 was 14 to 33% faster than `bnum` at five of six lengths and 9% slower at 224 bits; both wide types run at about a third of u128 speed per core (71 vs 226 M/s at 124 bits).
- Docs PRs: #16 (orchestrator model), #18 (Phase 1 decisions), #21 (P1-1 numbers), #25 (wide-integer decision and routing).
- **Decisions (2 Nov 2026, Joe):**
  - Phase 2 gate = Phase 1's Definition of Done, including Mac/PC cross-verification.
  - Wide integer type: hand-rolled u256. The PC benchmark is a sanity check, not a blocker.
  - Orchestrator chats run on Sonnet 5.5 at `medium`.
  - Claude Code tasks run as a local session on Joe's Mac (Joe pastes the prompt) whenever his computer is linked; cloud only when it is not. The agent-dispatch MCP is parked until Joe says it is ready.
  - The order of Phases 2 and 3, and the license, are deferred. Ask again at the end of Phase 1.
  - Windows PC: Ryzen 7 5800X3D, same network as the Mac (both reach his Pi-hole).
  - Standing permission: the orchestrator may merge a PR once it has posted its review comment and CI is green, including docs PRs.

## Open now
- Nothing in flight: no open PRs.
- Open issues: #26 (P1-3 wide engine), #27 (P1-4 `run-unit`), #28 (P1-5 Mac run to 160 bits, Joe), #29 (P1-6 PC run and cross-verification, Joe), #12 (parked, low priority).
- Joe has the agent prompts for #26 and #27 from the last orchestrator chat; they are not stored in the repo. If he hasn't sent them, ask him; if they are lost, rewrite them from the issues (acceptance checks are in the issue bodies).
- Joe: when the PC is free, do #29 (the orchestrator gives the Windows commands). Not blocking anything.
- Joe: the `code-prompt-builder` skill was proposed for update twice this chat (local-first routing; dispatch parked). The chat could not see whether he saved it. Ask.

## Next, in order
1. **#26 P1-3 wide engine.** Claude Code, local on the Mac, Opus 5.5, `high`, plan first. It is the long pole: it adds the new engine id, must reproduce every u128 result for all lengths to 126, and moves the hot loop onto the u256 from `benches/wide/`. The orchestrator reviews the diff against invariants 1 to 4, re-runs the tests, and merges once CI is green (this PR changes engine behaviour, so say so in the review).
2. **#27 P1-4 `run-unit`.** Claude Code, local, Sonnet 5.5, `medium`. Can run before or after #26; whichever lands second rebases. Both touch `src/main.rs`, so do not run them at the same time.
3. **#28 P1-5** after #26: Joe runs `search --to-bits 160` on the Mac; the orchestrator gives the command and records the numbers in `SPEC.md` section 7 in a docs PR.
4. **#29 P1-6** whenever the PC is free; rerun the cross-check after #26 if the engine id changed.
5. At the end of Phase 1: confirm the Definition of Done in `ROADMAP.md`, ask Joe about the Phase 2/3 order and the license, then write the next handoff.

## Ways of working
- The orchestrator reads and writes GitHub with `gh api` REST only; `gh pr` and `gh issue` fail (GraphQL blocked). Post the review as a PR comment, then merge with `gh api -X PUT repos/FLAiRistaken/palindrome-hunt/pulls/<n>/merge -f merge_method=squash`.
- The `main` ruleset requires the PR branch to be up to date: if `mergeable_state` says `behind`, call `PUT .../pulls/<n>/update-branch` and wait for CI again.
- The clone in the orchestrator sandbox is shallow and tracks only `main`. After pushing a new branch, add it to `remote.origin.fetch` (or the stop hook reports unpushed commits); the push itself is fine. The pinned toolchain cannot be downloaded there, so use `cargo +stable` for the orchestrator's own checks.
- Docs changes go through a `type:chore` issue and a PR (`Closes #N`, `phase:` label). Wait for the four required checks, post a review comment, then merge.
- Agent-dispatch MCP (Joe's own tool) failed its first real test on #22: the worktree branched from whatever was checked out in the main checkout (a stale PR branch), the job made no commit and no PR yet reported `succeeded` with `agent_ok: true` after 4 permission denials, and its log was unreadable from the orchestrator. Do not dispatch until Joe says it is ready; if he does, check diffstat, commits and PR before believing `succeeded`.
- The prompt skill's routing rule: look at the live tool list to decide local vs cloud; do not copy the agent line from an issue.
- Still pending from earlier: the `new-project-bootstrap` skill patch (matrix jobs must not be skipped at job level; `templates/mcp.json` with the token-savior ignore lines). The `PR has Closes` check fails once per PR because it fires before the label is added; harmless, not required.
- `PROJECT_INSTRUCTIONS.md` did not change; no need to re-paste it. `ORCHESTRATOR_CONTEXT.md` is now at 0.6.
