#!/usr/bin/env bash
# Lane B: run the PR-branch Linux packager (ops/package-linux.sh @ 6df6c429)
# against this round's linux-x86_64 artifacts, with a synthetic non-release
# version. Read-only w.r.t. the repository; all output under
# /home/hoo/g4-172/laneB-packages/linux.
set -Eeuo pipefail
export PATH="$HOME/.local/bin:$PATH"

TOOLS="$HOME/g4-172/laneB-tools"
mkdir -p "$TOOLS"
SRC=/mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/g4/package-laneB/package-linux.sh.branch-sha
cp "$SRC" "$TOOLS/package-linux.sh"
chmod +x "$TOOLS/package-linux.sh"
echo "== tool identities =="
sha256sum "$TOOLS/package-linux.sh"
command -v xmake && xmake --version | head -1
gcc --version | head -1
ar --version | head -1

echo "== run packager =="
exec "$TOOLS/package-linux.sh" \
  --version 0.0.0-preflight \
  --source-commit 69ba1ce05a2772f5aa077c6ece3365c40f6e0500 \
  --work-root /home/hoo/g4-172/linux-root \
  --out-dir /home/hoo/g4-172/laneB-packages/linux \
  --build-receipt /home/hoo/g4-172/evidence/build-linux/20261030T125315Z/summary.json
