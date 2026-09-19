#!/usr/bin/env bash
# Stage A machine-mode regression (campaign §7): drives the REAL
# `<product-binary> --machine play` transport (qianqian-headless.exe by
# default; QIANQIAN_MACHINE_BIN selects the canonical qianqian.exe)
# over cmd.exe with
# piped stdin, asserting the pinned observable contract (grammar,
# truthful status projection, outcome lines, exit codes, disposal
# quietness). Machine mode is the line-based automation transport; no
# console is required, so plain interop works here.
#
# Usage: tools/run-machine.sh <run-number>
set -uo pipefail

RUN="${1:?usage: run-machine.sh <run-number>}"
REPO="$(cd "$(dirname "$0")/../../.." && pwd)"
STAGE_WSL="/mnt/c/Users/Public/qianqian-dogfood"
LOGDIR="$REPO/research/transport-dogfood/evidence/logs"
# The exercised product binary (QIANQIAN_MACHINE_BIN=qianqian.exe
# selects the canonical product binary, U2 canonical product gate).
BIN="${QIANQIAN_MACHINE_BIN:-qianqian-headless.exe}"
PRODUCT="$REPO/target/x86_64-pc-windows-gnu/release/$BIN"

mkdir -p "$LOGDIR"
cp "$PRODUCT" "$STAGE_WSL/$BIN"

PASS=0; FAIL=0
note() { echo "M-$1: $2"; }

# run <id> <media> <stdin-file-or-NONE> <grep-assertion-function>
run() {
  local id="$1" media="$2" stdin_file="$3"
  local out="$STAGE_WSL/m-$id.out" err="$STAGE_WSL/m-$id.err"
  if [ "$stdin_file" = "NONE" ]; then
    ( cd "$STAGE_WSL" && cmd.exe /c "$BIN --machine play $media" </dev/null >"$(basename "$out")" 2>"$(basename "$err")" )
  else
    ( cd "$STAGE_WSL" && cmd.exe /c "$BIN --machine play $media <$(basename "$stdin_file")" >"$(basename "$out")" 2>"$(basename "$err")" )
  fi
  local rc=$?
  tr -d '\r' < "$out" > "$LOGDIR/m-$id.out" 2>/dev/null
  tr -d '\r' < "$err" > "$LOGDIR/m-$id.err" 2>/dev/null
  M_RC=$rc; M_OUT="$LOGDIR/m-$id.out"; M_ERR="$LOGDIR/m-$id.err"
}

assert() { # assert <description> <file> <fixed-string>
  if grep -qF -- "$3" "$2"; then return 0; else
    note "$M_ID" "FAIL: $1 (missing $3 in $2)"; FAIL=$((FAIL+1)); return 1
  fi
}
assert_not() {
  if grep -qF -- "$3" "$2"; then
    note "$M_ID" "FAIL: $1 (forbidden $3 present in $2)"; FAIL=$((FAIL+1)); return 1
  else return 0; fi
}
check_rc() { # check_rc <expected>
  if [ "$M_RC" = "$1" ]; then return 0; else
    note "$M_ID" "FAIL: exit code $M_RC, expected $1"; FAIL=$((FAIL+1)); return 1
  fi
}
scenario_pass() { note "$M_ID" "GREEN"; PASS=$((PASS+1)); }

# M1 — natural EOF on a 4 s fixture; quiet disposal; exit 0.
M_ID=M1
run M1 flac4.flac NONE
check_rc 0 && assert "source line" "$M_OUT" "source: 44100 Hz" \
  && assert "playing line" "$M_OUT" "playing" \
  && assert "EOF report" "$M_OUT" "EOF: played out completely" \
  && assert_not "no failure line" "$M_ERR" "playback failed" \
  && assert_not "no disposal warnings" "$M_ERR" "warning:" \
  && scenario_pass

