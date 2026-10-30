#!/usr/bin/env bash
# g4-s6-publish.sh — G4 linux-x86_64 lane: final snapshot, stage.json /
# summary.json generation, and mirror of the lane receipts (summary + logs
# only, never artifacts) into the Windows evidence directory.
#
# Order matters: s7 writes the last measured receipt into EV, s5 then hashes
# every EV file, and nothing in EV changes afterwards.
set -Eeuo pipefail
export PATH="$HOME/.local/bin:$PATH"
export LC_ALL=C
ROOT=${G4_ROOT:-$HOME/g4-172/linux-root}
EV=$ROOT/receipts/g4-linux-build
DRV=$(cd "$(dirname "$0")" && pwd)
OLD=/mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/q2-crossroot/build-receipts/linux-lin-l1-route.summary.json
DEST=/mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/g4/build-linux

rm -f "$EV/summary-generation.stdout.txt"

bash "$DRV/g4-s7-final-checks.sh"

python3 "$DRV/g4-s5-summary.py" "$EV" "$OLD" "$DRV" > /tmp/g4-summary.stdout

mkdir -p "$DEST/logs"
find "$DEST/logs" -mindepth 1 -delete
cp -a "$EV/." "$DEST/logs/"
rm -f "$DEST/logs/stage.json" "$DEST/logs/summary.json"
cp -f "$EV/stage.json" "$DEST/stage.json"
cp -f "$EV/summary.json" "$DEST/summary.json"
cp -f /tmp/g4-summary.stdout "$DEST/logs/summary-generation.stdout.txt"

echo "---- summary generator output ----"
cat /tmp/g4-summary.stdout
echo "---- mirrored lane tree ----"
find "$DEST" -maxdepth 2 -type f -printf '%10s  %p\n' | sort -k2
echo "---- identity of the two meta files ----"
sha256sum "$EV/stage.json" "$DEST/stage.json" "$EV/summary.json" "$DEST/summary.json"
echo "---- no build artifact under /mnt/c ----"
find "$DEST" \( -name '*.a' -o -name '*.so' -o -name 'libsongcore*' \) -print
echo "(nothing above = PASS)"
