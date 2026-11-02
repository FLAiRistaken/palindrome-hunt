---
title: P0-3 Make CI green on all four required checks
labels: phase:0,type:chore
---
## Task
`.github/workflows/ci.yml` was written during the bootstrap and has never run on a real runner. Make it pass for real.

## Acceptance
- On the first PR, "lint", "test (ubuntu-latest)", "test (macos-latest)" and "test (windows-latest)" are all green, and the job names are exactly those (branch protection depends on them).
- A docs-only PR shows all four as skipped, not pending.
- The pinned toolchain from `rust-toolchain.toml` (1.97.0) installs on all three runners; if it doesn't, fix the workflow, don't unpin.
- The smoke step's bash script runs on the Windows runner (`shell: bash`).
- Anything in the workflow that turned out wrong is fixed, not worked around, and listed in the PR. Note the runner-minute cost: the repo is public, so standard runners should be free; confirm on GitHub's current pricing page and say so in the PR.

## Route
agent: claude-cloud   model: Sonnet 5.5   effort: medium   mode: direct

## Depends on
P0-1
