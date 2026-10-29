#!/bin/bash
# songcore-172 D2: RED build (already-configured win tree).
set -uo pipefail
export PATH=/home/hoo/.local/bin:$PATH
W=/home/hoo/q172d2/win
LOG=/home/hoo/q172d2/red-build.log
cd "$W/native"
echo "=== xmake build songcore (RED) ==="
xmake build songcore > "$LOG" 2>&1
RC=$?
echo "RED_BUILD_EXIT=$RC"
echo "=== artifacts after RED build ==="
ls -la build/artifacts/ 2>/dev/null || echo "no artifacts dir"
ls -la build/artifacts/shared/ 2>/dev/null || echo "no shared dir"
echo "=== RED log tail (last 60) ==="
tail -60 "$LOG"
echo "=== RED log: BCrypt/undefined refs ==="
grep -n "BCrypt\|undefined\|songcore_shared\|songcore_static" "$LOG" | head -30
echo "RED_DONE"
