#!/bin/bash
# songcore-172 D2: Linux route setup + FFmpeg closure import (long pole).
# Runs fetch-ffmpeg (zip reused from #324 cache, sha-verified) and the
# linux-x86_64 import oracle. The xmake build itself is run later, after the
# D2 fix lands.
set -euo pipefail
export PATH=/home/hoo/.local/bin:$PATH
L=/home/hoo/q172d2/lin
WT=/mnt/c/Users/fred1/source/qianqian-172

rm -rf "$L"
mkdir -p "$L/native/build/downloads"
cd "$WT"
tar cf - \
  --exclude='native/build/artifacts' \
  --exclude='native/build/downloads' \
  --exclude='native/build/ffmpeg-src' \
  --exclude='native/build/ffmpeg-xmake' \
  --exclude='native/build/xmake' \
  native | tar xf - -C "$L"

cp /home/hoo/q324/native/build/downloads/ffmpeg-n9.0.1.zip "$L/native/build/downloads/"
cd "$L/native"
echo "=== fetch-ffmpeg ==="
bash scripts/fetch-ffmpeg
echo "=== ffmpeg_import --recipe linux-x86_64 ==="
python3 tools/ffmpeg_import.py --recipe linux-x86_64
echo "LINUX_IMPORT_DONE"
