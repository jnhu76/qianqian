#!/usr/bin/env bash
# Lane C (linux) - stage 02c: characterize the observed seek anomaly with an
# explicit op script (diagnostic only, package-static build).
set -u
PKG=/home/hoo/g4-172/laneB-stage/linux/songcore-v0.0.0-preflight-linux-x86_64
W=/home/hoo/g4-172/consume-linux
EVID=/mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/g4/consume-linux
LOG=$EVID/logs/stage02c-op-probe.log
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
M=$W/fixtures/mp3-cbr-id3v23.mp3
A=$W/fixtures/alac-16-44-stereo.m4a

echo "=== A: FLAC corpus-consumer sequence (full decode to EOF, then seek/read x3, then near-end seek) ==="
run "$LOG" "$W/bin/op-probe-static" "$F" d s:1000000 r:4096 s:2000000 r:4096 s:3000000 r:4096 s:3800000 r:4096 d

echo
echo "=== B: FLAC same but no initial full decode ==="
run "$LOG" "$W/bin/op-probe-static" "$F" s:1000000 r:4096 s:2000000 r:4096 s:3000000 r:4096 s:3800000 r:4096 d

echo
echo "=== C: FLAC seek straight to near end on a fresh handle ==="
run "$LOG" "$W/bin/op-probe-static" "$F" s:3800000 r:4096 d

echo
echo "=== D: FLAC seek 3.0s, read, then 3.8s (no earlier seeks) ==="
run "$LOG" "$W/bin/op-probe-static" "$F" s:3000000 r:4096 s:3800000 r:4096

echo
echo "=== E: FLAC repeated identical near-end seek on one handle ==="
run "$LOG" "$W/bin/op-probe-static" "$F" s:3800000 r:4096 s:3800000 r:4096 s:3800000 r:4096

echo
echo "=== F: FLAC backwards seeks from near end ==="
run "$LOG" "$W/bin/op-probe-static" "$F" s:3800000 r:4096 s:3000000 r:4096 s:3800000 r:4096

echo
echo "=== G: FLAC metadata validity across seek / EOF ==="
run "$LOG" "$W/bin/op-probe-static" "$F" m s:3000000 r:4096 m d m

echo
echo "=== H: MP3 same corpus-consumer sequence ==="
run "$LOG" "$W/bin/op-probe-static" "$M" d s:1000000 r:4096 s:2000000 r:4096 s:3000000 r:4096 s:3800000 r:4096 d

echo
echo "=== I: ALAC same corpus-consumer sequence ==="
run "$LOG" "$W/bin/op-probe-static" "$A" d s:1000000 r:4096 s:2000000 r:4096 s:3000000 r:4096 s:3800000 r:4096 d

sed -n '/op-probe-static-op\|^===/,$p' "$LOG"
