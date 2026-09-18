#!/usr/bin/env bash
# V-PROBE physical Windows runner (executed from WSL; drives the real
# host over cmd.exe interop). Stages the cross-built harness under
# C:\Users\Public\qianqian-vprobe\ and runs the scenario matrix N
# times, saving one JSON verdict per scenario into evidence/logs/ plus
# stderr trails, and an environment header per run.
#
# Usage: tools/run-windows.sh <run-number>
# Prerequisites:
#   cargo build --release --target x86_64-pc-windows-gnu (this crate)
set -euo pipefail

RUN="${1:?usage: run-windows.sh <run-number>}"
REPO="$(cd "$(dirname "$0")/../../.." && pwd)"
STAGE_WSL="/mnt/c/Users/Public/qianqian-vprobe"
LOGDIR="$REPO/experiments/v-probe/evidence/logs"
EXE="$REPO/experiments/v-probe/target/x86_64-pc-windows-gnu/release/v-probe.exe"

mkdir -p "$LOGDIR" "$STAGE_WSL"
cp "$EXE" "$STAGE_WSL/v-probe.exe"

ENVFILE="$REPO/experiments/v-probe/evidence/ENV-RUN${RUN}.txt"
{
  echo "run: $RUN"
  echo "date_utc: $(date -u +%FT%TZ)"
  echo "repo_head_sha: $(git -C "$REPO" rev-parse HEAD)"
  echo "branch: $(git -C "$REPO" branch --show-current)"
  echo "harness_tree_status:"
  git -C "$REPO" status --porcelain -- experiments/v-probe | sed 's/^/  /' || true
  echo "exe_sha256: $(sha256sum "$EXE" | cut -d' ' -f1)"
  echo "windows_caption_raw: $(powershell.exe -NoProfile -Command '(Get-CimInstance Win32_OperatingSystem).Caption' | tr -d '\r')"
  echo "audio_device: $(powershell.exe -NoProfile -Command '(Get-CimInstance Win32_SoundDevice | Select-Object -First 1).Name' | tr -d '\r')"
  echo "endpoint_note: default render endpoint (shared mode, event-driven float32), opened by the harness; factors are stream-local (IAudioStreamVolume)"
} > "$ENVFILE"

cd "$STAGE_WSL"

run() { # run <scenario>
  local scenario="$1"
  local log="$LOGDIR/${scenario}-run${RUN}.json"
  local err="$LOGDIR/${scenario}-run${RUN}.stderr"
  if cmd.exe /c "v-probe.exe --scenario $scenario" > "$log" 2> "$err"; then
    echo "run$RUN $scenario: GREEN (exit 0)"
  else
    local rc=$?
    echo "run$RUN $scenario: RED (exit $rc)"
  fi
  tr -d '\r' < "$err" | grep -E "verdict=" | sed "s/^/    /" || true
}

run V1a
run V1b
run V2a
run V2b
run V3
run V4
run V5

echo "run $RUN complete; logs in evidence/logs/, env in $ENVFILE"
