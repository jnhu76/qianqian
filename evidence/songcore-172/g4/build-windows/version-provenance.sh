#!/usr/bin/env bash
# Locate the ffversion.h that fed each build and show the embedded version.
set -uo pipefail
echo "=== #329 root closure oracle headers"
ls /home/hoo/qq329/win-l1-route/src/native/build/ffmpeg-xmake/oracle/ 2>&1 | head
for f in /home/hoo/qq329/win-l1-route/src/native/build/ffmpeg-xmake/oracle/ffversion.h \
         /home/hoo/qq329/win-l1-route/src/native/build/ffmpeg-src/ffbuild/version.h ; do
  echo "-- $f"; cat "$f" 2>&1 | head -5
done
echo
echo "=== our root closure oracle headers"
for f in /home/hoo/q172rc/win/native/build/ffmpeg-xmake/oracle/ffversion.h \
         /home/hoo/q172rc/win/native/build/ffmpeg-src/ffbuild/version.h ; do
  echo "-- $f"; cat "$f" 2>&1 | head -5
done
echo
echo "=== locate any ffversion.h under both roots"
find /home/hoo/qq329/win-l1-route/src/native/build -name 'ffversion.h' 2>/dev/null | head -5
find /home/hoo/q172rc/win/native/build -name 'ffversion.h' 2>/dev/null | head -5
echo
echo "=== version.sh provenance decision (does .git exist in either ffmpeg-src?)"
for d in /home/hoo/qq329/win-l1-route/src/native/build/ffmpeg-src /home/hoo/q172rc/win/native/build/ffmpeg-src; do
  echo "-- $d"; ls -d "$d/.git" 2>&1; git -C "$d" describe --tags 2>&1 | head -1
done
