#!/usr/bin/env bash
# Stage A TUI scenario runner (from WSL; drives the real Windows host).
#
# The ConPTY child must NOT inherit the WSL interop console — under that
# console the pseudoconsole attribute is ignored and the child renders
# to the interop terminal instead of the pipe. Each scenario therefore
# runs detached through Start-Process (hidden window), with
# stdout/stderr redirected to files on the Windows side; the driver's
# exit code rides through $p.ExitCode.
#
# Usage: tools/run-tui.sh <run-number> SCENARIO [SCENARIO...]
set -uo pipefail

RUN="${1:?usage: run-tui.sh <run-number> SCENARIO...}"
shift
REPO="$(cd "$(dirname "$0")/../../.." && pwd)"
HARNESS="$REPO/research/transport-dogfood"
STAGE_WSL="/mnt/c/Users/Public/qianqian-dogfood"
LOGDIR="$HARNESS/evidence/logs"
EXE="$HARNESS/target/x86_64-pc-windows-gnu/release/tuidriver.exe"
# The exercised product binary. The default keeps the historical
# regression target; QIANQIAN_TUI_BIN=qianqian.exe selects the
# canonical product binary (U2 canonical product gate, PR #168). The
# ConPTY driver is binary-agnostic (--exe); only this staging wrapper
# names the executable, so the selection lives here.
BIN="${QIANQIAN_TUI_BIN:-qianqian-headless.exe}"
PRODUCT="$REPO/target/x86_64-pc-windows-gnu/release/$BIN"
MEDIA="C:\\Users\\Public\\qianqian-dogfood"
OUT="C:\\Users\\Public\\qianqian-dogfood\\evidence"

mkdir -p "$LOGDIR"
# Leftover processes from a previous run hold the staged exes and the
# audio endpoint; clear them before staging (a warm restart immediately
# after another run otherwise loses the device-open race).
taskkill.exe /F /IM qianqian-headless.exe /T >/dev/null 2>&1 || true
taskkill.exe /F /IM qianqian.exe /T >/dev/null 2>&1 || true
taskkill.exe /F /IM tuidriver.exe /T >/dev/null 2>&1 || true
sleep 1
cp "$EXE" "$STAGE_WSL/tuidriver.exe"
cp "$PRODUCT" "$STAGE_WSL/$BIN"

# Stage-C scenario fixtures (C11/C12): renamed copies of the committed
# 4 s FLAC fixture — one very long filename (96 'a's), one CJK
# filename. Idempotent; the sha256 line below records the staged corpus
# either way.
FIX="$REPO/native/experiments/songcore-equivalence/fixtures"
LONGNAME="qianqian-longpath-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.flac"
[ -f "$STAGE_WSL/千曲.flac" ] || cp "$FIX/flac-16-44-stereo.flac" "$STAGE_WSL/千曲.flac"
[ -f "$STAGE_WSL/$LONGNAME" ] || cp "$FIX/flac-16-44-stereo.flac" "$STAGE_WSL/$LONGNAME"

# U1 scenario fixture (Issue #166 §17): the u1music FOLDER the
# U1-folder-open scenario types into the O line — exactly two audio
# candidates, flac4 sorting first and synth45 second (the path-sorted
# expansion order the scenario's needles pin). Idempotent like the
# rest; the sha256 line below records the folder contents too.
mkdir -p "$STAGE_WSL/u1music"
[ -f "$STAGE_WSL/u1music/flac4.flac" ] || cp "$STAGE_WSL/flac4.flac" "$STAGE_WSL/u1music/flac4.flac"
[ -f "$STAGE_WSL/u1music/synth45.mp3" ] || cp "$STAGE_WSL/synth45.mp3" "$STAGE_WSL/u1music/synth45.mp3"

# U2 scenario fixtures (Issue #166 §51). The 24-entry viewport list is
# 24 renamed copies of the 45 s synthetic sine; the soak list is 20 x
# 100 s synthetic tracks (~33 min of playback). Both are SYNTHETIC
# media generated locally with ffmpeg, exactly like synth45/synth30 —
# declared in CORPUS.md and SHA256-recorded per run below.
for i in $(seq -w 1 24); do
  [ -f "$STAGE_WSL/vtest$i.mp3" ] || cp "$STAGE_WSL/synth45.mp3" "$STAGE_WSL/vtest$i.mp3"
