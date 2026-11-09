#!/usr/bin/env sh
# Set up the agent working surface on a clone of main. Safe to re-run.
#   Fresh clone:  git fetch origin plan:plan && git show plan:plan/setup.sh | sh
# Restores plan/ from the `plan` branch if it is missing, then makes the root links (all
# gitignored on main) so agents and skills find their files at the usual paths.
set -eu
cd "$(git rev-parse --show-toplevel)"
git rev-parse -q --verify refs/heads/plan >/dev/null || git branch --track plan origin/plan
[ -d plan ] || git restore --source=plan --worktree -- plan
ln -sfn plan/CLAUDE.md CLAUDE.md
ln -sfn plan/CLAUDE.md AGENTS.md
ln -sfn plan/design/DESIGN.md DESIGN.md
ln -sfn plan/design/PRODUCT.md PRODUCT.md
ln -sfn plan/design/.impeccable .impeccable
ln -sfn ../plan/superpowers docs/superpowers
echo "plan: ready at $(git rev-parse --short plan)"
