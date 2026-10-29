# build-rc.ps1 -- Windows release-lane RC orchestrator for the SongCore
# windows-x86_64 static artifact (#172 S2).
#
# Single entry: environment preflight -> pin the source SHA -> run the
# windows-x86_64 recipe route inside WSL (Ubuntu-24.04 distro, xmake v2.9.7
# route, mingw cross session) -> static archive -> PE/COFF target identity
# check -> independently computed and cross-checked SHA256 (WSL sha256sum vs
# Windows Get-FileHash; artifact AND ABI header) -> one machine-readable JSON
# summary on stdout (< 64KB). Detailed logs land under
# evidence/songcore-172/build/rc/<stamp>/.
#
# FFmpeg closure discipline: this lane NEVER downloads or re-imports FFmpeg.
# It reuses an already-imported closure (the D2 route product: pinned source
# tree with .qianqian-verified marker + closure manifest + generated oracle
# headers), resolved in this order: (1) the persistent build root's placed
# closure (validated against pin.json on every run), (2) the work-root cache,
# (3) -ClosureSource (a complete imported closure copied in -- a copy, not an
# import). Missing closure is a FAIL with instructions, never a silent
# import. xmake's fail-closed identity gate (native/build/ffmpeg.lua)
# re-validates the manifest against the session target on every build.
#
# Re-entry safety: the WSL work root persists across runs. Tracked source
# subtrees are refreshed from the worktree on every run (deletions
# propagate); the xmake object cache and the placed closure survive, so an
# interrupted run resumes incrementally. `xmake build songcore` runs without
# -r for this reason; the first run in a fresh root compiles the full replay.
#
# Second root (reproducibility input): with -BuildDir <dir> the run also
# builds the same source SHA in a SECOND independent absolute root: fresh
# tree, the same validated imported closure copied in (no re-import), full
# build in that root. Both artifacts are hashed and reported; the byte
# comparison/classification itself is contract section 12 T1-T6 (S3) work and is
# NOT judged here. The second root's artifact is NOT copied back into the
# worktree consumption path -- only into the run's evidence dir.
#
# Exit codes are propagated verbatim: bash stages run `set -euo pipefail`
# and this script aborts on any nonzero $LASTEXITCODE (stage exit codes:
# 10 missing tool, 11 closure unavailable/mismatch, 12 artifact missing,
# 13 shared artifact present, 14 member-format failure, 15 ABI symbol
# count, 16 negative control passed, 17 provenance crosscheck failure).
#
# Usage (from any shell):
#   powershell -ExecutionPolicy Bypass -File ops\build-rc.ps1
#   powershell -ExecutionPolicy Bypass -File ops\build-rc.ps1 -BuildDir /home/hoo/rc-r2
#   ... -ClosureSource /home/hoo/q172d2-verify/closure   (WSL path; C:\ paths
#       are accepted and converted)
# NOTE (Git Bash/MSYS callers): MSYS rewrites POSIX-looking arguments to the
# Git installation directory before a native exe sees them (observed live:
# /home/hoo/... became C:\Program Files\Git\home\hoo\...). Invoke with
#   MSYS_NO_PATHCONV=1 powershell -File ops/build-rc.ps1 ...
# (or MSYS2_ARG_CONV_EXCL='*') so WSL-side paths survive; a run from
# cmd/PowerShell is unaffected. A mangled path fails closed at closure
# validation, never silently.
param(
    [string]$ProjectRoot = "",
    [string]$Distro = "Ubuntu-24.04",
    [string]$WslWorkRoot = "/home/hoo/q172rc",
    [string]$BuildDir = "",
    [string]$ClosureSource = "",
    # The route this lane productizes is the D2-proven xmake v2.9.7 route.
    # A different xmake is a route change: override this param only together
    # with fresh route evidence, never silently.
    [string]$ExpectedXmakeVersion = "2.9.7",
    # Frozen ABI v1 header identity (docs/release/songcore-release-contract.md
    # section 3, == sha256(native/include/songcore.h) since v0.1.0). Drift fails the
    # lane; the constant moves only with an authority reference.
    [string]$ExpectedAbiHeaderSha256 = "cf509a5c044735e4813642ac6683691242146e642342a8ac356e98fbd8e78536"
)

$ErrorActionPreference = "Stop"
if ($ProjectRoot -eq "") { $ProjectRoot = Split-Path -Parent $PSScriptRoot }
$ProjectRoot = (Resolve-Path $ProjectRoot).Path

$stamp = Get-Date -Format "yyyyMMdd-HHmmss"
$evDir = Join-Path $ProjectRoot "evidence\songcore-172\build\rc\$stamp"
New-Item -ItemType Directory -Force -Path $evDir | Out-Null

$utf8 = New-Object System.Text.UTF8Encoding($false)
function Write-LFText([string]$path, [string]$text) {
    [System.IO.File]::WriteAllText($path, $text.Replace("`r`n", "`n"), $utf8)
}

