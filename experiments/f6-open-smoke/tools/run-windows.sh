#!/usr/bin/env bash
# F6 Open physical Windows smoke runner (executed from WSL; drives the
# real host over cmd.exe interop). Stages the cross-built harness +
# fixtures under C:\Users\Public\qianqian-osmoke\ and runs the
# scenario matrix, saving one JSON log per scenario into
# evidence/logs/ plus stderr trails, and an environment header.
#
# Usage: tools/run-windows.sh <run-number>
# Prerequisites:
#   - osmoke.exe cross-built:
#       cd experiments/f6-open-smoke
#       QIANQIAN_NATIVE_DIR=<mingw staging> cargo build --release \
#           --target x86_64-pc-windows-gnu
set -euo pipefail

RUN="${1:?usage: run-windows.sh <run-number>}"
REPO="$(cd "$(dirname "$0")/../../.." && pwd)"
STAGE_WSL="/mnt/c/Users/Public/qianqian-osmoke"
LOGDIR="$REPO/experiments/f6-open-smoke/evidence/logs"
FIX="$REPO/native/experiments/songcore-equivalence/fixtures"
EXE="$REPO/experiments/f6-open-smoke/target/x86_64-pc-windows-gnu/release/osmoke.exe"

mkdir -p "$LOGDIR" "$STAGE_WSL"
cp "$EXE" "$STAGE_WSL/osmoke.exe"
# The MAIN file: a synthetic 45 s 44.1 kHz stereo CBR MP3 (ffmpeg
# sine), generated locally if not staged yet.
if [ ! -f "$STAGE_WSL/main45.mp3" ]; then
  ffmpeg -v error -f lavfi -i "sine=frequency=440:sample_rate=44100:duration=45" \
      -ac 2 -b:a 128k -y "$STAGE_WSL/main45.mp3"
fi
# Valid candidates + an invalid candidate (same classes as S-PROBE).
cp "$FIX/mp3-cbr-id3v23.mp3" "$STAGE_WSL/mp3-valid.mp3"
cp "$FIX/flac-16-44-stereo.flac" "$STAGE_WSL/flac-valid.flac"
head -c 1024 /dev/urandom > "$STAGE_WSL/garbage.mp3"

ENVFILE="$REPO/experiments/f6-open-smoke/evidence/ENV-RUN${RUN}.txt"
{
  echo "run: $RUN"
  echo "date_utc: $(date -u +%FT%TZ)"
  echo "repo_head_sha: $(git -C "$REPO" rev-parse HEAD)"
  echo "branch: $(git -C "$REPO" branch --show-current)"
  echo "harness_tree_status:"
  git -C "$REPO" status --porcelain | sed 's/^/  /' || true
  echo "exe_sha256: $(sha256sum "$EXE" | cut -d' ' -f1)"
  echo "main45_sha256: $(sha256sum "$STAGE_WSL/main45.mp3" | cut -d' ' -f1)"
  echo "mp3_valid_sha256: $(sha256sum "$STAGE_WSL/mp3-valid.mp3" | cut -d' ' -f1)"
  echo "flac_valid_sha256: $(sha256sum "$STAGE_WSL/flac-valid.flac" | cut -d' ' -f1)"
  echo "windows_caption_raw: $(powershell.exe -NoProfile -Command '(Get-CimInstance Win32_OperatingSystem).Caption' | tr -d '\r')"
  echo "audio_device: $(powershell.exe -NoProfile -Command '(Get-CimInstance Win32_SoundDevice | Select-Object -First 1).Name' | tr -d '\r')"
  echo "endpoint_note: default render endpoint (shared mode, event-driven), as opened by the harness stderr"
} > "$ENVFILE"

cd "$STAGE_WSL"

run() { # run <scenario> <args...>
  local scenario="$1"; shift
  local log="$LOGDIR/${scenario}-run${RUN}.json"
  local err="$LOGDIR/${scenario}-run${RUN}.stderr"
  # shellcheck disable=SC2086
  if cmd.exe /c "osmoke.exe --scenario $scenario --main main45.mp3 $*" \
      > "$log" 2> "$err"; then
    echo "run$RUN $scenario: GREEN (exit 0)"
  else
    local rc=$?
    echo "run$RUN $scenario: RED (exit $rc)"
  fi
  tr -d '\r' < "$err" | grep -E "verdict=|opened:" | sed "s/^/    /" || true
}

run O1 "--cand mp3-valid.mp3"
run O2 "--cand garbage.mp3 --cand mp3-valid.mp3"
run O3 "--cand flac-valid.flac"
run O4 "--cand mp3-valid.mp3"
run O5 "--cand mp3-valid.mp3 --cand flac-valid.flac --cand mp3-valid.mp3"
run O6 "--cand mp3-valid.mp3 --cand flac-valid.flac"
run O7 "--cand mp3-valid.mp3"

echo "run $RUN complete; logs in evidence/logs/, env in $ENVFILE"
