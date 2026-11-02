---
title: P0-4 Protect main (owner, repo settings)
labels: phase:0,type:chore,needs:joe
---
## Task
Turn on branch protection for `main` and confirm merge settings. Orchestrator sessions can't change repo settings, so Joe does it.

## Acceptance
In GitHub: Settings, then Branches (or Rules, then Rulesets), add a rule for `main`:
- Require a pull request before merging; required approvals **0** (GitHub doesn't let an author approve their own PR).
- Require status checks to pass: "lint", "test (ubuntu-latest)", "test (macos-latest)" and "test (windows-latest)". A check only appears in the picker after it has run once, so do this after P0-3 is green.
- Require branches to be up to date before merging.
- Block force pushes; restrict deletions.
- `pr-hygiene` is visible but not required.
- Merge settings are squash-only with head branches deleted automatically (`scripts/bootstrap-github.sh` sets these; if it failed, Settings → General → Pull Requests: untick "Allow merge commits" and "Allow rebase merging", tick "Automatically delete head branches").

Then tell the orchestrator; it reads the settings back where it can and says what it couldn't verify.

## Route
agent: joe   model: n/a   effort: n/a   mode: direct

## Depends on
P0-3
