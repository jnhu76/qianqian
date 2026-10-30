#!/usr/bin/env bash
# g4-recon.sh — read-only environment recon for the G4 linux-x86_64 build lane.
# Writes nothing outside stdout. Exit 0 always unless a hard probe fails.
set -uo pipefail
export PATH="$HOME/.local/bin:$PATH"
export LC_ALL=C

echo "== uname =="; uname -sr
echo "== nproc =="; nproc
echo "== gcc =="; gcc --version | head -1
echo "== xmake =="; command -v xmake; xmake --version | head -1
echo "== python3 =="; python3 --version
echo "== objdump =="; objdump --version | head -1
echo "== readelf =="; readelf --version | head -1
echo "== ar =="; ar --version | head -1
echo "== nm =="; nm --version | head -1
echo "== git =="; git --version
echo "== df =="; df -h "$HOME" | tail -2
echo "== wsl repo =="
git -C /home/hoo/Source/qianqian rev-parse HEAD || echo MISSING
git -C /home/hoo/Source/qianqian rev-parse 'HEAD^{tree}' || echo MISSING
git -C /home/hoo/Source/qianqian status --porcelain | head -5
git -C /home/hoo/Source/qianqian rev-parse --abbrev-ref HEAD
echo "== ffmpeg.lua at repo =="
grep -n "prefix-map" /home/hoo/Source/qianqian/native/build/ffmpeg.lua || echo NO_FIX_LINE
echo "== linux closure =="
CL=/home/hoo/qianqian-172-linux/native/build
ls -ld "$CL" "$CL/ffmpeg-xmake" "$CL/ffmpeg-src" "$CL/downloads" 2>&1
echo "-- manifest --"
cat "$CL/ffmpeg-xmake/manifest.json" 2>&1
echo "-- manifest sha256 --"
sha256sum "$CL/ffmpeg-xmake/manifest.json" 2>&1
echo "-- oracle config --"
ls -l "$CL/ffmpeg-xmake/oracle/ffbuild/config.mak" 2>&1
echo "-- verified stamp --"
cat "$CL/ffmpeg-src/.qianqian-verified" 2>&1
echo "-- src root contents --"
ls "$CL/ffmpeg-src" | head -20
echo "-- sizes --"
du -sh "$CL/ffmpeg-src" "$CL/ffmpeg-xmake" 2>&1
echo "== pinned zip =="
ls -l /home/hoo/q172d2-verify/win/native/build/downloads/ffmpeg-n9.0.1.zip 2>&1
sha256sum /home/hoo/q172d2-verify/win/native/build/downloads/ffmpeg-n9.0.1.zip 2>&1
echo "== target roots present? =="
ls -d /home/hoo/g4-172 2>&1 || echo "g4-172 absent"
echo "== qq329 leftovers (xmake cache reference) =="
ls -d /home/hoo/qq329/* 2>/dev/null | head -10 || echo none
echo "== done =="
