#!/usr/bin/env bash
# run-lr1.sh — WINDOWS-TUI-LISTENING-RELEASE-1 isolated-package dogfood
# (from WSL, driving the real Windows host).
#
# HONEST LABEL: ISOLATED-PACKAGE DOGFOOD on the SAME physical host as
# the development tree. It is not a clean machine / fresh VM: the host
# has the repository, the toolchains and prior dogfood state. What IS
# isolated: every product launch below runs the EXTRACTED package exe
# (never the repo target/ tree), with PATH reduced to normal Windows
# system paths, no QIANQIAN_NATIVE_DIR, no repository CWD, and staging
# directories the build never referenced.
#
# Quoting convention (used everywhere below): Windows paths are built
# in bash double-quoted strings where "\\" is one literal backslash and
# $VARS expand; PowerShell's own variables are written "\$".
#
# Usage: research/listening-release/tools/run-lr1.sh [RUN-NUMBER]

set -uo pipefail

RUN="${1:-1}"
REPO="$(cd "$(dirname "$0")/../../.." && pwd)"
HARNESS="$REPO/research/transport-dogfood"
CAMP="$REPO/research/listening-release"
LOGDIR="$CAMP/evidence/logs"
ENVFILE="$CAMP/evidence/ENV-LR1-RUN${RUN}.txt"

ZIP="$REPO/dist/qianqian-windows-x86_64.zip"
DRIVER_EXE="$HARNESS/target/x86_64-pc-windows-gnu/release/tuidriver.exe"

# Windows-side locations (literal backslash paths).
W_ROOT='C:\Users\Public\qn-lr1'
W_SPACES_ROOT='C:\qn lr1 spaces'
W_CJK_ROOT='C:\Users\Public\千千播放器'
PKG_DIR="qianqian-windows-x86_64"
W_PKG="$W_ROOT\\$PKG_DIR"          # bash double quotes: \\ -> \
W_PKG_SPACES="$W_SPACES_ROOT\\$PKG_DIR"
W_PKG_CJK="$W_CJK_ROOT\\$PKG_DIR"
W_PKG_ZIP="$W_ROOT\\package.zip"
W_MEDIA="$W_ROOT\\media"
W_OUT="$W_ROOT\\evidence"
W_DRIVER="$W_ROOT\\tuidriver.exe"

# WSL-side mirrors.
STAGE_WSL="/mnt/c/Users/Public/qn-lr1"
SPACES_WSL="/mnt/c/qn lr1 spaces"
CJK_WSL="/mnt/c/Users/Public/千千播放器"
PKG_WSL="$STAGE_WSL/$PKG_DIR"

FIX="$REPO/native/experiments/songcore-equivalence/fixtures"
LONGNAME="qianqian-longpath-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.flac"

fail() { echo "run-lr1: FAIL: $*" >&2; exit 1; }
note() { echo "run-lr1: $*"; }

[[ -f "$ZIP" ]] || fail "package ZIP missing: $ZIP (run tools/package-windows.sh first)"
[[ -f "$DRIVER_EXE" ]] || fail "tuidriver.exe missing (build research/transport-dogfood for x86_64-pc-windows-gnu)"

note "stage 0: killing leftovers"
taskkill.exe /F /IM qianqian.exe /T >/dev/null 2>&1 || true
taskkill.exe /F /IM tuidriver.exe /T >/dev/null 2>&1 || true
sleep 1

rm -rf "$STAGE_WSL" "$SPACES_WSL" "$CJK_WSL"
mkdir -p "$STAGE_WSL" "$SPACES_WSL" "$CJK_WSL" "$LOGDIR" "$CAMP/evidence"

# --- stage 0: environment record ---------------------------------------
ZIP_SHA="$(sha256sum "$ZIP" | awk '{print $1}')"
{
  echo "run: LR1-RUN$RUN"
  echo "date_utc: $(date -u +%FT%TZ)"
  echo "branch: $(git -C "$REPO" branch --show-current)"
  echo "head_sha: $(git -C "$REPO" rev-parse HEAD)"
  echo "origin_main: $(git -C "$REPO" rev-parse origin/main)"
  echo "worktree_status: $(git -C "$REPO" status --porcelain | wc -l) dirty entries"
  echo "dogfood_class: ISOLATED-PACKAGE DOGFOOD (same physical host; extracted-package launches, system-only PATH, no repo CWD, no QIANQIAN_NATIVE_DIR)"
  echo "package_zip_sha256: $ZIP_SHA"
  echo "windows_caption: $(powershell.exe -NoProfile -Command '(Get-CimInstance Win32_OperatingSystem).Caption' | tr -d '\r')"
  echo "audio_device: $(powershell.exe -NoProfile -Command '(Get-CimInstance Win32_SoundDevice | Select-Object -First 1).Name' | tr -d '\r')"
} > "$ENVFILE"

