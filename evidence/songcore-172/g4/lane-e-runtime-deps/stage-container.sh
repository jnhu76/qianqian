#!/usr/bin/env bash
# Lane E: stage artifacts for container runs and verify docker mount access.
set -u
export LC_ALL=C
W="$HOME/lane-e-runtime/work"
STAGE=/mnt/c/Users/fred1/lane-e-container
MEDIA=/home/hoo/Source/qianqian/native/experiments/songcore-equivalence/fixtures/mp3-cbr-id3v23.mp3
EVD=/mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/g4/lane-e-runtime-deps
RAW="$EVD/raw"
LOG="$RAW/container-exitcodes.txt"
: > "$LOG"

logrun() {
  local desc="$1"; local out="$2"; shift 2
  "$@" > "$out" 2>&1
  local rc=$?
  printf '%s | argv: %s | exit=%s | output=%s\n' "$desc" "$*" "$rc" "$out" >> "$LOG"
  return $rc
}

rm -rf "$STAGE"; mkdir -p "$STAGE"
cp "$W/shared/libsongcore.so" "$STAGE/"
cp "$W/shared/external-consumer-shared" "$STAGE/"
cp "$W/shared/abi-complete-shared" "$STAGE/"
cp "$W/shared/abi-sizeof-probe-shared" "$STAGE/"
cp "$W/static/external-consumer-static" "$STAGE/"
cp "$W/static/minimal-libs" "$STAGE/"
ls -l "$STAGE" > "$RAW/container-stage-listing.txt" 2>&1

logrun "docker version (WSL client)" "$RAW/docker-version.txt" docker version
logrun "alpine smoke" "$RAW/docker-smoke.txt" docker run --rm alpine:latest echo ok
logrun "mount visibility /mnt/c stage" "$RAW/docker-mount-test.txt" \
  docker run --rm -v "$STAGE:/work" alpine:latest ls -l /work

echo "done"
