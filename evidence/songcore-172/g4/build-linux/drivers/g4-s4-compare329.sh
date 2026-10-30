#!/usr/bin/env bash
# g4-s4-compare329.sh — G4 linux-x86_64 lane, stage 4: compare this build's
# artifacts with the #329 archived products (both readings measured here).
set -Eeuo pipefail
export PATH="$HOME/.local/bin:$PATH"
export LC_ALL=C
ROOT=${G4_ROOT:-$HOME/g4-172/linux-root}
EV=$ROOT/receipts/g4-linux-build
C=$EV/compare-329
mkdir -p "$C"
DRV=$(cd "$(dirname "$0")" && pwd)
NEW=$ROOT/src/native/build/artifacts/libsongcore.a
OLD=/home/hoo/qq329/lin-r1/src/native/build/artifacts/libsongcore.a
NEW_SO=$ROOT/src/native/build/artifacts/shared/libsongcore.so
OLD_SO=/home/hoo/qq329/lin-r1/src/native/build/artifacts/shared/libsongcore.so

sha256sum "$NEW" "$OLD" "$NEW_SO" "$OLD_SO" > "$C/sha256-four.txt"
cat "$C/sha256-four.txt"
python3 "$DRV/g4-ar-cmp.py" "$NEW" "$OLD" "$C/ar-cmp-new-vs-329" | tee "$C/ar-cmp-summary.txt"
echo "---- shared byte identity ----"
if cmp -s "$NEW_SO" "$OLD_SO"; then
  echo "shared_cmp=identical"
else
  echo "shared_cmp=DIFFER"
  cmp -l "$NEW_SO" "$OLD_SO" | wc -l
fi
