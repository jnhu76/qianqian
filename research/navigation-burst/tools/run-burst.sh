#!/usr/bin/env bash
# Navigation-burst campaign runner (research/navigation-burst-boundary-0),
# modeled on research/transport-dogfood/tools/run-tui.sh: cross-build the
# product + driver, stage on the real Windows host, launch the ConPTY
# driver DETACHED with QIANQIAN_AUDIO_LOG=1 (the existing off-by-default
# mechanism log: one `[qianqian-wasapi] opened:` line per episode output
# open), run the B-* burst scenarios, and copy the evidence back.
#
# Usage: tools/run-burst.sh <run-number> SCENARIO [SCENARIO...]
set -uo pipefail

RUN="${1:?usage: run-burst.sh <run-number> SCENARIO...}"
shift
REPO="$(cd "$(dirname "$0")/../../.." && pwd)"
HARNESS="$REPO/research/transport-dogfood"
OUTDIR="$REPO/research/navigation-burst/evidence"
STAGE_WSL="/mnt/c/Users/Public/qianqian-dogfood"
EXE="$HARNESS/target/x86_64-pc-windows-gnu/release/tuidriver.exe"
BIN="qianqian-headless.exe"
PRODUCT="$REPO/target/x86_64-pc-windows-gnu/release/$BIN"
MEDIA="C:\\Users\\Public\\qianqian-dogfood"
OUT="C:\\Users\\Public\\qianqian-dogfood\\evidence"

mkdir -p "$OUTDIR"
taskkill.exe /F /IM qianqian-headless.exe /T >/dev/null 2>&1 || true
taskkill.exe /F /IM tuidriver.exe /T >/dev/null 2>&1 || true
sleep 1
cp "$EXE" "$STAGE_WSL/tuidriver.exe"
cp "$PRODUCT" "$STAGE_WSL/$BIN"
# The staging write can silently race a leftover process holding the
# staged exe; a stale binary then runs OLD scenarios. Verify the bytes
# actually landed before launching anything.
if [ "$(sha256sum "$EXE" | cut -d' ' -f1)" != "$(sha256sum "$STAGE_WSL/tuidriver.exe" | cut -d' ' -f1)" ] \
   || [ "$(sha256sum "$PRODUCT" | cut -d' ' -f1)" != "$(sha256sum "$STAGE_WSL/$BIN" | cut -d' ' -f1)" ]; then
  echo "FATAL: staged binaries do not match the fresh builds" >&2
  exit 3
fi

# The 5 s natural-EOF first track (B-eof-natural / B-manual-short):
# locally generated synthetic sine, like the harness's other declared
# synthetic media.
if [ ! -f "$STAGE_WSL/synth5.mp3" ]; then
  ffmpeg -v error -f lavfi -i "sine=frequency=440:sample_rate=44100:duration=5" \
      -ac 2 -b:a 128k -y "$STAGE_WSL/synth5.mp3"
fi

SCEN="$*"
PS_SCEN=$(printf "'%s'," $SCEN | sed 's/,$//')

# ROUND semantics: QIANQIAN_AUDIO_LOG=1 (default here) records the
# per-episode `[qianqian-wasapi] opened:` markers for TIMING, at the
# cost of stderr/VT interleaving that pollutes grid rows. Set
# BURST_AUDIO_LOG=0 for a count round: the grid stays clean and the
# Track-row value changes count committed replacements exactly.
AUDIO_LOG="${BURST_AUDIO_LOG:-1}"

ENVFILE="$OUTDIR/ENV-BURST-RUN${RUN}.txt"
{
  echo "run: BURST-RUN$RUN"
  echo "campaign: research/navigation-burst-boundary-0"
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
  echo "driver_env: QIANQIAN_AUDIO_LOG=$AUDIO_LOG (inherited by the child; the off-by-default per-episode mechanism lines)"
  echo "synth5_sha256: $(sha256sum "$STAGE_WSL/synth5.mp3" | cut -d' ' -f1)"
  echo "rustc: $(rustc --version)"
  echo "windows_caption: $(powershell.exe -NoProfile -Command '(Get-CimInstance Win32_OperatingSystem).Caption' | tr -d '\r')"
  echo "audio_device: $(powershell.exe -NoProfile -Command '(Get-CimInstance Win32_SoundDevice | Select-Object -First 1).Name' | tr -d '\r')"
} > "$ENVFILE"

powershell.exe -NoProfile -Command "
    if ('$AUDIO_LOG' -ne '0') { \$env:QIANQIAN_AUDIO_LOG = '$AUDIO_LOG' }
    \$p = Start-Process -FilePath 'C:\\Users\\Public\\qianqian-dogfood\\tuidriver.exe' \`
        -ArgumentList '--exe','C:\\Users\\Public\\qianqian-dogfood\\$BIN', \`
            '--media','$MEDIA','--out','$OUT',$PS_SCEN \`
        -WorkingDirectory 'C:\\Users\\Public\\qianqian-dogfood' \`
        -WindowStyle Hidden -Wait -PassThru
    exit \$p.ExitCode
"
DRIVER_RC=$?
cat "$STAGE_WSL/evidence/summary.txt" 2>/dev/null
echo "driver exit: $DRIVER_RC"
cp "$STAGE_WSL/evidence/summary.txt" "$OUTDIR/burst-run$RUN.summary" 2>/dev/null || true
# Copy the per-scenario evidence back under the round number: verdict
# JSON, frame transcript, timestamped markers, raw VT stream.
for s in "$@"; do
  for ext in json txt markers.txt raw.txt; do
    cp "$STAGE_WSL/evidence/$s.$ext" "$OUTDIR/run$RUN-$s.$ext" 2>/dev/null || true
  done
done
exit "$DRIVER_RC"
