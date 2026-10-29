#!/bin/bash
# songcore-172 D2: Windows (mingw cross) RED build — current (unfixed) recipe.
# Route per #324: tar the native tree into WSL local FS, reuse the #324
# windows-x86_64 FFmpeg closure (ffmpeg-src + manifest + oracle), then
#   xmake f -p mingw -a x86_64 -m release -y
#   xmake build songcore          -> expected RED: shared DLL link dies on
#                                    BCrypt* although recipe artifacts.shared=false
# Everything lands in /home/hoo/q172d2/win. Exit code is echoed, never swallowed.
set -uo pipefail
export PATH=/home/hoo/.local/bin:$PATH
W=/home/hoo/q172d2/win
WT=/mnt/c/Users/fred1/source/qianqian-172
LOG=/home/hoo/q172d2/red-build.log

rm -rf "$W"
mkdir -p "$W/native/build"
cd "$WT"
tar cf - \
  --exclude='native/build/artifacts' \
  --exclude='native/build/downloads' \
  --exclude='native/build/ffmpeg-src' \
  --exclude='native/build/ffmpeg-xmake' \
  --exclude='native/build/xmake' \
  native | tar xf - -C "$W"

# Reuse the #324 windows-x86_64 closure (source + oracle config headers + manifest).
cp -a /home/hoo/q324/native/build/ffmpeg-src  "$W/native/build/ffmpeg-src"
cp -a /home/hoo/q324/native/build/ffmpeg-xmake "$W/native/build/ffmpeg-xmake"
cp -a /home/hoo/q324/native/build/downloads   "$W/native/build/downloads"

# Provenance record for the reused closure.
{
  echo "== reused closure provenance =="
  sha256sum "$W/native/build/ffmpeg-xmake/manifest.json"
  sha256sum "$W/native/ffmpeg/targets/windows-x86_64.json"
  sha256sum /home/hoo/q324/native/ffmpeg/targets/windows-x86_64.json 2>/dev/null || true
} > /home/hoo/q172d2/red-closure-provenance.txt
cat /home/hoo/q172d2/red-closure-provenance.txt

cd "$W/native"
echo "=== xmake f -p mingw -a x86_64 -m release -y ==="
xmake f -p mingw -a x86_64 -m release -y
echo "CONFIG_EXIT=$?"
echo "=== xmake build songcore (RED) ==="
set +e
xmake -y build songcore > "$LOG" 2>&1
RC=$?
set -e
echo "RED_BUILD_EXIT=$RC"
echo "=== artifacts after RED build ==="
ls -la build/artifacts/ 2>/dev/null || echo "no artifacts dir"
ls -la build/artifacts/shared/ 2>/dev/null || echo "no shared dir"
echo "=== RED log tail ==="
tail -40 "$LOG"
echo "RED_DONE"
