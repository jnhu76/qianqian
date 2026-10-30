#!/usr/bin/env bash
# Record xmake build duration for every build-linux.sh run today, so the
# receipt can distinguish a real rebuild from a cache-only no-op.
for d in /home/hoo/qianqian-172-linux/ops/build-evidence/20261030T12*; do
  echo "dir=$d"
  grep -o 'build ok, spent [0-9.]*s' "$d/xmake-build.log" 2>/dev/null | tail -1
  grep -c 'cache compiling\|compiling' "$d/xmake-build.log" 2>/dev/null | sed 's/^/compile_lines=/'
  ls "$d/summary.json" 2>/dev/null
done