# --- stage 1: extract the package on Windows (3 locations) -------------
note "stage 1: extracting the ZIP (plain / spaces / CJK locations)"
cp "$ZIP" "$STAGE_WSL/package.zip"
powershell.exe -NoProfile -Command "
  foreach (\$d in '$W_ROOT', '$W_SPACES_ROOT', '$W_CJK_ROOT') {
    Expand-Archive -Path '$W_PKG_ZIP' -DestinationPath \$d -Force
  }
  exit 0
" || fail "Expand-Archive failed"
for base in "$PKG_WSL" "$CJK_WSL/$PKG_DIR" "$SPACES_WSL/$PKG_DIR"; do
  for f in qianqian.exe QUICKSTART.md LICENSE THIRD_PARTY_NOTICES.md BUILD-MANIFEST.txt; do
    [[ -f "$base/$f" ]] || fail "extracted package missing $f under $base"
  done
done
PKG_EXE_SHA="$(sha256sum "$PKG_WSL/qianqian.exe" | awk '{print $1}')"
cp "$PKG_WSL/BUILD-MANIFEST.txt" "$CAMP/evidence/BUILD-MANIFEST-LR1-RUN$RUN.txt"
{
  echo "package_exe_sha256: $PKG_EXE_SHA"
  echo "extracted_locations: plain '$W_PKG' | spaces '$W_PKG_SPACES' | CJK '$W_PKG_CJK'"
} >> "$ENVFILE"

# --- stage 2: non-TUI isolated gates ------------------------------------
# cwd = C:\Windows\Temp, PATH = system only, no repo in sight.
note "stage 2: --version/--help from each location (cwd=Temp, system PATH)"
{
  echo
  echo "non_tui_isolated_gates (cwd=C:\\Windows\\Temp; PATH system-only; no QIANQIAN_NATIVE_DIR):"
  for loc in "$W_PKG" "$W_PKG_SPACES" "$W_PKG_CJK"; do
    for arg in --version --help; do
      powershell.exe -NoProfile -Command "
        \$env:PATH = 'C:\Windows\System32;C:\Windows'
        Set-Location 'C:\Windows\Temp'
        \$p = Start-Process -FilePath '$loc\\qianqian.exe' -ArgumentList '$arg' \
          -NoNewWindow -Wait -PassThru \
          -RedirectStandardOutput '$W_ROOT\\cli.out'
        exit \$p.ExitCode
      " >/dev/null 2>&1
      rc=$?
      first="$(head -1 "$STAGE_WSL/cli.out" 2>/dev/null | tr -d '\r')"
      echo "  [$loc] qianqian.exe $arg -> exit=$rc first='$first'"
      [[ $rc -eq 0 ]] || fail "isolated gate failed at $loc ($arg, exit $rc)"
    done
  done
} | tee -a "$ENVFILE"

# --- stage 3: new-console allocation (double-click equivalent) ----------
note "stage 3: fresh-console allocation at the CJK location"
alloc="$(powershell.exe -NoProfile -Command "
  \$env:PATH = 'C:\Windows\System32;C:\Windows'
  \$p = Start-Process -FilePath '$W_PKG_CJK\\qianqian.exe' \
    -WorkingDirectory '$W_PKG_CJK' -WindowStyle Hidden -PassThru
  Start-Sleep -Seconds 4
  \$alive = -not \$p.HasExited
  if (\$alive) { Stop-Process -Id \$p.Id -Force }
  if (\$alive) { 'ALIVE_4S' } else { 'EXITED code=' + \$p.ExitCode }
" 2>/dev/null | tr -d '\r')"
echo "new_console_allocation_cjk_location: $alloc" | tee -a "$ENVFILE"
[[ "$alloc" == *ALIVE_4S* ]] || fail "package exe did not stay alive in a fresh console at the CJK path ($alloc)"
taskkill.exe /F /IM qianqian.exe /T >/dev/null 2>&1 || true

