#!/usr/bin/env bash
# Lane C (linux) - stage 06: provenance of the include path actually used,
# comparison run with recorded exit code, and copy of the run outputs into the
# Windows-visible evidence lane directory.
set -u
PKG=/home/hoo/g4-172/laneB-stage/linux/songcore-v0.0.0-preflight-linux-x86_64
W=/home/hoo/g4-172/consume-linux
EVID=/mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/g4/consume-linux
LOG=$EVID/logs/stage06-finalize.log
mkdir -p "$EVID/out/static" "$EVID/out/shared"
: > "$LOG"

run() {
    local log=$1; shift
    printf '\n$ %s\n' "$*" >> "$log"
    "$@" >> "$log" 2>&1
    local rc=$?
    printf '[exit] %d\n' "$rc" >> "$log"
    echo "EXIT=$rc : $*"
    return $rc
}

echo "=== include-path provenance: which songcore.h does the compile actually open? ==="
{
  echo "\$ gcc -H -fsyntax-only -I$PKG/include $W/src/consume_corpus.c"
  gcc -H -fsyntax-only -I"$PKG/include" "$W/src/consume_corpus.c" 2>&1 | grep -i 'songcore.h'
  echo "[exit] $?"
  echo "--- expected: $PKG/include/songcore.h ---"
  echo "--- repo header (NOT used; recorded for contrast) ---"
  sha256sum /mnt/c/Users/fred1/source/qianqian/native/include/songcore.h 2>/dev/null || true
  echo "--- package header ---"
  sha256sum "$PKG/include/songcore.h"
  echo "--- are they byte-identical? ---"
  cmp -s /mnt/c/Users/fred1/source/qianqian/native/include/songcore.h "$PKG/include/songcore.h" \
      && echo "IDENTICAL_TO_REPO_HEADER" || echo "DIFFERS_FROM_REPO_HEADER"
} >> "$LOG" 2>&1
sed -n '/include-path provenance/,/DIFFERS_FROM_REPO_HEADER\|IDENTICAL_TO_REPO_HEADER/p' "$LOG"

echo
echo "=== archive/shlib bytes are unchanged before and after the consumer builds ==="
{
  sha256sum "$PKG/lib/libsongcore.a" "$PKG/lib/shared/libsongcore.so"
} >> "$LOG"

echo
echo "=== comparison against the frozen reference (recorded exit) ==="
run "$LOG" python3 "$EVID/scripts/05-compare.py" "$W" "$EVID/out/comparison.json"

echo
echo "=== copy run outputs into the evidence lane (Windows-visible) ==="
cp "$W"/out/static/*.jsonl "$W"/out/static/*.out "$EVID/out/static/" 2>/dev/null
cp "$W"/out/shared/*.jsonl "$W"/out/shared/*.out "$W"/out/shared/*.err "$EVID/out/shared/" 2>/dev/null
cp "$W"/out/shared/lddebug-mp3.err "$EVID/out/shared/" 2>/dev/null
{
  echo "--- copied outputs ---"
  ls -l "$EVID/out/static" "$EVID/out/shared"
  echo "--- sha256 of copied outputs ---"
  sha256sum "$EVID"/out/static/* "$EVID"/out/shared/* "$EVID/out/comparison.json"
} >> "$LOG" 2>&1
echo "copied"

echo
echo "=== isolated workdir inventory (proof the consumer never used the repo tree) ==="
{
  echo "--- workdir tree ---"
  find "$W" -maxdepth 2 -type f | sort
  echo "--- any repo build-tree reference in the consumer binaries? ---"
  for b in "$W"/bin/*; do strings -a "$b" | egrep '/home/hoo/Source|native/build|build/artifacts' && echo "DEV_TREE_REF_IN $b"; done
  echo "(scan done)"
  echo "--- source sha256 ---"
  sha256sum "$W"/src/*.c
  echo "--- reference.json in the workdir ---"
  sha256sum "$W/reference.json"
} >> "$LOG" 2>&1
sed -n '/isolated workdir inventory/,$p' "$LOG"
