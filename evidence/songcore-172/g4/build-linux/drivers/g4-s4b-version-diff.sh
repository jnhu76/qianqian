#!/usr/bin/env bash
# g4-s4b-version-diff.sh — confirm the 4 differing members differ only inside
# the FFMPEG_VERSION literal, and show the source-level causal link.
set -Eeuo pipefail
export LC_ALL=C
ROOT=/home/hoo/g4-172/linux-root
EV=$ROOT/receipts/g4-linux-build
C=$EV/compare-329
DRV=$(cd "$(dirname "$0")" && pwd)
python3 "$DRV/probe-version-bytes.py" \
  "$ROOT/src/native/build/artifacts/libsongcore.a" \
  /home/hoo/qq329/lin-r1/src/native/build/artifacts/libsongcore.a \
  "$C/version-bytes.txt" | tee "$C/version-bytes.stdout.txt"
echo "---- source-level link ----"
grep -n "ffversion.h\|FFMPEG_VERSION" /home/hoo/qianqian-172-linux/native/build/ffmpeg-src/libavutil/version.c
echo "---- generated header in the reused closure ----"
cat /home/hoo/g4-172/linux-root/src/native/build/ffmpeg-xmake/oracle/libavutil/ffversion.h
echo "---- describe of the clones ----"
git -C /home/hoo/qianqian-172-linux describe --tags
git -C /home/hoo/g4-172/linux-root/src describe --tags
