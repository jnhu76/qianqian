#!/usr/bin/env bash
# Lane E: hash inputs, archived evidence, and compare ABI headers across copies.
set -u
export LC_ALL=C
REPO=/mnt/c/Users/fred1/source/qianqian
EVD="$REPO/evidence/songcore-172/g4/lane-e-runtime-deps"
RAW="$EVD/raw"
REPO_NATIVE=/home/hoo/Source/qianqian
BUILD_ROOT=/home/hoo/g4-172/linux-root/src

{
  echo "== consumer sources used and build header =="
  sha256sum "$REPO/evidence/songcore-172/audit/abi-complete-consumer.c" \
            "$REPO/evidence/songcore-172/linux/verify/package/external-consumer-example.c" \
            "$BUILD_ROOT/native/include/songcore.h"
  echo
  echo "== same header in other locations (ABI identity) =="
  for h in "$REPO_NATIVE/native/include/songcore.h" \
           "$REPO/evidence/songcore-172/package/stage/songcore-v0.2.0-rc.1-windows-x86_64/include/songcore.h" \
           "$REPO/evidence/songcore-172/g4/p0-entries/package-rc-out/default/stage/songcore-v0.2.0-rc.1-windows-x86_64/include/songcore.h"; do
    if [ -f "$h" ]; then sha256sum "$h"; else echo "MISSING $h"; fi
  done
  echo
  echo "== packaged consumer template copies =="
  for c in "$REPO/evidence/songcore-172/package/stage/songcore-v0.2.0-rc.1-windows-x86_64/metadata/external-consumer-example.c" \
           "$REPO/evidence/songcore-172/g4/p0-entries/package-rc-out/default/stage/songcore-v0.2.0-rc.1-windows-x86_64/metadata/external-consumer-example.c"; do
    if [ -f "$c" ]; then sha256sum "$c"; else echo "MISSING $c"; fi
  done
  echo
  echo "== archived prior evidence cited (for conflict/agreement record) =="
  sha256sum "$REPO/evidence/songcore-172/link-contract.md" \
            "$REPO/evidence/songcore-172/audit/lin-external-census.txt" \
            "$REPO/evidence/songcore-172/audit/lin-link-libs.log" \
            "$REPO/evidence/songcore-172/audit/lin-link-nolibs.log" \
            "$REPO/evidence/songcore-172/audit/lin-link-nomath.log"
} > "$RAW/evidence-binding-sha256.txt" 2>&1
cat "$RAW/evidence-binding-sha256.txt"
