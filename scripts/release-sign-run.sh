#!/usr/bin/env bash
# Sign one Pages build of the client: download that run's artifact (the exact bytes Pages would
# serve), check every hash, sign release.json with your offline key, and print the command that
# deploys the signature. Nothing is dispatched by this script. See docs/release-signing.md.
#
#   scripts/release-sign-run.sh <run-id> <private-key.pem>
#
# The run id is the "pages" workflow run to sign: `gh run list --workflow pages.yml -L 5`.
# Sign the newest one — deploying an older run's signature rolls Pages back to it.
set -euo pipefail
RUN=${1:?usage: release-sign-run.sh <run-id> <private-key.pem>}
KEY=${2:?usage: release-sign-run.sh <run-id> <private-key.pem>}
ROOT=$(cd "$(dirname "$0")/.." && pwd)
DIR=$(mktemp -d)
trap 'rm -rf "$DIR"' EXIT

gh run view "$RUN" --json headSha,headBranch,event,conclusion,createdAt \
  --template 'run {{.headBranch}} @ {{.headSha}} ({{.event}}, {{.conclusion}}, {{.createdAt}})
'
gh run download "$RUN" -n github-pages -D "$DIR/artifact"
mkdir "$DIR/site"
tar -xf "$DIR/artifact/artifact.tar" -C "$DIR/site"

# Prints the version, the commit CI built and the file count — check the commit is yours.
SIG=$(node "$ROOT/scripts/release-sign.mjs" sign "$DIR/site" "$KEY" | tee /dev/stderr | sed -n 's/^release.sig: //p')
[ -n "$SIG" ] || { echo "signing failed" >&2; exit 1; }
echo
echo "Deploy it (the workflow verifies it against sw-keys.js before deploying):"
echo "  gh workflow run pages.yml -f run_id=$RUN -f signature=$SIG"
