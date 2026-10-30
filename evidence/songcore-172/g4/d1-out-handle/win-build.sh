#!/usr/bin/env bash
# D1: does the *out_handle = NULL store exist in the COMPILED artifact on the
# second release target (windows-x86_64 / mingw)? Source is `git archive HEAD`
# from the pinned SHA — the product tree is never modified.
set -u
export PATH="$HOME/.local/bin:$PATH"
REPO=/home/hoo/Source/qianqian
SRC="$HOME/d1-win-src"
OUT="$HOME/d1-out-handle-build"
EV=/mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/g4/d1-out-handle
LOG="$OUT/win-build.log"
: > "$LOG"

rm -rf "$SRC"; mkdir -p "$SRC"
git -C "$REPO" archive HEAD | tar -x -C "$SRC"
echo "extracted_sha=$(git -C "$REPO" rev-parse HEAD)" | tee -a "$LOG"
sha256sum "$SRC/native/src/songcore_ffmpeg.c" | tee -a "$LOG"

mkdir -p "$SRC/native/build"
ln -sfn /home/hoo/q172d2-verify/closure/ffmpeg-xmake "$SRC/native/build/ffmpeg-xmake"
ln -sfn /home/hoo/q172d2-verify/closure/ffmpeg-src "$SRC/native/build/ffmpeg-src"
echo "manifest: $(sha256sum "$SRC/native/build/ffmpeg-xmake/manifest.json" | cut -d' ' -f1)" | tee -a "$LOG"
python3 - "$SRC/native/build/ffmpeg-xmake/manifest.json" <<'PY' | tee -a "$LOG"
import json,sys
m=json.load(open(sys.argv[1]))
print("manifest_target=", m.get("target"))
PY

cd "$SRC/native" || exit 1
xmake f -p mingw -a x86_64 -m release -y >> "$LOG" 2>&1
echo "configure_exit=$?" | tee -a "$LOG"
xmake build -y songcore_static >> "$LOG" 2>&1
rc=$?
echo "build_exit=$rc" | tee -a "$LOG"
ls -la "$SRC/native/build/artifacts" 2>/dev/null | tee -a "$LOG"
[ -f "$SRC/native/build/artifacts/libsongcore.a" ] && \
  sha256sum "$SRC/native/build/artifacts/libsongcore.a" | tee -a "$LOG"

echo "== disassembly of song_open: windows artifact ==" | tee -a "$LOG"
x86_64-w64-mingw32-objdump -d "$SRC/native/build/artifacts/libsongcore.a" 2>/dev/null \
  | awk '/<song_open>:/{f=1} f{print} f&&/^$/{exit}' | head -40 | tee -a "$LOG"
exit $rc
