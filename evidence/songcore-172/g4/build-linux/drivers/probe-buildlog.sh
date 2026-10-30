#!/usr/bin/env bash
# probe-buildlog.sh — confirm the fresh build compiled every unit.
set -uo pipefail
export LC_ALL=C
L=/home/hoo/g4-172/linux-root/receipts/g4-linux-build/xmake-build.log
echo "log_lines=$(wc -l < "$L")"
echo "gcc_invocations=$(grep -c '/usr/bin/gcc' "$L")"
echo "---- head ----"
head -12 "$L"
echo "---- tail ----"
tail -10 "$L"
echo "---- object count ----"
find /home/hoo/g4-172/linux-root/src/native/build/xmake -name '*.o' | wc -l
echo "---- objects under ffmpeg-src paths ----"
find /home/hoo/g4-172/linux-root/src/native/build/xmake -name '*.o' -path '*ffmpeg*' | wc -l