done
mkdir -p "$STAGE_WSL/u2soak"
for i in $(seq -w 1 20); do
  if [ ! -f "$STAGE_WSL/u2soak/soak$i.mp3" ]; then
    ffmpeg -v error -f lavfi -i "sine=frequency=$((300 + 10#$i)):sample_rate=44100:duration=100" \
        -ac 2 -b:a 128k -y "$STAGE_WSL/u2soak/soak$i.mp3"
  fi
done

SCEN="$*"
PS_SCEN=$(printf "'%s'," $SCEN | sed 's/,$//')

# Strong run identity (campaign §4 / addendum §5): each claimed result
# must be attributable to an exact binary hash and corpus state.
ENVFILE="$HARNESS/evidence/ENV-TUI-RUN${RUN}.txt"
{
  echo "run: TUI-RUN$RUN"
  echo "date_utc: $(date -u +%FT%TZ)"
  echo "branch: $(git -C "$REPO" branch --show-current)"
  echo "head_sha: $(git -C "$REPO" rev-parse HEAD)"
  echo "origin_main: $(git -C "$REPO" rev-parse origin/main)"
  echo "worktree_status: $(git -C "$REPO" status --porcelain | wc -l) dirty entries"
  echo "build_command: QIANQIAN_NATIVE_DIR=<mingw staging> cargo build --release --target x86_64-pc-windows-gnu --features playback -p qianqian-headless"
  echo "target_triple: x86_64-pc-windows-gnu (GNU toolchain)"
  echo "features: playback profile: release"
  echo "product_binary: $BIN"
  echo "product_binary_sha256: $(sha256sum "$PRODUCT" | cut -d' ' -f1)"
  echo "tuidriver_sha256: $(sha256sum "$EXE" | cut -d' ' -f1)"
  echo "songcore_sha256: $(sha256sum "${QIANQIAN_NATIVE_DIR:-/tmp/qn-dogfood-stage/native}/build/artifacts/libsongcore.a" | cut -d' ' -f1)"
  echo "rustc: $(rustc --version)"
  echo "corpus_sha256:"
  sha256sum "$STAGE_WSL"/*.mp3 "$STAGE_WSL"/*.flac "$STAGE_WSL"/*.m4a "$STAGE_WSL"/garbage.bin "$STAGE_WSL"/u1music/* 2>/dev/null | sed 's|/mnt/c/Users/Public/qianqian-dogfood/|    |;s/^/  /'
  echo "windows_caption: $(powershell.exe -NoProfile -Command '(Get-CimInstance Win32_OperatingSystem).Caption' | tr -d '\r')"
  echo "audio_device: $(powershell.exe -NoProfile -Command '(Get-CimInstance Win32_SoundDevice | Select-Object -First 1).Name' | tr -d '\r')"
  echo "endpoint_note: default render endpoint, shared mode, event-driven (as opened by the wasapi stderr line in each transcript)"
} > "$ENVFILE"

powershell.exe -NoProfile -Command "
    \$p = Start-Process -FilePath 'C:\\Users\\Public\\qianqian-dogfood\\tuidriver.exe' \`
        -ArgumentList '--exe','C:\\Users\\Public\\qianqian-dogfood\\$BIN', \`
            '--media','$MEDIA','--out','$OUT',$PS_SCEN \`
        -WorkingDirectory 'C:\\Users\\Public\\qianqian-dogfood' \`
        -WindowStyle Hidden -Wait -PassThru
    exit \$p.ExitCode
"
DRIVER_RC=$?
# No stdout redirection through Start-Process: a redirected child shares
# the parent (interop) console, which defeats the ConPTY detachment. The
# driver writes its own summary into the evidence directory instead.
cat "$STAGE_WSL/evidence/summary.txt" 2>/dev/null
echo "driver exit: $DRIVER_RC"
cp "$STAGE_WSL/evidence/summary.txt" "$LOGDIR/tui-run$RUN.summary" 2>/dev/null || true
exit "$DRIVER_RC"
