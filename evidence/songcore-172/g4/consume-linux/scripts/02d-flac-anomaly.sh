#!/usr/bin/env bash
# Lane C (linux) - stage 02d: pin down the FLAC post-EOF seek anomaly.
# Diagnostic only (package-static build); not a G2 gate.
set -u
PKG=/home/hoo/g4-172/laneB-stage/linux/songcore-v0.0.0-preflight-linux-x86_64
W=/home/hoo/g4-172/consume-linux
EVID=/mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/g4/consume-linux
LOG=$EVID/logs/stage02d-flac-seek-anomaly.log
: > "$LOG"

run() {
    local log=$1; shift
    printf '\n$ %s\n' "$*" >> "$log"
    "$@" >> "$log" 2>&1
    local rc=$?
    printf '[exit] %d\n' "$rc" >> "$log"
    echo "EXIT=$rc : $*"
    return $rc
}

cp "$EVID/src/op_probe.c" "$W/src/op_probe.c"
run "$LOG" gcc -O2 -I"$PKG/include" "$W/src/op_probe.c" "$PKG/lib/libsongcore.a" -lm -lpthread -o "$W/bin/op-probe-static"

F=$W/fixtures/flac-16-44-stereo.flac
L=$W/fixtures/alac-long.m4a

echo "=== P1: FLAC drain-to-EOF then a SINGLE near-end seek (with last_error) ==="
run "$LOG" "$W/bin/op-probe-static" "$F" d s:3800000 e r:4096 e

echo
echo "=== P2: FLAC drain-to-EOF then seek 3.0, read, then 3.8 (last_error captured) ==="
run "$LOG" "$W/bin/op-probe-static" "$F" d s:3000000 r:4096 s:3800000 e r:4096 e

echo
echo "=== P3: FLAC drain-to-EOF then seek/read 25-50-75 then 3.8 + retry after failed seek ==="
run "$LOG" "$W/bin/op-probe-static" "$F" d s:1000000 r:4096 s:2000000 r:4096 s:3000000 r:4096 s:3800000 e r:4096 s:3800000 e r:4096 d

echo
echo "=== P4: FLAC drain-to-EOF then seek 3.9 (single) ==="
run "$LOG" "$W/bin/op-probe-static" "$F" d s:3900000 e r:4096 d

echo
echo "=== P5: FLAC drain-to-EOF then seek 2.0 then 3.8 (no 3.0) ==="
run "$LOG" "$W/bin/op-probe-static" "$F" d s:2000000 r:4096 s:3800000 e r:4096 d

echo
echo "=== P6: FLAC drain-to-EOF then 3.8 then 2.0 then 3.8 ==="
run "$LOG" "$W/bin/op-probe-static" "$F" d s:3800000 e r:4096 s:2000000 e r:4096 s:3800000 e r:4096 d

echo
echo "=== P7: alac-long (6 s) same corpus sequence, last_error captured ==="
run "$LOG" "$W/bin/op-probe-static" "$L" d s:1500000 r:4096 s:3000000 r:4096 s:4500000 r:4096 s:5800000 e r:4096 d

echo
echo "--- log ---"
cat "$LOG"
