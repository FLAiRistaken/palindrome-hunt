#!/usr/bin/env bash
# Creates the repo's labels and seeds its first issues from scripts/seed-issues/*.md.
# Run once from the repo root, after `gh auth login` and after the repo exists on GitHub.
#   scripts/bootstrap-github.sh --dry-run    preview, creates nothing
#   scripts/bootstrap-github.sh              create labels, then any seed issue whose title doesn't exist yet
# Safe to re-run: labels are updated in place and issues are matched by exact title.
# Env: PHASES (default "0 1 2 3 4 5") sets which phase:<n> labels exist.
#
# Seed issue file format (name the files in the order you want them created):
#   ---
#   title: P0-1 Skeleton: ...
#   labels: phase:0,type:chore
#   ---
#   <markdown body>
set -euo pipefail

dry=false
if [ "${1:-}" = "--dry-run" ]; then dry=true; fi
PHASES="${PHASES:-0 1 2 3 4 5}"
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

run() {
  if [ "$dry" = true ]; then
    printf 'DRY-RUN: gh'
    printf ' %q' "$@"
    printf '\n'
  else
    gh "$@"
  fi
}

label() { # name color description
  run label create "$1" --color "$2" --description "$3" --force
}

for p in $PHASES; do
  label "phase:$p" "0052CC" "Roadmap phase $p"
done
label "type:feature" "0E8A16" "New behaviour"
label "type:bug" "D73A4A" "Differs from the spec"
label "type:chore" "C5DEF5" "Tooling, process, cleanup"
label "type:spike" "FBCA04" "Time-boxed investigation"
label "needs:joe" "B60205" "Only the owner can do the next step"

# Squash-only merges and branch clean-up. The docs say squash merge; this is what enforces it.
run api -X PATCH 'repos/{owner}/{repo}' -F allow_squash_merge=true -F allow_merge_commit=false \
  -F allow_rebase_merge=false -F delete_branch_on_merge=true

if [ "$dry" = true ]; then
  existing=""
else
  existing="$(gh issue list --state all --limit 500 --json title --jq '.[].title')"
fi

shopt -s nullglob
for f in "$here"/seed-issues/*.md; do
  title="$(sed -n 's/^title: //p' "$f" | head -1)"
  labels="$(sed -n 's/^labels: //p' "$f" | head -1)"
  body="$(awk '/^---$/ { n++; next } n >= 2 { print }' "$f")"
  if [ -z "$title" ]; then
    echo "skip $f: no title" >&2
    continue
  fi
  if printf '%s\n' "$existing" | grep -Fxq -- "$title"; then
    echo "exists: $title"
    continue
  fi
  run issue create --title "$title" --label "$labels" --body "$body"
done