# M2 — status then stop; immutable Stopped terminal; exit 0.
M_ID=M2
printf 'status\nstop\n' > "$STAGE_WSL/m-M2.in"
run M2 synth45.mp3 "$STAGE_WSL/m-M2.in"
check_rc 0 && assert "status pending" "$M_OUT" "outcome: pending" \
  && assert "stopped report" "$M_OUT" "stopped before completion" \
  && assert_not "no EOF" "$M_OUT" "EOF: played out completely" \
  && scenario_pass

# M3 — pause / resume through the seam; truthful command state.
M_ID=M3
printf 'pause\nstatus\nresume\nstatus\n' > "$STAGE_WSL/m-M3.in"
run M3 synth45.mp3 "$STAGE_WSL/m-M3.in"
check_rc 0 && assert "pause intent recorded" "$M_OUT" "pause_requested: true" \
  && assert "resume released" "$M_OUT" "pause_requested: false" \
  && assert_not "no invented fourth state" "$M_OUT" "outcome: paused" \
  && scenario_pass

# M4 — seek forward; position rebase visible in the projection. The
# feeder must PIPE with the sleep between commands: sleeps written into
# a stdin FILE execute at authoring time (run-F lesson) and the status
# would then race the cutover and truthfully show --:--. Stop ends the
# episode so the scenario does not run to the 45 s natural EOF.
M_ID=M4
out="$STAGE_WSL/m-M4.out"; err="$STAGE_WSL/m-M4.err"
( cd "$STAGE_WSL" && cmd.exe /c "$BIN --machine play synth45.mp3" \
    < <( { printf 'seek 5\n'; sleep 2; printf 'status\n'; sleep 1; printf 'stop\n'; } ) \
    >"$(basename "$out")" 2>"$(basename "$err")" )
rc=$?
tr -d '\r' < "$out" > "$LOGDIR/m-M4.out" 2>/dev/null
tr -d '\r' < "$err" > "$LOGDIR/m-M4.err" 2>/dev/null
M_RC=$rc; M_OUT="$LOGDIR/m-M4.out"; M_ERR="$LOGDIR/m-M4.err"
# The projection advances between the rebase (5 s) and the status read,
# so the honest witness is a bounded window, not an exact second.
check_rc 0 && grep -qE -- "position: 00:0[5-9] / 00:45" "$M_OUT" \
  && assert "stopped report" "$M_OUT" "stopped before completion" \
  && scenario_pass

# M5 — unreadable seek tokens are inert input, never a crash.
M_ID=M5
printf 'seek nan\nseek 1e400\nseek -5\nstatus\nstop\n' > "$STAGE_WSL/m-M5.in"
run M5 synth45.mp3 "$STAGE_WSL/m-M5.in"
check_rc 0 && assert "nan inert" "$M_ERR" "cannot read seek time" \
  && assert "status rendered" "$M_OUT" "outcome:" \
  && assert "stopped" "$M_OUT" "stopped before completion" \
  && scenario_pass

# M6 — invalid candidate: activation failure reported, no forged
# terminal outcome, exit 1.
M_ID=M6
run M6 garbage.bin NONE
check_rc 1 && assert "activation report" "$M_ERR" "playback session" \
  && assert_not "no fabricated EOF" "$M_OUT" "EOF: played out completely" \
  && assert_not "no fabricated failure" "$M_OUT" "playback failed" \
  && scenario_pass

# M7 — unknown interactive command stays presentation noise.
M_ID=M7
printf 'frobnicate now\nstop\n' > "$STAGE_WSL/m-M7.in"
run M7 synth45.mp3 "$STAGE_WSL/m-M7.in"
check_rc 0 && assert "unknown reported" "$M_ERR" "ignored input" \
  && assert "stop still worked" "$M_OUT" "stopped before completion" \
  && scenario_pass

echo "machine regression run$RUN: PASS=$PASS FAIL=$FAIL"
{
  echo "binary: $BIN"
  echo "binary_sha256: $(sha256sum "$STAGE_WSL/$BIN" | cut -d' ' -f1)"
  echo "machine run$RUN: PASS=$PASS FAIL=$FAIL"
} > "$LOGDIR/machine-run$RUN.summary"
[ "$FAIL" = 0 ]