# Accept both C:\... and /mnt|/home WSL forms; pass WSL form to the stages.
function Convert-ToWslPath([string]$p) {
    if ($p -match '^([A-Za-z]):[\\/](.*)$') {
        $drive = $Matches[1].ToLowerInvariant()
        $rest = $Matches[2] -replace '\\', '/'
        return "/mnt/$drive/$rest"
    }
    return $p
}

function Fail-Param([string]$msg) { throw "build-rc.ps1: $msg" }

$buildDirWsl = ""
if ($BuildDir -ne "") {
    $buildDirWsl = Convert-ToWslPath $BuildDir
    if (-not $buildDirWsl.StartsWith("/")) {
        Fail-Param "BuildDir must resolve to an absolute WSL path (got '$buildDirWsl')"
    }
    if ($buildDirWsl -eq $WslWorkRoot) {
        Fail-Param "BuildDir must differ from -WslWorkRoot ('$WslWorkRoot')"
    }
}
$closureSourceWsl = ""
if ($ClosureSource -ne "") { $closureSourceWsl = Convert-ToWslPath $ClosureSource }

$summary = [ordered]@{
    script      = "ops/build-rc.ps1"
    started     = $stamp
    project     = $ProjectRoot
    wsl_distro  = $Distro
    wsl_work_root = $WslWorkRoot
    build_dir   = $buildDirWsl
    closure_source = $closureSourceWsl
    git_branch  = ""
    source      = [ordered]@{}
    steps       = [ordered]@{}
    artifacts   = [ordered]@{}
    ffmpeg_provenance = [ordered]@{}
    toolchain   = [ordered]@{}
    second_root = [ordered]@{}
    ok          = $false
}

function Save-Summary {
    $json = $summary | ConvertTo-Json -Depth 6
    Write-LFText (Join-Path $evDir "summary.json") $json
    Write-Output $json
}

function Fail([string]$step, [string]$detail) {
    $summary.steps[$step] = "FAIL: $detail"
    Save-Summary
    exit 1
}

function Invoke-WslStage([string]$scriptName, [string]$stepName) {
    # Runs a bash stage file inside the distro; output is captured to the
    # evidence dir (stdout stays reserved for the final JSON summary) and
    # the child exit code is propagated verbatim.
    $winPath = Join-Path $evDir $scriptName
    $wslPath = Convert-ToWslPath $winPath
    $out = & wsl.exe -d $Distro -- bash $wslPath 2>&1
    $rc = $LASTEXITCODE
    $text = ($out | ForEach-Object { $_.ToString() }) -join "`n"
    Write-LFText (Join-Path $evDir ($stepName + ".log")) $text
    if ($rc -ne 0) {
        Fail $stepName ("wsl stage exited $rc; see evidence log " + $stepName + ".log")
    }
    $summary.steps[$stepName] = "ok (exit 0)"
    return $out
}

# ---------------------------------------------------------------- preflight
foreach ($f in @("native\xmake.lua", "native\build\songcore.lua",
                 "native\build\ffmpeg.lua", "native\ffmpeg\targets\windows-x86_64.json",
                 "native\ffmpeg\pin.json", "native\ffmpeg\profiles\codec-base.json",
                 "native\include\songcore.h", "native\src\songcore_ffmpeg.c")) {
    if (-not (Test-Path (Join-Path $ProjectRoot $f))) {
        Fail "preflight" "missing worktree file: $f"
    }
}
$gitHead = & git -C $ProjectRoot rev-parse HEAD 2>&1
if ($LASTEXITCODE -ne 0) { Fail "preflight" "git rev-parse failed: $gitHead" }
$gitBranch = & git -C $ProjectRoot rev-parse --abbrev-ref HEAD 2>&1
$summary.git_branch = $gitBranch
$summary.source.commit = $gitHead

# Pin the source SHA by refusing uncommitted tracked changes under native/
# (the build input must be attributable to the SHA above; untracked files
# such as evidence are harmless and simply recorded).
$dirty = & git -C $ProjectRoot status --porcelain -- native/ 2>&1
if ($LASTEXITCODE -ne 0) { Fail "preflight" "git status failed: $dirty" }
$dirtyTracked = @($dirty | Where-Object { $_ -notmatch '^\?\?' })
if ($dirtyTracked.Count -gt 0) {
    Fail "preflight" ("native/ has uncommitted tracked changes; artifact would " +
        "not be attributable to the pinned source SHA: " + ($dirtyTracked -join "; "))
}
$summary.source.native_tracked_clean = $true

