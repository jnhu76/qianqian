#!/usr/bin/env bash
# Lane C (linux) - stage 03: run the STATIC consumers (package lib/libsongcore.a)
# over the frozen corpus; record argv + exit code per run.
set -u

W=/home/hoo/g4-172/consume-linux
EVID=/mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/g4/consume-linux
LOG=$EVID/logs
LOG_FILE=$EVID/logs/stage03-static-run.log
mkdir -p "$W/out/static"
: > "$LOG_FILE"

declare -A FIX=(
  [mp3-cbr-id3v23]=mp3-cbr-id3v23.mp3
  [flac-16-44-stereo]=flac-16-44-stereo.flac
  [alac-16-44-stereo]=alac-16-44-stereo.m4a
  [alac-long]=alac-long.m4a
)
ORDER="mp3-cbr-id3v23 flac-16-44-stereo alac-16-44-stereo alac-long"

OVERALL=0
for id in $ORDER; do
  f=$W/fixtures/${FIX[$id]}

  # --- package example consumer (open/probe/decode/seek/EOF/close smoke) ---
  printf '\n=== [static] consumer-example-static %s ===\n' "$f" >> "$LOG_FILE"
  printf '$ %s %s\n' "$W/bin/consumer-example-static" "$f" >> "$LOG_FILE"
  "$W/bin/consumer-example-static" "$f" > "$W/out/static/example-$id.out" 2>&1
  rc=$?
  printf '[exit] %d\n' "$rc" >> "$LOG_FILE"
  cat "$W/out/static/example-$id.out" >> "$LOG_FILE"
  echo "RUN static/example $id EXIT=$rc"
  [ $rc -ne 0 ] && OVERALL=1

  # --- corpus consumer (open/probe/metadata/full decode sha256/seek 25-50-75/EOF drain/close) ---
  printf '\n=== [static] consume-corpus-static %s ===\n' "$f" >> "$LOG_FILE"
  printf '$ %s %s\n' "$W/bin/consume-corpus-static" "$f" >> "$LOG_FILE"
  "$W/bin/consume-corpus-static" "$f" > "$W/out/static/corpus-$id.jsonl" 2> "$W/out/static/corpus-$id.err"
  rc=$?
  printf '[exit] %d\n' "$rc" >> "$LOG_FILE"
  cat "$W/out/static/corpus-$id.err" >> "$LOG_FILE"
  echo "RUN static/corpus $id EXIT=$rc"
  [ $rc -ne 0 ] && OVERALL=1
done

echo "STATIC_RUNS_EXIT=$OVERALL"