# --- stage 4a: media corpus ---------------------------------------------
note "stage 4a: staging the media corpus"
M="$STAGE_WSL/media"
mkdir -p "$M"
cp "$FIX/flac-16-44-stereo.flac" "$M/flac4.flac"
cp "$FIX/mp3-cbr-id3v23.mp3"     "$M/mp3cbr.mp3"
cp "$FIX/alac-16-44-stereo.m4a"  "$M/alac4.m4a"
cp "$FIX/flac-16-44-stereo.flac" "$M/千曲.flac"
cp "$FIX/flac-16-44-stereo.flac" "$M/$LONGNAME"
printf 'deterministic invalid candidate' > "$M/garbage.bin"
mkdir -p "$M/u1music"
cp "$M/flac4.flac" "$M/u1music/flac4.flac"
cp "$M/mp3cbr.mp3" "$M/u1music/synth45.mp3"
ffmpeg -v error -f lavfi -i 'sine=frequency=440:sample_rate=44100:duration=45' \
    -ac 2 -b:a 128k -y "$M/synth45.mp3" || fail "synth45 generation"
ffmpeg -v error -f lavfi -i 'sine=frequency=330:sample_rate=44100:duration=30' \
    -ac 2 -y "$M/synth30.flac" || fail "synth30 generation"
# The U2-cover-clean fixture: a committed SYNTHETIC MP3 with an
# embedded mjpeg cover art (the field-defect shape, U2 corrective).
COVERFIX="$REPO/apps/headless/tests/fixtures/mp3-cbr-cover.mp3"
[[ -f "$COVERFIX" ]] || fail "cover-art fixture missing: $COVERFIX"
cp "$COVERFIX" "$M/cover.mp3"

# F-matrix folder (WINDOWS-TUI-LISTENING-RELEASE-1 §37 shapes):
# 5 accepted / 5 quiet skips / 2 probe rejections / 1 denied subfolder.
FM="$M/fmatrix"
mkdir -p "$FM/nested" "$FM/locked-away"
cp "$FIX/flac-16-44-stereo.flac" "$FM/01-track.flac"
cp "$FIX/alac-16-44-stereo.m4a"  "$FM/02-track.m4a"
cp "$FIX/flac-16-44-stereo.flac" "$FM/nested/03-deep.flac"
cp "$FIX/flac-16-44-stereo.flac" "$FM/千曲 deep.flac"
cp "$FIX/flac-16-44-stereo.flac" "$FM/$LONGNAME"
printf 'jpg-ish noise'     > "$FM/cover.jpg"
printf 'png-ish noise'     > "$FM/folder.png"
printf '[00:00.00] lyric'  > "$FM/lyric.lrc"
printf 'notes'             > "$FM/notes.txt"
printf '[.ShellClassInfo]' > "$FM/desktop.ini"
printf 'this is definitely not a flac stream, just text bytes' > "$FM/broken.flac"
: > "$FM/zero.mp3"
icacls_out="$(powershell.exe -NoProfile -Command "icacls '$W_MEDIA\\fmatrix\\locked-away' /deny 'Everyone:(OI)(CI)R'" 2>&1 | tr -d '\r' | tail -1)"
echo "f5_deny_icacls: $icacls_out" >> "$ENVFILE"

mkdir -p "$M/corrupt"
printf 'garbage not audio' > "$M/corrupt/a.flac"
printf 'garbage not audio' > "$M/corrupt/b.mp3"

mkdir -p "$M/dup"
cp "$FIX/flac-16-44-stereo.flac" "$M/dup/one.flac"
cp "$FIX/mp3-cbr-id3v23.mp3"     "$M/dup/two.mp3"

mkdir -p "$M/trunc"
cp "$FIX/flac-16-44-stereo.flac" "$M/trunc/01-good.flac"
head -c 60000 "$FIX/flac-16-44-stereo.flac" > "$M/trunc/02-trunc.flac"
echo "trunc_fixture_bytes: $(stat -c%s "$M/trunc/02-trunc.flac") of $(stat -c%s "$FIX/flac-16-44-stereo.flac")" >> "$ENVFILE"

note "stage 4a: building the 1,000- and 5,000-entry lists"
B1="$M/big1000"; mkdir -p "$B1"
for i in $(seq -w 1 1000); do cp "$FIX/mp3-cbr-id3v23.mp3" "$B1/track-$i.mp3"; done
B5="$M/big5000"; mkdir -p "$B5"
# NTFS caps hard links at 1023 per file, so the 5,000 entries share TEN
# seed files (500 links each) — path-sorted order and byte-identity are
# unaffected (track-NNNN names never collide across seeds).
for s in $(seq 0 9); do cp "$FIX/mp3-cbr-id3v23.mp3" "$B5/seed$s.mp3"; done
powershell.exe -NoProfile -Command "
  \$dir = '$W_MEDIA\\big5000'
  for (\$i = 1; \$i -le 5000; \$i++) {
    \$seed = '{0}\\seed{1}.mp3' -f \$dir, ([int][math]::Floor((\$i - 1) / 500))
    New-Item -ItemType HardLink -Path ('{0}\\track-{1:d4}.mp3' -f \$dir, \$i) -Target \$seed | Out-Null
  }
