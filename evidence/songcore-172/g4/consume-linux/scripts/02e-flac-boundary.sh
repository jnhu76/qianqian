#!/usr/bin/env bash
# Lane C (linux) - stage 02e: boundary of the FLAC post-EOF seek failure. Diagnostic only.
set -u
PKG=/home/hoo/g4-172/laneB-stage/linux/songcore-v0.0.0-preflight-linux-x86_64
W=/home/hoo/g4-172/consume-linux
EVID=/mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/g4/consume-linux
LOG=$EVID/logs/stage02e-flac-boundary.log
: > "$LOG"
F=$W/fixtures/flac-16-44-stereo.flac

run() {
    local log=$1; shift
    printf '\n$ %s\n' "$*" >> "$log"
    "$@" >> "$log" 2>&1
    local rc=$?
    printf '[exit] %d\n' "$rc" >> "$log"
    echo "EXIT=$rc : $*"
    return $rc
}

for t in 3000000 3500000 3700000 3750000 3761000 3762000 3765000 3770000 3800000 3950000; do
  run "$LOG" "$W/bin/op-probe-static" "$F" d "s:$t" e
done

echo "--- log ---"
cat "$LOG"
