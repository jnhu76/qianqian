#!/usr/bin/env bash
# Prepare an independent replay root for ops/build-linux.sh:
# a byte-copy of the campaign clone under a scratch HOME so the entry's
# hard-coded DST ($HOME/qianqian-172-linux) resolves there and the real
# campaign worktree is never mutated.
set -eu
SRC=/home/hoo/qianqian-172-linux
DST=/home/hoo/q172-g4-audit/qianqian-172-linux
if [ -d "$DST" ]; then
  echo "already prepared: $DST"
else
  mkdir -p /home/hoo/q172-g4-audit
  cp -a "$SRC" /home/hoo/q172-g4-audit/qianqian-172-linux
fi
df -h /home/hoo | tail -1
git -C "$DST" rev-parse HEAD
git -C "$DST" rev-parse --abbrev-ref HEAD
du -sh "$DST" | tail -1
test -f "$DST/native/build/ffmpeg-xmake/manifest.json" && echo closure_manifest=PRESENT
test -f "$DST/native/build/ffmpeg-src/.qianqian-verified" && echo verified_marker=PRESENT
