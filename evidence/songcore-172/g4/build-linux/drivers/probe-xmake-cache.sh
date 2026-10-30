#!/usr/bin/env bash
# probe-xmake-cache.sh — identify what "cache compiling.release" means here.
set -uo pipefail
export PATH="$HOME/.local/bin:$PATH"
export LC_ALL=C
echo "== ccache present? =="
command -v ccache || echo "ccache: not installed"
echo "== xmake global config =="
ls -la "$HOME/.xmake" 2>&1 | head -20
for f in "$HOME/.xmake/.xmake.conf" "$HOME/.xmake/xmake.conf"; do
  [ -f "$f" ] && { echo "--- $f"; cat "$f"; }
done
echo "== xmake global status =="
xmake g --show 2>&1 | head -30 || true
echo "== project build dirs =="
ls -la /home/hoo/g4-172/linux-root/src/native/build | head -20
echo "== xmake cache dirs =="
ls -la "$HOME/.xmake/cache" 2>&1 | head
du -sh "$HOME/.xmake/cache" 2>/dev/null || true
echo "== object files anywhere in the root =="
find /home/hoo/g4-172/linux-root/src/native -name '*.o' -newermt '2026-10-30 12:30' 2>/dev/null | wc -l
find /home/hoo/g4-172/linux-root/src/native -name '*.o' 2>/dev/null | head -5
echo "== xmake build cache root =="
ls -la /home/hoo/g4-172/linux-root/src/native/build/.objs 2>&1 | head
ls -la /home/hoo/g4-172/linux-root/src/native/build/xmake 2>&1 | head
