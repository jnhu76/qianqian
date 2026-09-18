#!/usr/bin/env bash
# F6-S-PROBE physical Windows runner (executed from WSL; drives the real
# host over cmd.exe interop). Stages the cross-built harness + fixtures
# under C:\Users\Public\qianqian-sprobe\ and runs the full scenario
# matrix N times, saving one JSON log per run into evidence/logs/ and a
# header of environment identities into evidence/ENV-RUN<N>.txt.
#
# Usage: tools/run-windows.sh <run-number>
# Prerequisites:
#   - sprobe.exe cross-built:
#       cd experiments/f6-source-probe
#       QIANQIAN_NATIVE_DIR=<mingw staging> cargo build --release \
#           --target x86_64-pc-windows-gnu
#   - the mingw COFF SongCore archive staged for the build (see README)
set -euo pipefail

RUN="${1:?usage: run-windows.sh <run-number>}"
REPO="$(cd "$(dirname "$0")/../../.." && pwd)"
STAGE_WSL="/mnt/c/Users/Public/qianqian-sprobe"
STAGE_WIN='C:\Users\Public\qianqian-sprobe'
LOGDIR="$REPO/experiments/f6-source-probe/evidence/logs"
FIX="$REPO/native/experiments/songcore-equivalence/fixtures"
EXE="$REPO/experiments/f6-source-probe/target/x86_64-pc-windows-gnu/release/sprobe.exe"

mkdir -p "$LOGDIR" "$STAGE_WSL"
cp "$EXE" "$STAGE_WSL/sprobe.exe"
# The MAIN file: a synthetic 45 s 44.1 kHz stereo CBR MP3 (ffmpeg sine),
# generated locally if not staged yet. It only needs to be real,
# decodable audio long enough for every scenario window.
if [ ! -f "$STAGE_WSL/main45.mp3" ]; then
  ffmpeg -v error -f lavfi -i "sine=frequency=440:sample_rate=44100:duration=45" \
      -ac 2 -b:a 128k -y "$STAGE_WSL/main45.mp3"
fi
# Fixtures (valid candidates)
cp "$FIX/mp3-cbr-id3v23.mp3" "$STAGE_WSL/mp3-valid.mp3"
cp "$FIX/flac-16-44-stereo.flac" "$STAGE_WSL/flac-valid.flac"
cp "$FIX/alac-long.m4a" "$STAGE_WSL/m4a-valid.m4a"
# Invalid candidates: garbage bytes, empty file, truncated container.
head -c 1024 /dev/urandom > "$STAGE_WSL/garbage.mp3"
: > "$STAGE_WSL/empty.mp3"
head -c 300 "$FIX/mp3-cbr-id3v23.mp3" > "$STAGE_WSL/truncated.mp3"

# Environment identities for this run.
ENVFILE="$REPO/experiments/f6-source-probe/evidence/ENV-RUN${RUN}.txt"
{
  echo "run: $RUN"
  echo "date_utc: $(date -u +%FT%TZ)"
  echo "repo_main_sha: $(git -C "$REPO" rev-parse HEAD)"
  echo "harness_tree_dirty_files:"
  git -C "$REPO" status --porcelain -- experiments/f6-source-probe | sed 's/^/  /'
  echo "exe_sha256: $(sha256sum "$EXE" | cut -d' ' -f1)"
  echo "main45_sha256: $(sha256sum "$STAGE_WSL/main45.mp3" | cut -d' ' -f1)"
  echo "mp3_valid_sha256: $(sha256sum "$STAGE_WSL/mp3-valid.mp3" | cut -d' ' -f1)"
  echo "flac_valid_sha256: $(sha256sum "$STAGE_WSL/flac-valid.flac" | cut -d' ' -f1)"
  echo "m4a_valid_sha256: $(sha256sum "$STAGE_WSL/m4a-valid.m4a" | cut -d' ' -f1)"
  echo "windows: $(powershell.exe -NoProfile -Command '(Get-CimInstance Win32_OperatingSystem).Caption; (Get-CimInstance Win32_OperatingSystem).BuildNumber' | tr -d '\r' | paste -sd' ' -)"
  echo "audio_device: $(powershell.exe -NoProfile -Command '(Get-CimInstance Win32_SoundDevice | Select-Object -First 1).Name' | tr -d '\r')"
  echo "endpoint_note: default render endpoint (shared mode, event-driven), as opened by the harness log line"
} > "$ENVFILE"

cd "$STAGE_WSL"

run() { # run <scenario> <json-name> <args...>
  local scenario="$1"; local name="$2"; shift 2
  local log="$LOGDIR/${name}-run${RUN}.json"
  # shellcheck disable=SC2086
  if cmd.exe /c "sprobe.exe --scenario $scenario --main main45.mp3 $*" \
      > "$log" 2> "$LOGDIR/.stderr-tmp"; then
    echo "run$RUN $name: matched (exit 0)"
  else
    local rc=$?
    echo "run$RUN $name: MISMATCH (exit $rc)"
  fi
  cat "$LOGDIR/.stderr-tmp" | tr -d '\r' | grep -E "verdict=|reason:" | sed "s/^/    /" || true
}

run S1  s1  "--cand mp3-valid.mp3"
run S2  s2  "--cand flac-valid.flac"
run S3  s3  "--cand garbage.mp3"
run S4  s4  "--cand mp3-valid.mp3"
run S5  s5  "--cand mp3-valid.mp3"
run S6  s6  "--cand mp3-valid.mp3"
run S7  s7  "--cand missing-file.mp3 --cand empty.mp3 --cand garbage.mp3"
run S8  s8  "--cand m4a-valid.m4a"
run S9  s9  "--cand mp3-valid.mp3 --cand garbage.mp3"
run S10 s10 "--cand flac-valid.flac --cand mp3-valid.mp3"
run NEG neg "--cand mp3-valid.mp3"

echo "run $RUN complete; logs in evidence/logs/, env in $ENVFILE"
