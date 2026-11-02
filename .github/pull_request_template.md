## What and why
<!-- One or two sentences. What changed, and which acceptance checks it satisfies. -->

Closes #

**Phase label:** `phase:` <!-- also add it as a PR label; pr-hygiene checks both -->

## How it's proven
<!-- Paste the output of each check you ran, not a summary of it. -->

## Not verified here
<!-- Anything you could not run (Joe's Mac or PC, a GPU, the coordinator, a service you can't reach). Say so; the owner checks these. Write "none" only if it's true. -->

## Checklist
- [ ] One task, one branch, `Closes #N` above
- [ ] No change to secrets, signing, deployment or production settings
- [ ] No secrets or tokens in code, tests, fixtures or logs
- [ ] No test skipped, deleted or loosened to get green
- [ ] Any change to what a unit result means bumps `FORMAT_VERSION` or `ENGINE` (see docs/SPEC.md)
