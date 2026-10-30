#!/usr/bin/env bash
# probe-version-string.sh — locate the closure's derivation-time FFmpeg version
# string and show which generated inputs carry it (read-only).
set -uo pipefail
export LC_ALL=C
ORACLE=/home/hoo/qianqian-172-linux/native/build/ffmpeg-xmake/oracle
SRC=/home/hoo/qianqian-172-linux/native/build/ffmpeg-src
CPORACLE=/home/hoo/g4-172/linux-root/src/native/build/ffmpeg-xmake/oracle
echo "== ffversion.h =="
find "$ORACLE" -name 'ffversion.h' -o -name 'version.h' | head -20
for f in $(find "$ORACLE" -name 'ffversion.h'); do
  echo "--- $f"; cat "$f"
done
echo "== closure oracle grep =="
grep -rl "qianqian-v0.1.0" "$ORACLE" 2>/dev/null | head -20
echo "== copied oracle grep (this root) =="
grep -rl "qianqian-v0.1.0" "$CPORACLE" 2>/dev/null | head -20
echo "== ffmpeg-src git describe context =="
ls -d "$SRC/.git" 2>&1
git -C /home/hoo/qianqian-172-linux rev-parse HEAD 2>&1
git -C /home/hoo/qianqian-172-linux describe --tags 2>&1
echo "== current root describe (for contrast) =="
git -C /home/hoo/g4-172/linux-root/src describe --tags 2>&1
echo "== version.c members in our archive =="
ar t /home/hoo/g4-172/linux-root/src/native/build/artifacts/libsongcore.a | grep -c 'version.c.o'
ar t /home/hoo/g4-172/linux-root/src/native/build/artifacts/libsongcore.a | grep 'version.c.o'
