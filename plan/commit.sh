#!/usr/bin/env sh
# Commit plan/ to the orphan branch `plan` without touching the working tree or main's index,
# then push it. Usage: plan/commit.sh "message"
set -eu
cd "$(git rev-parse --show-toplevel)"
msg=${1:-"plan: update"}
export GIT_INDEX_FILE=$(mktemp -u)
trap 'rm -f "$GIT_INDEX_FILE"' EXIT
git add -f plan/
tree=$(git write-tree)
parent=$(git rev-parse -q --verify refs/heads/plan || true)
if [ "$parent" ] && [ "$tree" = "$(git rev-parse "$parent^{tree}")" ]; then
  echo "plan: no changes"
else
  commit=$(git commit-tree "$tree" ${parent:+-p "$parent"} -m "$msg")
  git update-ref refs/heads/plan "$commit" ${parent:-}
  echo "plan: $(git rev-parse --short "$commit") $msg"
fi
git push -q origin plan