# Recipe + pin + header identities (Windows-side independent computation).
$recipeFile = Join-Path $ProjectRoot "native\ffmpeg\targets\windows-x86_64.json"
$recipeSha = (Get-FileHash -Algorithm SHA256 $recipeFile).Hash.ToLowerInvariant()
$summary.source.recipe_sha256 = $recipeSha
$headerFile = Join-Path $ProjectRoot "native\include\songcore.h"
$headerShaWin = (Get-FileHash -Algorithm SHA256 $headerFile).Hash.ToLowerInvariant()
$summary.source.abi_header_sha256 = $headerShaWin
if ($headerShaWin -ne $ExpectedAbiHeaderSha256) {
    Fail "preflight" ("ABI header drift: sha256(native/include/songcore.h)=$headerShaWin " +
        "!= frozen identity $ExpectedAbiHeaderSha256. The artifact must not ship against " +
        "an unreviewed header change; update -ExpectedAbiHeaderSha256 only with an " +
        "authority reference (docs/release/songcore-release-contract.md section 3).")
}
$pin = Get-Content (Join-Path $ProjectRoot "native\ffmpeg\pin.json") -Raw | ConvertFrom-Json
$summary.ffmpeg_provenance.tag = $pin.ffmpeg_tag
$summary.ffmpeg_provenance.commit = $pin.ffmpeg_commit_sha
$summary.ffmpeg_provenance.source_sha256 = $pin.source_sha256

$null = & wsl.exe -d $Distro -- true 2>&1
if ($LASTEXITCODE -ne 0) { Fail "preflight" "WSL distro '$Distro' not runnable (exit $LASTEXITCODE)" }
$summary.steps["preflight"] = "ok"

# Environment variables for the WSL stages (converted with wslpath inside;
# no Windows paths travel through argv, so no quoting can be mangled).
$env:QR_WT           = $ProjectRoot
$env:QR_EV           = $evDir
$env:QR_ROOT         = $WslWorkRoot
$env:QR_BUILDDIR     = $buildDirWsl
$env:QR_CLOSURE_SRC  = $closureSourceWsl
$env:WSLENV          = "QR_WT:QR_EV:QR_ROOT:QR_BUILDDIR:QR_CLOSURE_SRC"

# --------------------------------------------------- stage: WSL tool checks
# Records the exact toolchain identity (compiler/archiver/linker/xmake) the
# artifact is built with; fails closed on a missing tool or an xmake outside
# the pinned route version.
$toolcheck = @'
set -euo pipefail
export PATH="$HOME/.local/bin:$PATH"
# xmake paints even piped output with ANSI escapes; strip them so the
# recorded identities and the pinned-version match stay byte-clean.
strip_ansi() { sed 's/\x1b\[[0-9;]*m//g'; }
for t in xmake python3 x86_64-w64-mingw32-gcc x86_64-w64-mingw32-ar \
         x86_64-w64-mingw32-ld x86_64-w64-mingw32-objdump x86_64-w64-mingw32-nm \
         tar sha256sum; do
    command -v "$t" > /dev/null || { echo "missing tool: $t"; exit 10; }
    echo "tool ok: $t -> $(command -v "$t")"
done
echo "TOOL_XMAKE=$(xmake --version | head -1 | strip_ansi)"
echo "TOOL_CC=$(x86_64-w64-mingw32-gcc --version | head -1 | strip_ansi)"
echo "TOOL_AR=$(x86_64-w64-mingw32-ar --version | head -1 | strip_ansi)"
echo "TOOL_LD=$(x86_64-w64-mingw32-ld --version | head -1 | strip_ansi)"
echo "TOOL_PYTHON=$(python3 --version 2>&1 | strip_ansi)"
echo "TOOLS_OK"
'@
Write-LFText (Join-Path $evDir "stage-tools.sh") $toolcheck
$toolsOut = Invoke-WslStage "stage-tools.sh" "wsl_tools"

$toolXmake = ""; $toolCc = ""; $toolAr = ""; $toolLd = ""; $toolPython = ""
foreach ($line in $toolsOut) {
    $s = $line.ToString()
    if ($s -match '^TOOL_XMAKE=(.*)$')   { $toolXmake = $Matches[1] }
    if ($s -match '^TOOL_CC=(.*)$')      { $toolCc    = $Matches[1] }
    if ($s -match '^TOOL_AR=(.*)$')      { $toolAr    = $Matches[1] }
    if ($s -match '^TOOL_LD=(.*)$')      { $toolLd    = $Matches[1] }
    if ($s -match '^TOOL_PYTHON=(.*)$')  { $toolPython = $Matches[1] }
}
if ($toolXmake -notmatch ("^xmake v" + [regex]::Escape($ExpectedXmakeVersion))) {
    Fail "wsl_tools" ("xmake '$toolXmake' is outside the pinned v$ExpectedXmakeVersion " +
        "route. The RC lane productizes the D2-proven route; a different xmake needs " +
        "its own route re-proof (override -ExpectedXmakeVersion only with that evidence).")
}
$summary.toolchain.xmake = $toolXmake
$summary.toolchain.cc = $toolCc
$summary.toolchain.ar = $toolAr
$summary.toolchain.ld = $toolLd
$summary.toolchain.python = $toolPython

