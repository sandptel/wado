#!/usr/bin/env sh
# Commit plan/ to the orphan branch `plan` without touching the working tree or main's index.
# Usage: plan/commit.sh "message"
set -eu
cd "$(git rev-parse --show-toplevel)"
msg=${1:-"plan: update"}
export GIT_INDEX_FILE=$(mktemp -u)
trap 'rm -f "$GIT_INDEX_FILE"' EXIT
git add -f plan/
tree=$(git write-tree)
parent=$(git rev-parse -q --verify refs/heads/plan || true)
[ "$parent" ] && [ "$tree" = "$(git rev-parse "$parent^{tree}")" ] && { echo "plan: no changes"; exit 0; }
commit=$(git commit-tree "$tree" ${parent:+-p "$parent"} -m "$msg")
git update-ref refs/heads/plan "$commit" ${parent:-}
echo "plan: $(git rev-parse --short "$commit") $msg"
