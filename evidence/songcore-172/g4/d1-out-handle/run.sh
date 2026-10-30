#!/usr/bin/env bash
# D1 out_handle differential: compile and run the sentinel probe against the
# libsongcore.a built from source SHA 69ba1ce.
set -u
export PATH="$HOME/.local/bin:$PATH"
REPO=/home/hoo/Source/qianqian
EV=/mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/g4/d1-out-handle
BIN="$HOME/d1-out-handle-build"
FIXTURE=/mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/binding-parity/20261028-215245/media/truncated-100000.flac
LOG="$EV/run.log"
: > "$LOG"

{
  echo "== source identity =="
  git -C "$REPO" rev-parse HEAD
  sha256sum "$REPO/native/src/songcore_ffmpeg.c"
  sha256sum "$REPO/native/include/songcore.h"
  sha256sum "$REPO/native/build/artifacts/libsongcore.a"
  echo "== library symbol =="
  nm -g --defined-only "$REPO/native/build/artifacts/libsongcore.a" 2>/dev/null | grep -w song_open
  echo "== fixture =="
  sha256sum "$FIXTURE"
  echo "== path (b) window: songcore_ffmpeg.c:820-832 =="
  sed -n '820,832p' "$REPO/native/src/songcore_ffmpeg.c"
  echo "== build probe =="
} >> "$LOG" 2>&1

gcc -O1 -g -Wall -Wextra -std=c11 \
    -I "$REPO/native/include" \
    "$EV/d1_probe.c" \
    "$REPO/native/build/artifacts/libsongcore.a" \
    -Wl,--wrap=calloc \
    -lm -lpthread -o "$BIN/d1_probe" >> "$LOG" 2>&1
echo "compile_exit=$?" | tee -a "$LOG"

{
  echo "== run =="
  echo "argv: $BIN/d1_probe --fixture $FIXTURE"
} >> "$LOG"

"$BIN/d1_probe" --fixture "$FIXTURE" >> "$LOG" 2>&1
rc=$?
echo "run_exit=$rc" >> "$LOG"
cat "$LOG"
exit 0
