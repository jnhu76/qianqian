#!/usr/bin/env bash
# D1 out_handle differential: build libsongcore.a at source SHA 69ba1ce.
# Read-only w.r.t. the product tree: only untracked build outputs are created.
set -u
export PATH="$HOME/.local/bin:$PATH"
REPO=/home/hoo/Source/qianqian
OUT="$HOME/d1-out-handle-build"
mkdir -p "$OUT"
LOG="$OUT/build.log"
: > "$LOG"

echo "== head ==" | tee -a "$LOG"
git -C "$REPO" rev-parse HEAD | tee -a "$LOG"
git -C "$REPO" status --porcelain --untracked-files=no | tee -a "$LOG"

echo "== closure link ==" | tee -a "$LOG"
mkdir -p "$REPO/native/build"
DONOR=/home/hoo/qianqian-172-linux/native/build
for d in ffmpeg-xmake ffmpeg-src; do
  if [ ! -e "$REPO/native/build/$d" ]; then
    ln -sfn "$DONOR/$d" "$REPO/native/build/$d"
    echo "symlinked $d -> $(readlink -f "$REPO/native/build/$d")" | tee -a "$LOG"
  fi
done
echo "manifest sha256: $(sha256sum "$REPO/native/build/ffmpeg-xmake/manifest.json" | cut -d' ' -f1)" | tee -a "$LOG"
rm -f "$REPO/native/build/artifacts/libqianqian_av.a"

cd "$REPO/native" || exit 1
echo "== xmake f (cwd=$(pwd)) ==" | tee -a "$LOG"
xmake f -p linux -a x86_64 -m release -y >> "$LOG" 2>&1
echo "configure_exit=$?" | tee -a "$LOG"

echo "== xmake build songcore_static ==" | tee -a "$LOG"
xmake build -y songcore_static >> "$LOG" 2>&1
rc=$?
echo "build_exit=$rc" | tee -a "$LOG"

ls -la "$REPO/native/build/artifacts" 2>/dev/null | tee -a "$LOG"
if [ -f "$REPO/native/build/artifacts/libsongcore.a" ]; then
  sha256sum "$REPO/native/build/artifacts/libsongcore.a" | tee -a "$LOG"
  echo "== songcore_ffmpeg.c sha256 ==" | tee -a "$LOG"
  sha256sum "$REPO/native/src/songcore_ffmpeg.c" | tee -a "$LOG"
fi
exit $rc
