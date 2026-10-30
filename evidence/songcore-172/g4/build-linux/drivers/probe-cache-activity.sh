#!/usr/bin/env bash
# probe-cache-activity.sh — was any cache written/read outside the fresh root?
set -uo pipefail
export LC_ALL=C
R=/home/hoo/g4-172/linux-root/src/native
echo "== files under ~/.xmake modified today after 12:00Z =="
find "$HOME/.xmake" -newermt '2026-10-30 12:00:00' -printf '%TY-%Tm-%Td %TH:%TM %p\n' 2>/dev/null | head -20
echo "count=$(find "$HOME/.xmake" -newermt '2026-10-30 12:00:00' 2>/dev/null | wc -l)"
echo "== build/.build_cache =="
find "$R/build/.build_cache" -type f | wc -l
find "$R/build/.build_cache" -type f -printf '%TY-%Tm-%Td %TH:%TM %10s %p\n' | head -5
find "$R/build/.build_cache" -type f -printf '%TY-%Tm-%Td %TH:%TM\n' | sort | uniq -c | head
echo "== build/.objs =="
find "$R/build/.objs" -name '*.o' | wc -l
find "$R/build/.objs" -name '*.o' -printf '%TY-%Tm-%Td %TH:%TM\n' | sort | uniq -c | head
echo "== total .o under native (excluding oracle closure copy) =="
find "$R" -name '*.o' -not -path '*/ffmpeg-xmake/*' | wc -l
echo "== oldest/newest .o in .objs =="
find "$R/build/.objs" -name '*.o' -printf '%T@ %p\n' | sort -n | head -2
find "$R/build/.objs" -name '*.o' -printf '%T@ %p\n' | sort -n | tail -2
echo "== log lines mentioning cache/hit/miss =="
grep -iE 'cache|hit|miss' /home/hoo/g4-172/linux-root/receipts/g4-linux-build/xmake-build.log | grep -v 'cache compiling' | head -20
echo "== count of 'cache compiling' lines =="
grep -c 'cache compiling' /home/hoo/g4-172/linux-root/receipts/g4-linux-build/xmake-build.log
