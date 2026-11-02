---
title: P0-5 Check code-navigation tooling on Rust
labels: phase:0,type:spike
---
## Task
Find out whether token-savior indexes Rust usefully, so agent prompts only mention it if it works. Ten minutes.

## Acceptance
- With the repo checked out on the Mac and added to token-savior's workspace roots: `switch_project`, then `find_symbol` on `Layout` (in `src/layout.rs`), and `get_function_source` on `run_unit`.
- Result recorded in `docs/ORCHESTRATOR_CONTEXT.md` section 5: works / partial / not supported. If it doesn't work, prompts use plain Read/Grep.

## Route
agent: joe   model: n/a   effort: n/a   mode: direct (the orchestrator chat must be linked to the Mac)

## Depends on
P0-1