# ------------------------------------------------- stage: prepare + closure
# Refresh tracked sources into the persistent build root (overlay; the xmake
# cache and the placed closure survive across runs), then resolve the
# imported FFmpeg closure WITHOUT importing: (1) the root's placed closure,
# (2) the work-root cache, (3) -ClosureSource (complete closure copied in).
# Every placement is validated against pin.json's commit|source digest
# marker; xmake's identity gate re-validates the manifest at build time.
$stagePrepare = @'
set -euo pipefail
export PATH="$HOME/.local/bin:$PATH"
WT="$(wslpath -u "$QR_WT")"
ROOT="$QR_ROOT"
W="$ROOT/win"

# Defense in depth for the rm/cp operations below: the work root must be a
# plain absolute WSL-local path.
case "$ROOT" in
  /home/*|/root/*|/tmp/*) : ;;
  *) echo "refusing suspicious work root: '$ROOT'"; exit 11 ;;
esac

mkdir -p "$ROOT" "$W"
# Refresh only git-controlled source subtrees; build/ (xmake cache +
# closure) persists across runs -> interrupted runs resume incrementally.
rm -rf "$W/native/src" "$W/native/include" "$W/native/ffmpeg" \
       "$W/native/tools" "$W/native/scripts" "$W/native/experiments" \
       "$W/native/xmake.lua" "$W/native/README.md" \
       "$W/native/build/ffmpeg.lua" "$W/native/build/songcore.lua"
mkdir -p "$W/native/build"
cd "$WT"
tar cf - \
    --exclude='native/build/artifacts' \
    --exclude='native/build/xmake' \
    --exclude='native/build/downloads' \
    --exclude='native/build/ffmpeg-xmake' \
    --exclude='native/build/ffmpeg-src' \
    native | tar xf - -C "$W"

cd "$W/native"
EXPECT=$(python3 -c "import json;p=json.load(open('ffmpeg/pin.json'));print(p['ffmpeg_commit_sha']+'|'+p['source_sha256'])")

check_marker() {
    local ACTUAL
    ACTUAL=$(cat build/ffmpeg-src/.qianqian-verified)
    if [ "$ACTUAL" != "$EXPECT" ]; then
        echo "closure pin mismatch: '$ACTUAL' != '$EXPECT'"
        exit 11
    fi
}
complete_closure() {
    test -f build/ffmpeg-xmake/manifest.json &&
    test -f build/ffmpeg-src/.qianqian-verified &&
    test -f build/ffmpeg-xmake/oracle/config_components.h
}
cache_valid() {
    test -f "$ROOT/closure/ffmpeg-xmake/manifest.json" &&
    test -f "$ROOT/closure/ffmpeg-src/.qianqian-verified" &&
    test -f "$ROOT/closure/ffmpeg-xmake/oracle/config_components.h"
}
source_closure_valid() {
    test -f "$1/ffmpeg-xmake/manifest.json" &&
    test -f "$1/ffmpeg-src/.qianqian-verified" &&
    test -f "$1/ffmpeg-xmake/oracle/config_components.h"
}
place_closure() {
    rm -rf build/ffmpeg-xmake build/ffmpeg-src
    cp -a "$1/ffmpeg-xmake" build/ffmpeg-xmake
    cp -a "$1/ffmpeg-src" build/ffmpeg-src
    check_marker
}

if complete_closure; then
    check_marker
    echo "closure: persistent build root (reused, pin-validated)"
elif cache_valid; then
    place_closure "$ROOT/closure"
    echo "closure: work-root cache"
elif [ -n "$QR_CLOSURE_SRC" ] && source_closure_valid "$QR_CLOSURE_SRC"; then
    place_closure "$QR_CLOSURE_SRC"
    echo "closure: copied from -ClosureSource (no re-import)"
else
    echo "FAIL: no complete imported FFmpeg closure available."
    echo "  The RC lane never downloads or re-imports FFmpeg."
    echo "  Pass -ClosureSource <wsl-path> pointing at a complete imported"
    echo "  closure (ffmpeg-src/ + ffmpeg-xmake/{manifest.json,oracle}/), or"
    echo "  run the D2 import once (ops/verify-build.ps1) first."
    exit 11
fi

# Keep the work-root cache in sync with the placed closure so later runs and
# the -BuildDir second root can reuse it without any import.
if complete_closure; then
    mkdir -p "$ROOT/closure"
    rm -rf "$ROOT/closure/ffmpeg-src" "$ROOT/closure/ffmpeg-xmake"
    cp -a build/ffmpeg-src "$ROOT/closure/ffmpeg-src"
    cp -a build/ffmpeg-xmake "$ROOT/closure/ffmpeg-xmake"
fi

test -f build/ffmpeg-xmake/manifest.json || { echo "no manifest after closure step"; exit 11; }
echo "PREPARE_OK"
'@
Write-LFText (Join-Path $evDir "stage-prepare.sh") $stagePrepare
$null = Invoke-WslStage "stage-prepare.sh" "closure_prepare"

# ------------------------------------------------ stage: build + validation
$stageBuild = @'
set -euo pipefail
export PATH="$HOME/.local/bin:$PATH"
WT="$(wslpath -u "$QR_WT")"
EV="$(wslpath -u "$QR_EV")"
ROOT="$QR_ROOT"
W="$ROOT/win"
cd "$W/native"

xmake f -p mingw -a x86_64 -m release -y > "$EV/config.log" 2>&1
xmake build songcore > "$EV/build.log" 2>&1

test -f build/artifacts/libsongcore.a || { echo "FAIL: libsongcore.a missing after build"; exit 12; }
test ! -e build/artifacts/shared || { echo "FAIL: shared/ artifact dir exists (recipe says shared=false)"; exit 13; }

# grep -c exits 1 on a zero count, which is a valid count here; suspend -e
# around the captures and assert the values explicitly below.
set +e
PE=$(x86_64-w64-mingw32-objdump -f build/artifacts/libsongcore.a 2>/dev/null | grep -c 'file format pe-x86-64')
ELF=$(x86_64-w64-mingw32-objdump -f build/artifacts/libsongcore.a 2>/dev/null | grep -c 'file format elf')
SONG=$(x86_64-w64-mingw32-nm build/artifacts/libsongcore.a 2>/dev/null | grep -c ' T song')
set -e
if [ "$PE" -le 0 ] || [ "$ELF" -ne 0 ]; then
    echo "FAIL: member formats pe=$PE elf=$ELF (expected pe>0, elf==0)"
    exit 14
fi
if [ "$SONG" -ne 15 ]; then
    echo "FAIL: song* ABI symbols = $SONG (expected 15)"
    exit 15
fi

# Negative control: the recipe does not enable the shared form, so an
# explicit build MUST fail closed. Exiting 0 here means the policy regressed.
if xmake build songcore_shared > "$EV/negative-shared.log" 2>&1; then
    echo "FAIL: explicit songcore_shared exited 0 (recipe policy broken)"
    exit 16
fi

# Provenance crosscheck: the closure manifest actually used must agree with
# pin.json, the profile file digest, and the recipe file digest (V3-style
# carried-duplicate byte-equality, contract section 8).
python3 -c "
import json, hashlib, sys
m = json.load(open('build/ffmpeg-xmake/manifest.json'))
p = json.load(open('ffmpeg/pin.json'))
errs = []
def sha(p_): return hashlib.sha256(open(p_, 'rb').read()).hexdigest()
if m['ffmpeg_tag'] != p['ffmpeg_tag']: errs.append('ffmpeg_tag')
if m['ffmpeg_commit_sha'] != p['ffmpeg_commit_sha']: errs.append('ffmpeg_commit_sha')
if m['ffmpeg_source_sha256'] != p['source_sha256']: errs.append('ffmpeg_source_sha256')
if m.get('profile_sha256') != sha('ffmpeg/profiles/' + m.get('profile', '') + '.json'): errs.append('profile_sha256')
mrs = (m.get('target') or {}).get('recipe_sha256', '')
if mrs != sha('ffmpeg/targets/windows-x86_64.json'): errs.append('recipe_sha256')
if errs:
    print('CLOSURE_CROSSCHECK=FAIL: ' + ','.join(errs)); sys.exit(17)
print('CLOSURE_CROSSCHECK=OK')
print('CLOSURE_UNITS=%d' % len(m['units']))
print('FFMPEG_TAG=' + m['ffmpeg_tag'])
print('FFMPEG_COMMIT=' + m['ffmpeg_commit_sha'])
print('FFMPEG_SOURCE_SHA256=' + m['ffmpeg_source_sha256'])
print('PROFILE=' + m.get('profile', ''))
print('PROFILE_SHA256=' + m.get('profile_sha256', ''))
print('RECIPE_SHA256_IN_MANIFEST=' + mrs)
print('CLOSURE_MANIFEST_SHA256=' + sha('build/ffmpeg-xmake/manifest.json'))
"
RSHA_LOCAL=$(sha256sum ffmpeg/targets/windows-x86_64.json | awk '{print $1}')
echo "RECIPE_SHA256_LOCAL=$RSHA_LOCAL"
HSUM=$(sha256sum "$WT/native/include/songcore.h" | awk '{print $1}')
echo "HEADER_SHA256=$HSUM"

# Copy the artifacts + closure manifest back into the worktree (the D2
# consumption step: downstream consumers find native/build/artifacts).
mkdir -p "$WT/native/build/artifacts" "$WT/native/build/ffmpeg-xmake"
cp build/artifacts/libsongcore.a "$WT/native/build/artifacts/libsongcore.a"
cp build/artifacts/libqianqian_av.a "$WT/native/build/artifacts/libqianqian_av.a"
cp build/ffmpeg-xmake/manifest.json "$WT/native/build/ffmpeg-xmake/manifest.json"

SHA=$(sha256sum "$WT/native/build/artifacts/libsongcore.a" | awk '{print $1}')
SIZE=$(stat -c '%s' "$WT/native/build/artifacts/libsongcore.a")
echo "PE_MEMBERS=$PE"
echo "ELF_MEMBERS=$ELF"
echo "SONG_ABI=$SONG"
echo "ARTIFACT_SHA256=$SHA"
echo "ARTIFACT_SIZE=$SIZE"
echo "BUILD_OK"
'@
Write-LFText (Join-Path $evDir "stage-build.sh") $stageBuild
$buildOut = Invoke-WslStage "stage-build.sh" "build_and_validate"

# ------------------------------------------------------------- R1 summarize
$peMembers = ""; $elfMembers = ""; $songAbi = ""; $sha = ""; $size = ""; $hsum = ""
$closureManifestSha = ""; $recipeShaInManifest = ""; $recipeShaLocal = ""
$closureUnits = ""; $ffTag = ""; $ffCommit = ""; $ffSourceSha = ""
$profileName = ""; $profileSha = ""
foreach ($line in $buildOut) {
    $s = $line.ToString()
    if ($s -match '^PE_MEMBERS=(\d+)$')                { $peMembers = $Matches[1] }
    if ($s -match '^ELF_MEMBERS=(\d+)$')               { $elfMembers = $Matches[1] }
    if ($s -match '^SONG_ABI=(\d+)$')                  { $songAbi   = $Matches[1] }
    if ($s -match '^ARTIFACT_SHA256=([0-9a-f]+)$')     { $sha       = $Matches[1] }
    if ($s -match '^ARTIFACT_SIZE=(\d+)$')             { $size      = $Matches[1] }
    if ($s -match '^HEADER_SHA256=([0-9a-f]+)$')       { $hsum      = $Matches[1] }
    if ($s -match '^CLOSURE_MANIFEST_SHA256=([0-9a-f]+)$') { $closureManifestSha = $Matches[1] }
    if ($s -match '^RECIPE_SHA256_IN_MANIFEST=([0-9a-f]+)$') { $recipeShaInManifest = $Matches[1] }
    if ($s -match '^RECIPE_SHA256_LOCAL=([0-9a-f]+)$') { $recipeShaLocal = $Matches[1] }
    if ($s -match '^CLOSURE_UNITS=(\d+)$')             { $closureUnits = $Matches[1] }
    if ($s -match '^FFMPEG_TAG=(.*)$')                 { $ffTag     = $Matches[1] }
    if ($s -match '^FFMPEG_COMMIT=([0-9a-f]+)$')       { $ffCommit  = $Matches[1] }
    if ($s -match '^FFMPEG_SOURCE_SHA256=([0-9a-f]+)$'){ $ffSourceSha = $Matches[1] }
    if ($s -match '^PROFILE=(.*)$')                    { $profileName = $Matches[1] }
    if ($s -match '^PROFILE_SHA256=([0-9a-f]+)$')      { $profileSha = $Matches[1] }
}
if ($sha -eq "") { Fail "summarize" "stage did not report ARTIFACT_SHA256" }

# Independent Windows-side SHA256 of the copied-back artifact and header,
# cross-checked against the WSL computation; mismatch is a hard failure.
$artifact = Join-Path $ProjectRoot "native\build\artifacts\libsongcore.a"
$shaWin = (Get-FileHash -Algorithm SHA256 $artifact).Hash.ToLowerInvariant()
if ($shaWin -ne $sha) {
    Fail "summarize" "artifact sha256 mismatch: WSL=$sha Windows=$shaWin (copy-back corruption?)"
}
$headerShaWin2 = (Get-FileHash -Algorithm SHA256 $headerFile).Hash.ToLowerInvariant()
if ($hsum -ne $headerShaWin2) {
    Fail "summarize" "header sha256 mismatch: WSL=$hsum Windows=$headerShaWin2"
}
if ($headerShaWin2 -ne $ExpectedAbiHeaderSha256) {
    Fail "summarize" "built-against header is not the frozen ABI v1 identity"
}
if ($recipeShaInManifest -ne $recipeShaLocal -or $recipeShaLocal -ne $recipeSha) {
    Fail "summarize" ("recipe sha disagreement: manifest=$recipeShaInManifest " +
        "wsl-local=$recipeShaLocal windows=$recipeSha")
}

$magic = [System.IO.File]::ReadAllBytes($artifact)[0..7]
$magicOk = (($magic[0] -eq 0x21) -and ($magic[1] -eq 0x3C) -and ($magic[2] -eq 0x61) -and
            ($magic[3] -eq 0x72) -and ($magic[4] -eq 0x63) -and ($magic[5] -eq 0x68) -and
            ($magic[6] -eq 0x3E) -and ($magic[7] -eq 0x0A))
if (-not $magicOk) { Fail "summarize" "copied-back artifact is not an ar archive (!<arch>)" }

# Closure-manifest toolchain block (the identity the closure was imported
# with), read from the manifest actually used and copied back.
$manifestPath = Join-Path $ProjectRoot "native\build\ffmpeg-xmake\manifest.json"
$manifest = Get-Content $manifestPath -Raw | ConvertFrom-Json
$summary.ffmpeg_provenance.tag = $ffTag
$summary.ffmpeg_provenance.commit = $ffCommit
$summary.ffmpeg_provenance.source_sha256 = $ffSourceSha
$summary.ffmpeg_provenance.profile = $profileName
$summary.ffmpeg_provenance.profile_sha256 = $profileSha
$summary.ffmpeg_provenance.recipe_sha256 = $recipeSha
$summary.ffmpeg_provenance.closure_manifest_sha256 = $closureManifestSha
$summary.ffmpeg_provenance.closure_units = $closureUnits
$summary.ffmpeg_provenance.closure_toolchain = [ordered]@{
    system = $manifest.toolchain.system
    machine = $manifest.toolchain.machine
    cc = $manifest.toolchain.cc
    cc_ident = $manifest.toolchain.cc_ident
}
$summary.artifacts["libsongcore_a_sha256"] = $sha
$summary.artifacts["libsongcore_a_size"]   = $size
$summary.artifacts["pe_members"]           = $peMembers
$summary.artifacts["elf_members"]          = $elfMembers
$summary.artifacts["song_abi_symbols"]     = $songAbi
$summary.artifacts["ar_magic"]             = "ok"
$summary.artifacts["abi_header_sha256"]    = $headerShaWin2
$summary.artifacts["negative_shared_exit"] = "nonzero (fail-closed verified)"
$summary.artifacts["sha256_crosscheck"]    = "WSL sha256sum == Windows Get-FileHash"
$summary.steps["summarize"] = "ok"

# --------------------------------------------------- second root (optional)
if ($buildDirWsl -ne "") {
    $stageR2 = @'
set -euo pipefail
export PATH="$HOME/.local/bin:$PATH"
WT="$(wslpath -u "$QR_WT")"
EV="$(wslpath -u "$QR_EV")"
ROOT="$QR_ROOT"
B="$QR_BUILDDIR"
CS="$QR_CLOSURE_SRC"
W2="$B/win"

# Defense in depth: the second root must be a plain absolute WSL-local path
# and must be set at all before any rm/cp below runs.
if [ -z "$B" ]; then
    echo "QR_BUILDDIR is empty; second-root stage must not run"
    exit 11
fi
case "$B" in
  /home/*|/root/*|/tmp/*) : ;;
  *) echo "refusing suspicious build dir: '$B'"; exit 11 ;;
esac
if [ "$B" = "$ROOT" ]; then
    echo "second root equals the primary work root; refusing"
    exit 11
fi

# The second root is per-run fresh: an independent absolute root for the
# contract section 12 T1 comparison (same source SHA / pin / profile / recipe).
rm -rf "$W2"
mkdir -p "$W2/native/build"
cd "$WT"
tar cf - \
    --exclude='native/build/artifacts' \
    --exclude='native/build/xmake' \
    --exclude='native/build/downloads' \
    --exclude='native/build/ffmpeg-xmake' \
    --exclude='native/build/ffmpeg-src' \
    native | tar xf - -C "$W2"

cd "$W2/native"
EXPECT=$(python3 -c "import json;p=json.load(open('ffmpeg/pin.json'));print(p['ffmpeg_commit_sha']+'|'+p['source_sha256'])")

check_marker() {
    local ACTUAL
    ACTUAL=$(cat build/ffmpeg-src/.qianqian-verified)
    if [ "$ACTUAL" != "$EXPECT" ]; then
        echo "closure pin mismatch: '$ACTUAL' != '$EXPECT'"
        exit 11
    fi
}
source_closure_valid() {
    test -f "$1/ffmpeg-xmake/manifest.json" &&
    test -f "$1/ffmpeg-src/.qianqian-verified" &&
    test -f "$1/ffmpeg-xmake/oracle/config_components.h"
}
place_closure() {
    cp -a "$1/ffmpeg-xmake" build/ffmpeg-xmake
    cp -a "$1/ffmpeg-src" build/ffmpeg-src
    check_marker
}

# Reuse an already-imported closure only; never download/re-import here.
if [ -n "$CS" ] && source_closure_valid "$CS"; then
    place_closure "$CS"
    echo "closure: copied from -ClosureSource"
elif source_closure_valid "$ROOT/closure"; then
    place_closure "$ROOT/closure"
    echo "closure: copied from primary work-root cache"
else
    echo "FAIL: no complete imported FFmpeg closure available for the second root."
    exit 11
fi

xmake f -p mingw -a x86_64 -m release -y > "$EV/r2-config.log" 2>&1
xmake build songcore > "$EV/r2-build.log" 2>&1

test -f build/artifacts/libsongcore.a || { echo "FAIL: R2 libsongcore.a missing"; exit 12; }
test ! -e build/artifacts/shared || { echo "FAIL: R2 shared/ dir exists"; exit 13; }

set +e
PE2=$(x86_64-w64-mingw32-objdump -f build/artifacts/libsongcore.a 2>/dev/null | grep -c 'file format pe-x86-64')
ELF2=$(x86_64-w64-mingw32-objdump -f build/artifacts/libsongcore.a 2>/dev/null | grep -c 'file format elf')
SONG2=$(x86_64-w64-mingw32-nm build/artifacts/libsongcore.a 2>/dev/null | grep -c ' T song')
set -e
if [ "$PE2" -le 0 ] || [ "$ELF2" -ne 0 ]; then
    echo "FAIL: R2 member formats pe=$PE2 elf=$ELF2"
    exit 14
fi
if [ "$SONG2" -ne 15 ]; then
    echo "FAIL: R2 song* ABI symbols = $SONG2 (expected 15)"
    exit 15
fi
if xmake build songcore_shared > "$EV/r2-negative-shared.log" 2>&1; then
    echo "FAIL: R2 explicit songcore_shared exited 0 (recipe policy broken)"
    exit 16
fi

# The second root's artifact lands in the run's evidence dir only -- the
# worktree consumption path keeps the primary root's artifact.
mkdir -p "$EV/r2-artifacts"
cp build/artifacts/libsongcore.a "$EV/r2-artifacts/libsongcore.a"

SHA2=$(sha256sum "$EV/r2-artifacts/libsongcore.a" | awk '{print $1}')
SIZE2=$(stat -c '%s' "$EV/r2-artifacts/libsongcore.a")
echo "R2_PE_MEMBERS=$PE2"
echo "R2_ELF_MEMBERS=$ELF2"
echo "R2_SONG_ABI=$SONG2"
echo "R2_ARTIFACT_SHA256=$SHA2"
echo "R2_ARTIFACT_SIZE=$SIZE2"
echo "R2_BUILD_OK"
'@
    Write-LFText (Join-Path $evDir "stage-r2.sh") $stageR2
    $r2Out = Invoke-WslStage "stage-r2.sh" "second_root_build"

    $r2Sha = ""; $r2Size = ""; $r2Pe = ""; $r2Song = ""
    foreach ($line in $r2Out) {
        $s = $line.ToString()
        if ($s -match '^R2_ARTIFACT_SHA256=([0-9a-f]+)$') { $r2Sha = $Matches[1] }
        if ($s -match '^R2_ARTIFACT_SIZE=(\d+)$')         { $r2Size = $Matches[1] }
        if ($s -match '^R2_PE_MEMBERS=(\d+)$')            { $r2Pe = $Matches[1] }
        if ($s -match '^R2_SONG_ABI=(\d+)$')              { $r2Song = $Matches[1] }
    }
    if ($r2Sha -eq "") { Fail "second_root_summarize" "R2 stage did not report R2_ARTIFACT_SHA256" }
    $r2Artifact = Join-Path $evDir "r2-artifacts\libsongcore.a"
    $r2ShaWin = (Get-FileHash -Algorithm SHA256 $r2Artifact).Hash.ToLowerInvariant()
    if ($r2ShaWin -ne $r2Sha) {
        Fail "second_root_summarize" "R2 artifact sha256 mismatch: WSL=$r2Sha Windows=$r2ShaWin"
    }
    $summary.second_root.root = $buildDirWsl
    $summary.second_root.artifact_sha256 = $r2Sha
    $summary.second_root.artifact_size = $r2Size
    $summary.second_root.pe_members = $r2Pe
    $summary.second_root.song_abi_symbols = $r2Song
    $summary.second_root.sha256_crosscheck = "WSL sha256sum == Windows Get-FileHash"
    $summary.second_root.byte_comparison = ("reported only; equal-hash judgment and " +
        "member-wise classification are contract section 12 T1-T6 (S3) work, not judged here")
    $summary.second_root.consumption_path = ("untouched: the second root's artifact is " +
        "evidence-only (" + $r2Artifact + "), not copied into native/build/artifacts")
    $summary.steps["second_root_summarize"] = "ok"
}

# ------------------------------------------------------------------ finish
$summary.ok = $true
Save-Summary
exit 0
