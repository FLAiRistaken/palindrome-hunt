---
title: P0-2 Spike: run the engine on the Mac and the Windows PC, and cross-verify
labels: phase:0,type:spike,needs:joe
---
## Task
Prove the engine is correct, uses every core, survives being killed, and gives byte-identical unit results on both of Joe's real machines. Cross-machine reproduction is what the whole distributed design rests on. Timebox: one evening.

## Acceptance
On the Apple Silicon Mac and on the Windows PC, from a release build:
- `palhunt search --to-bits 126` completes; `palhunt status` says verified below 2^126 with hits `1, 3, 5, 7, 9` only. Paste the last 10 lines of the search output (they include candidates per second) and the core count.
- Activity Monitor / Task Manager shows all cores busy during the 123- and 124-bit lengths.
- Kill it mid-run (Ctrl-C during bits 123), re-run the same command: it resumes without redoing logged units and reaches the same status.
- Copy the Mac's `palhunt-state/results.jsonl` to the PC (and the other way) and run `palhunt verify --sample 200 --state-dir <copied dir>`: every unit reproduces exactly.
- If anything fails, the exact output goes in this issue and the options are discussed in an orchestrator chat, not improvised.
- Results (speed per machine, cross-verification) are recorded in `docs/SPEC.md` section 7 and any decision it forces in `docs/ORCHESTRATOR_CONTEXT.md` section 2.

## Route
agent: joe   model: n/a   effort: n/a   mode: direct (local Claude Code on the Mac can run the Mac half)

## Depends on
P0-1