" || fail "hardlink staging failed"
rm -f "$B5"/seed*.mp3
echo "corpus_counts: big1000=$(ls "$B1" | wc -l) big5000=$(ls "$B5" | wc -l)" >> "$ENVFILE"

{
  echo "corpus_sha256 (fixtures are committed repo media; synth* are locally"
  echo "  generated sine tracks — declared SYNTHETIC; noise/garbage/trunc"
  echo "  are deliberate failure shapes, not media):"
  ( cd "$M" && find . -type f \( -name '*.flac' -o -name '*.mp3' -o -name '*.m4a' \) \
      | LC_ALL=C sort | head -60 \
      | while read -r f; do sha256sum "$f"; done \
      | sed 's|/mnt/c/Users/Public/qn-lr1/media|    |;s/\./  /;s/^/  /' )
  echo "  (big1000/big5000 entries are byte-identical copies/hardlinks of mp3cbr.mp3 — one hash represents them)"
  echo "  mp3cbr.mp3 again: $(sha256sum "$FIX/mp3-cbr-id3v23.mp3" | cut -d' ' -f1)"
} >> "$ENVFILE"

cp "$DRIVER_EXE" "$STAGE_WSL/tuidriver.exe"

# --- stage 4b: scenarios against the EXTRACTED package exe --------------
# The ConPTY child gets --cwd $W_MEDIA so relative O-dialog candidates
# (u1music, missing-file.flac) resolve against the MEDIA corpus — the
# package exe itself lives in its own extracted folder.
run_scenarios() {
  local label="$1"; shift
  local ps_scen
  ps_scen=$(printf "'%s'," "$@" | sed 's/,$//')
  note "stage 4b [$label]: $*"
  local t0 t1 rc
  t0=$(date +%s)
  powershell.exe -NoProfile -Command "
    \$env:PATH = 'C:\Windows\System32;C:\Windows'
    \$p = Start-Process -FilePath '$W_DRIVER' \
      -ArgumentList '--exe','$W_PKG\\qianqian.exe','--media','$W_MEDIA','--out','$W_OUT','--cwd','$W_MEDIA',$ps_scen \
      -WorkingDirectory '$W_ROOT' -WindowStyle Hidden -Wait -PassThru
    exit \$p.ExitCode
  "
  rc=$?
  t1=$(date +%s)
  echo "wall_seconds_${label}: $((t1 - t0)) (scenarios: $*)" | tee -a "$ENVFILE"
  cp "$STAGE_WSL/evidence/summary.txt" "$LOGDIR/lr1-run${RUN}-${label}.summary" 2>/dev/null || true
  if [[ $rc -ne 0 ]]; then
    note "  [$label] driver exit $rc — RED (see lr1-run${RUN}-${label}.summary)"
    return 1
  fi
  local green
  green="$(grep -c GREEN "$STAGE_WSL/evidence/summary.txt" 2>/dev/null || true)"
  note "  [$label] ${green:-?} GREEN"
  return 0
}

ok=0
run_scenarios core   U1-idle U1-folder-open U2-shuffle-start U2-cover-clean U2-help C11-longpath C12-cjk A15 \
  && ok=$((ok+1)) || true
run_scenarios fmatrix LR1-folder-mixed LR1-all-corrupt LR1-duplicate-roots LR1-truncated-next \
  && ok=$((ok+1)) || true
run_scenarios large  LR1-large \
  && ok=$((ok+1)) || true
run_scenarios huge   LR1-huge \
  && ok=$((ok+1)) || true

# --- cleanup: undo the ACL denial so the tree stays removable -----------
powershell.exe -NoProfile -Command "icacls '$W_MEDIA\\fmatrix\\locked-away' /remove:d Everyone; icacls '$W_MEDIA\\fmatrix\\locked-away' /reset" >/dev/null 2>&1 || true
taskkill.exe /F /IM qianqian.exe /T >/dev/null 2>&1 || true

# Full per-scenario evidence (summaries, transcripts, raw captures).
cp -r "$STAGE_WSL/evidence" "$CAMP/evidence/transcripts-run$RUN" 2>/dev/null || true

echo "scenario_groups_green: $ok/4" | tee -a "$ENVFILE"
[[ $ok -eq 4 ]] || fail "one or more scenario groups were RED"
note "OK — evidence in $CAMP/evidence/ and $LOGDIR/"
