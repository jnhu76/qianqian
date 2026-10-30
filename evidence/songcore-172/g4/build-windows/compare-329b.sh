#!/usr/bin/env bash
# Occurrence-index member comparison + differing-member forensics.
set -uo pipefail
Q329=/home/hoo/qq329/win-l1-route/src/native/build/artifacts/libsongcore.a
OUR=/home/hoo/q172rc/win/native/build/artifacts/libsongcore.a
LANE=/mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/g4/build-windows
echo "=== occurrence-index comparison"
python3 "$LANE/ar_member_cmp.py" "$Q329" "$OUR"

echo
echo "=== version.c.obj forensics"
x86_64-w64-mingw32-ar p "$Q329" version.c.obj > /home/hoo/g4-cmp/ver-329.o 2>/dev/null || true
x86_64-w64-mingw32-ar p "$OUR" version.c.obj > /home/hoo/g4-cmp/ver-our.o 2>/dev/null || true
ls -l /home/hoo/g4-cmp/ver-329.o /home/hoo/g4-cmp/ver-our.o
echo "-- strings only in #329 member:"
comm -23 <(strings -a /home/hoo/g4-cmp/ver-329.o | sort -u) <(strings -a /home/hoo/g4-cmp/ver-our.o | sort -u) | head -20
echo "-- strings only in our member:"
comm -13 <(strings -a /home/hoo/g4-cmp/ver-329.o | sort -u) <(strings -a /home/hoo/g4-cmp/ver-our.o | sort -u) | head -20

echo
echo "=== version unit source identity (closure vs reused closure)"
sha256sum /home/hoo/qq329/win-l1-route/src/native/build/ffmpeg-src/libavutil/version.c \
          /home/hoo/q172rc/win/native/build/ffmpeg-src/libavutil/version.c 2>&1
echo "-- version_major/minor/micro"
grep -E "LIBAVUTIL_VERSION_(MAJOR|MINOR|MICRO)" /home/hoo/qq329/win-l1-route/src/native/build/ffmpeg-src/libavutil/version.h 2>/dev/null | head -3
grep -E "LIBAVUTIL_VERSION_(MAJOR|MINOR|MICRO)" /home/hoo/q172rc/win/native/build/ffmpeg-src/libavutil/version.h 2>/dev/null | head -3
