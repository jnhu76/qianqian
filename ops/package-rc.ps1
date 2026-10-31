# package-rc.ps1 -- Windows release-lane RC packager for the SongCore
# windows-x86_64 static artifact (#172 S2), per the frozen contract
# docs/release/songcore-release-contract.md:
#   section 7  PACKAGE_LAYOUT (v0.1.0-shaped tree, schema-2 additions only
#              under metadata/ and LICENSES/; windows ships static-only,
#              no lib/shared/)
#   section 8  PROVENANCE_SCHEMA (metadata/manifest.json, schema_version 2,
#              V1-V7 machine checks executed BEFORE the container is written)
#   section 10 LICENSE_OBLIGATIONS (O1/O2/O4 pass gates; B1-B4 RELEASE_BLOCKED
#              conditions fail the run)
#
# Replayable flow (fail-closed; no container is written until every pre-zip
# check passes):
#   0. preflight: pwsh 7+ required (ConvertTo-Json fidelity), git HEAD,
#      native/ tracked-clean (R2 discipline, mirrors ops/build-rc.ps1),
#      frozen ABI header identity, artifact present.
#   1. WSL audit + license stage (the pinned toolchain's binutils, the same
#      route that built the artifact):
#        - pinned FFmpeg zip sha256 == pin.json source_sha256, then the four
#          FFmpeg COPYING.* texts are extracted FROM THE ZIP (O1: shipped
#          license texts come from the verified pinned source, never
#          hand-maintained copies);
#        - songcore-v0.1.0 tarball sha256 == frozen S-V010 identity, then
#          LICENSES/THIRD_PARTY_NOTICES.txt is extracted from it (S-V010
#          origin, contract section 7);
#        - R3 archive target identity: objdump -f all members pe-x86-64,
#          0 elf (method mirror of tools/check_release.py:117-121);
#        - V4 symbol audit: nm ' T song' == 15, sorted list captured;
#        - member count (ar t) and the undefined-import audit (nm ' U ',
#          BCrypt* presence per contract section 9).
#   2. stage the section 7 tree under <OutDir>/stage/<pkg-root>/.
#   3. build metadata/manifest.json (schema 2) bound to the real staged
#      bytes; write metadata/symbols.txt, metadata/checksums.txt (13
#      entries: every packaged file except itself, v0.1.0 shape) and
#      manifest files{} (the same set minus manifest.json itself --
#      non-circular binding; this set arithmetic is what contract section 8
#      V1 states since the Issue #330 differential disposition).
#   4. V1-V7 + O1/O2 + B4 re-verification on the staged tree; any failure
#      aborts with exit 1 and no container.
#   5. write the .zip container (System.IO.Compression, forward-slash entry
#      names, exact 14-file content set), then re-open it and re-verify:
#      exact entry set, no duplicates/traversal/backslash/symlink members
#      (R4 method), every entry's bytes re-hashed against checksums.txt and
#      files{} (R5 method).
#   6. write SHA256SUMS sidecar next to the zip (<sha256>  <zip-name>),
#      then a package-summary.json receipt.
#
# Version: the concrete next release number is owner-reserved (contract
# section 14 Q1). Default 0.2.0-rc.1 is the S2 ask's provisional RC
# identity, marked provisional in manifest notes; it is NOT a release and
# must never be published under the songcore-v* namespace without owner
# ratification. songcore-v0.1.0 is immutable history (section 2) and is
# refused here.
#
# Usage (Windows PowerShell 5.1 or pwsh 7+; both verified by the in-run
# manifest round-trip check):
#   powershell -ExecutionPolicy Bypass -File ops\package-rc.ps1
#   pwsh -File ops/package-rc.ps1
#   pwsh -File ops/package-rc.ps1 -Force        # accepted (rerun rebuilds anyway)
# Git Bash callers: prefix with MSYS_NO_PATHCONV=1 so WSL-side default paths
# (pinned zip / v0.1.0 tarball) survive argv (same note as ops/build-rc.ps1).
param(
    [string]$ProjectRoot = "",
    [Parameter(Mandatory=$true)][string]$Version,
    [string]$WslDistro = "Ubuntu-24.04",
    # The artifact the manifest binds; default is the build-lane consumption path.
    [string]$ArtifactPath = "",
    [Parameter(Mandatory=$true)][string]$BuildReceipt,
    [string]$OutDir = "",
    # WSL paths: the pinned FFmpeg source zip (re-verified against pin.json
    # on every run) and the published songcore-v0.1.0 tarball (re-verified
    # against the frozen S-V010 sha256 on every run).
    [string]$PinnedZip = "/home/hoo/q172d2-verify/win/native/build/downloads/ffmpeg-n9.0.1.zip",
    [string]$V010Tarball = "/home/hoo/s2pkg-v010/songcore-v0.1.0-linux-x86_64.tar.gz",
    # The section 7 consumer-smoke example (campaign S3 lane authoring;
    # byte-reused, not forked).
    [string]$ConsumerExample = "",
    [switch]$Force
)
$ErrorActionPreference = "Stop"

# Runs under Windows PowerShell 5.1 and pwsh 7+. Serializer equivalence is
# NOT assumed from the version number: the written manifest is verified by a
# semantic round-trip check (empty arrays, nulls, counts) immediately after
# it is written, which fails closed on any serializer defect on ANY host.
# (Gate round 1, 2026-10-29: an earlier "pwsh 7+" guard rejected the gate's
# powershell 5.1 invocation. Probing both serializers showed identical
# semantics for every shape this manifest uses — empty arrays, nulls,
# nested ordered dicts — so the version assumption was removed and replaced
# by the round-trip check below.)
if ($ProjectRoot -eq "") { $ProjectRoot = Split-Path -Parent $PSScriptRoot }
$ProjectRoot = (Resolve-Path $ProjectRoot).Path
if ($ArtifactPath -eq "") { $ArtifactPath = Join-Path $ProjectRoot "native\build\artifacts\libsongcore.a" }
if ($OutDir -eq "")       { $OutDir = Join-Path $ProjectRoot "evidence\songcore-172\package" }
if ($ConsumerExample -eq "") { $ConsumerExample = Join-Path $ProjectRoot "evidence\songcore-172\linux\verify\package\external-consumer-example.c" }
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null

# Frozen identities (docs/release/songcore-release-contract.md section 3 and
# section 15 / S-V010). They move only with an authority reference.
$ExpectedAbiHeaderSha256 = "cf509a5c044735e4813642ac6683691242146e642342a8ac356e98fbd8e78536"
$V010TarSha256           = "018e0cbdfd8d83b6967bbba8e899ae6dbcc2ccf690c5294bd48e2ef2699f6815"
$ImmutableHistorical     = "0.1.0"   # songcore-v0.1.0 is frozen history (section 2)
$SymbolList = @(
    "song_audio_stream_count", "song_audio_stream_info", "song_close",
    "song_get_artwork_count", "song_get_artwork_item", "song_get_metadata",
    "song_get_metadata_count", "song_get_metadata_entry", "song_last_error",
    "song_open", "song_probe", "song_read_pcm", "song_seek",
    "song_select_stream", "songcore_abi_version"
)
$CopyingNames = @("COPYING.GPLv2", "COPYING.GPLv3", "COPYING.LGPLv2.1", "COPYING.LGPLv3")
$ZipOrder = @(
    "include/songcore.h", "lib/libsongcore.a",
    "LICENSES/COPYING.GPLv2", "LICENSES/COPYING.GPLv3", "LICENSES/COPYING.LGPLv2.1",
    "LICENSES/COPYING.LGPLv3", "LICENSES/THIRD_PARTY_NOTICES.txt",
    "LICENSES/LICENSE-MIT", "LICENSES/LICENSE-APACHE",
    "README.md",
    "metadata/manifest.json", "metadata/symbols.txt",
    "metadata/external-consumer-example.c", "metadata/checksums.txt"
)

$utf8 = New-Object System.Text.UTF8Encoding($false)
function Write-LFText([string]$path, [string]$text) {
    [System.IO.File]::WriteAllText($path, $text.Replace("`r`n", "`n"), $utf8)
}
function Convert-ToWslPath([string]$p) {
    if ($p -match '^([A-Za-z]):[\\/](.*)$') {
        $drive = $Matches[1].ToLowerInvariant()
        $rest = $Matches[2] -replace '\\', '/'
        return "/mnt/$drive/$rest"
    }
    return $p
}
function Get-Sha256([string]$path) {
    return (Get-FileHash -Algorithm SHA256 $path).Hash.ToLowerInvariant()
}

$pkgRoot = "songcore-v$Version-windows-x86_64"
$zipPath = Join-Path $OutDir "$pkgRoot.zip"
$summary = [ordered]@{
    script = "ops/package-rc.ps1"
    started = (Get-Date -Format "yyyyMMdd-HHmmss")
    version = $Version
    version_status = "provisional (contract section 14 Q1 owner-reserved)"
    package_root = $pkgRoot
    project = $ProjectRoot
    artifact = $ArtifactPath
    out_dir = $OutDir
    source = [ordered]@{}
    audits = [ordered]@{}
    checks = [ordered]@{}
    container = [ordered]@{}
    ok = $false
}
function Save-Summary {
    Write-LFText (Join-Path $OutDir "package-summary.json") ($summary | ConvertTo-Json -Depth 8)
}
function Fail([string]$step, [string]$detail) {
    $summary.checks[$step] = "FAIL: $detail"
    Save-Summary
    [Console]::Error.WriteLine("package-rc.ps1 [$step]: $detail")
    exit 1
}

# ------------------------------------------------------------------ preflight
if ($Version -eq $ImmutableHistorical) {
    Fail "preflight" "refusing version '0.1.0': songcore-v0.1.0 is immutable history (contract section 2); a failed release releases under a new version, never a re-tag."
}
if ($Version -notmatch '^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$') {
    Fail "preflight" "version '$Version' is not the authorized MAJOR.MINOR.PATCH identity; suffixes are not authorized by contract section 2."
}
if (-not (Test-Path $BuildReceipt)) { Fail "provenance" "build receipt missing: $BuildReceipt" }

$versionDecisionPath = Join-Path $ProjectRoot "docs\release\songcore-version-decision.json"
if (-not (Test-Path $versionDecisionPath)) { Fail "version_identity" "version identity decision missing" }
$versionDecision = Get-Content $versionDecisionPath -Raw | ConvertFrom-Json
if ($versionDecision.schema_version -ne 1 -or $versionDecision.status -ne "APPROVED") {
    Fail "version_identity" "Q1 version identity is OWNER_BLOCKED"
}
if (-not ($versionDecision.authority_ref -is [string]) -or [string]::IsNullOrWhiteSpace($versionDecision.authority_ref)) {
    Fail "version_identity" "approved version decision lacks authority_ref"
}
$expectedTag = "songcore-v$Version"
$expectedLinuxRoot = "$expectedTag-linux-x86_64"
$expectedWindowsRoot = "$expectedTag-windows-x86_64"
if ($versionDecision.semantic_version -ne $Version -or
    $versionDecision.manifest_version -ne $Version -or
    $versionDecision.component_version -ne $Version -or
    $versionDecision.component_tag -ne $expectedTag -or
    $versionDecision.linux_package_root -ne $expectedLinuxRoot -or
    $versionDecision.windows_package_root -ne $expectedWindowsRoot) {
    Fail "version_identity" "approved normalized version mapping does not match requested version"
}
if ($versionDecision.component_version_mode -notin @("independent", "project-coupled")) {
    Fail "version_identity" "component_version_mode is not approved"
}
if ($versionDecision.component_version_mode -eq "project-coupled" -and
    $versionDecision.xmake_project_version -ne $Version) {
    Fail "version_identity" "project-coupled identity does not match xmake project version"
}
if (-not (Test-Path $ArtifactPath)) {
    Fail "preflight" "artifact missing: $ArtifactPath (run ops/build-rc.ps1 first)"
}
foreach ($f in @("native\ffmpeg\pin.json", "native\ffmpeg\targets\windows-x86_64.json",
                 "native\ffmpeg\profiles\codec-base.json", "native\include\songcore.h",
                 "native\build\ffmpeg-xmake\manifest.json", "LICENSE-MIT", "LICENSE-APACHE")) {
    if (-not (Test-Path (Join-Path $ProjectRoot $f))) { Fail "preflight" "missing input: $f" }
}
if (-not (Test-Path $ConsumerExample)) {
    Fail "preflight" "consumer example missing: $ConsumerExample"
}

$gitHead = & git -C $ProjectRoot rev-parse HEAD 2>&1
if ($LASTEXITCODE -ne 0) { Fail "preflight" "git rev-parse failed: $gitHead" }
$summary.source.commit = $gitHead
# R2 discipline (method mirror of tools/check_release.py:83): the packaged
# artifact must be attributable to the pinned source SHA.
$dirty = & git -C $ProjectRoot status --porcelain -- native/ 2>&1
$dirtyTracked = @($dirty | Where-Object { $_ -notmatch '^\?\?' })
if ($dirtyTracked.Count -gt 0) {
    Fail "preflight" "native/ has uncommitted tracked changes; the artifact would not be attributable to $gitHead"
}
$summary.source.native_tracked_clean = $true

$headerSha = Get-Sha256 (Join-Path $ProjectRoot "native\include\songcore.h")
$summary.source.abi_header_sha256 = $headerSha
if ($headerSha -ne $ExpectedAbiHeaderSha256) {
    Fail "preflight" "ABI header drift: $headerSha != frozen $ExpectedAbiHeaderSha256 (contract section 3)"
}

$artifactSha = Get-Sha256 $ArtifactPath
$artifactBytes = (Get-Item $ArtifactPath).Length
$summary.artifact_sha256 = $artifactSha
$summary.artifact_bytes = $artifactBytes

$pin = Get-Content (Join-Path $ProjectRoot "native\ffmpeg\pin.json") -Raw | ConvertFrom-Json
$recipe = Get-Content (Join-Path $ProjectRoot "native\ffmpeg\targets\windows-x86_64.json") -Raw | ConvertFrom-Json
$closurePath = Join-Path $ProjectRoot "native\build\ffmpeg-xmake\manifest.json"
$closure = Get-Content $closurePath -Raw | ConvertFrom-Json
$pinSha = Get-Sha256 (Join-Path $ProjectRoot "native\ffmpeg\pin.json")
$recipeSha = Get-Sha256 (Join-Path $ProjectRoot "native\ffmpeg\targets\windows-x86_64.json")
$profileSha = Get-Sha256 (Join-Path $ProjectRoot "native\ffmpeg\profiles\codec-base.json")
$closureSha = Get-Sha256 $closurePath

$receipt = Get-Content $BuildReceipt -Raw | ConvertFrom-Json
$receiptErrors = @()
if ($receipt.script -ne "ops/build-rc.ps1" -or -not $receipt.ok) { $receiptErrors += "receipt is not a successful ops/build-rc.ps1 run" }
if ($receipt.source.commit -ne $gitHead) { $receiptErrors += "source commit mismatch" }
if ($receipt.artifacts.libsongcore_a_sha256 -ne $artifactSha) { $receiptErrors += "artifact sha256 mismatch" }
if ([int64]$receipt.artifacts.libsongcore_a_size -ne $artifactBytes) { $receiptErrors += "artifact byte count mismatch" }
if ($receipt.artifacts.abi_header_sha256 -ne $headerSha) { $receiptErrors += "header sha256 mismatch" }
if ($receipt.source.recipe_sha256 -ne $recipeSha -or $receipt.ffmpeg_provenance.recipe_sha256 -ne $recipeSha) { $receiptErrors += "recipe sha256 mismatch" }
if ($receipt.ffmpeg_provenance.profile_sha256 -ne $profileSha) { $receiptErrors += "profile sha256 mismatch" }
if ($receipt.ffmpeg_provenance.closure_manifest_sha256 -ne $closureSha) { $receiptErrors += "closure sha256 mismatch" }
if ($receipt.ffmpeg_provenance.commit -ne $pin.ffmpeg_commit_sha -or $receipt.ffmpeg_provenance.source_sha256 -ne $pin.source_sha256) { $receiptErrors += "pin identity mismatch" }
if ($closure.target.recipe_sha256 -ne $recipeSha) { $receiptErrors += "closure recipe_sha256 mismatch" }
if ($receiptErrors.Count -gt 0) { Fail "provenance" ($receiptErrors -join "; ") }
$summary.build_receipt_sha256 = Get-Sha256 $BuildReceipt
$python = (Get-Command python -ErrorAction SilentlyContinue).Source
if (-not $python) { Fail "provenance" "python is required for the shared build-receipt verifier" }
$treeOid = & git -C $ProjectRoot rev-parse 'HEAD^{tree}'
& $python (Join-Path $ProjectRoot "tools\verify_songcore_build_receipt.py") `
    --receipt $BuildReceipt --source-commit $gitHead --source-tree $treeOid `
    --artifact $ArtifactPath --header (Join-Path $ProjectRoot "native\include\songcore.h") `
    --recipe (Join-Path $ProjectRoot "native\ffmpeg\targets\windows-x86_64.json") `
    --profile (Join-Path $ProjectRoot "native\ffmpeg\profiles\codec-base.json") `
    --pin (Join-Path $ProjectRoot "native\ffmpeg\pin.json") --closure $closurePath `
    --target windows-x86_64 --toolchain $receipt.toolchain.identity
if ($LASTEXITCODE -ne 0) { Fail "provenance" "shared build-receipt verifier rejected the Windows build" }

# --------------------------------------------- stage 1: WSL audit + licenses
$stageRoot = Join-Path $OutDir "stage"
$stagePkg = Join-Path $stageRoot $pkgRoot
$stageWsl = Convert-ToWslPath $stagePkg
$auditScript = Join-Path $OutDir "stage-audit.sh"
Write-LFText $auditScript @'
set -euo pipefail
# stage-audit.sh -- WSL side of ops/package-rc.ps1. Emits KEY=VALUE lines on
# stdout; any identity failure exits nonzero (fail-closed) with FAIL on stderr.
ZIP="$QR_PINNED_ZIP"
V010="$QR_V010_TAR"
STAGE="$QR_STAGE_WSL"
WT="$QR_WORKTREE_WSL"
ARCHIVE="$QR_ARTIFACT_WSL"
AR=x86_64-w64-mingw32-ar
NM=x86_64-w64-mingw32-nm
OD=x86_64-w64-mingw32-objdump

for t in "$AR" "$NM" "$OD" sha256sum python3 tar; do
    command -v "$t" > /dev/null || { echo "FAIL: missing tool $t" >&2; exit 10; }
done

# O1 basis: the pinned FFmpeg source zip is re-verified against pin.json on
# every run before anything is extracted from it.
EXPECT_ZIP=$(python3 -c "import json;print(json.load(open('$WT/native/ffmpeg/pin.json'))['source_sha256'])")
GOT_ZIP=$(sha256sum "$ZIP" | awk '{print $1}')
if [ "$GOT_ZIP" != "$EXPECT_ZIP" ]; then
    echo "FAIL: pinned FFmpeg zip sha mismatch: $GOT_ZIP != $EXPECT_ZIP" >&2
    exit 21
fi
echo "ZIP_SHA256=$GOT_ZIP"

mkdir -p "$STAGE/LICENSES"
python3 - "$ZIP" "$STAGE/LICENSES" <<'PY'
import os, sys, zipfile
zip_path, out = sys.argv[1], sys.argv[2]
names = ["COPYING.GPLv2", "COPYING.GPLv3", "COPYING.LGPLv2.1", "COPYING.LGPLv3"]
z = zipfile.ZipFile(zip_path)
# GitHub source zips prefix every member with a top-level dir (FFmpeg-n9.0.1/);
# resolve each COPYING text by its unique suffix, fail-closed on ambiguity.
members = z.namelist()
resolved = {}
for n in names:
    hits = [m for m in members if m.split("/")[-1] == n]
    if len(hits) != 1:
        print("FAIL: pinned zip member resolution for %s: %d hits" % (n, len(hits)), file=sys.stderr)
        sys.exit(22)
    resolved[n] = hits[0]
for n in names:
    with z.open(resolved[n]) as src, open(os.path.join(out, n), "wb") as dst:
        dst.write(src.read())
print("COPYING_EXTRACTED=" + ",".join(names))
PY
for n in COPYING.GPLv2 COPYING.GPLv3 COPYING.LGPLv2.1 COPYING.LGPLv3; do
    echo "SHA_$n=$(sha256sum "$STAGE/LICENSES/$n" | awk '{print $1}')"
done

# THIRD_PARTY_NOTICES.txt origin: the published songcore-v0.1.0 asset,
# re-verified against the frozen S-V010 sha256 on every run (contract
# section 7 origin note + section 10 O2/B3).
GOT_V010=$(sha256sum "$V010" | awk '{print $1}')
if [ "$GOT_V010" != "$QR_V010_SHA" ]; then
    echo "FAIL: songcore-v0.1.0 tarball sha mismatch: $GOT_V010 != $QR_V010_SHA" >&2
    exit 23
fi
echo "V010_TAR_SHA256=$GOT_V010"
V010_MEMBER="songcore-v0.1.0-linux-x86_64/LICENSES/THIRD_PARTY_NOTICES.txt"
TMPD=$(mktemp -d)
tar -xzf "$V010" -C "$TMPD" "$V010_MEMBER"
mv "$TMPD/$V010_MEMBER" "$STAGE/LICENSES/THIRD_PARTY_NOTICES.txt"
rm -rf "$TMPD"

# P3 wording refresh (Issue #330; review-C F4 / P3-3): the v0.1.0-extracted
# notices carry two stale statements — the v0.1.0/linux identity header and
# the "carry no open-source license yet ... NOT granted by this package"
# Qianqian paragraph, which contradicts the LICENSE-MIT/LICENSE-APACHE texts
# shipped alongside in the same package (B4) and the repo-root dual license.
# The historical RC container keeps the stale wording on purpose: archived
# evidence is not rewritten (frozen zip sha256 7b4d0615d25af464739373a0a356
# 5f77f3208ce9c263f59e578d443801536415 unchanged). Every pack from this
# script forward gets the corrected identity header and component-license
# paragraph below; the extracted FFmpeg section stays byte-identical to the
# v0.1.0 asset. Fail-closed: if the v0.1.0-extracted shape ever drifts, the
# run aborts (exit 24) instead of shipping a silent partial rewrite.
python3 - "$STAGE/LICENSES/THIRD_PARTY_NOTICES.txt" <<'PY'
import os, sys
path = sys.argv[1]
lines = open(path, encoding="utf-8").read().split("\n")
if not (len(lines) >= 3 and lines[0].startswith("Third-party notices")
        and lines[1] and set(lines[1]) == {"="}):
    sys.stderr.write("FAIL: notices identity header shape drifted\n")
    sys.exit(24)
q = [i for i, l in enumerate(lines) if l == "Qianqian"]
if len(q) != 1:
    sys.stderr.write("FAIL: expected exactly one Qianqian section, got %d\n" % len(q))
    sys.exit(24)
title = "Third-party notices \u2014 SongCore %s (%s)" % (
    os.environ["QR_NOTICES_VERSION"], os.environ["QR_NOTICES_TARGET"])
lines[0] = title
lines[1] = "=" * len(title)
paragraph = [
    "Qianqian",
    "  The Qianqian sources (songcore.h and the SongCore implementation) are",
    "  dual-licensed: MIT or Apache-2.0, at the recipient's option. The",
    "  complete license texts ship in this package as LICENSES/LICENSE-MIT",
    "  and LICENSES/LICENSE-APACHE and are sha256-bound in",
    "  metadata/checksums.txt. This is an engineering statement, not legal",
    "  advice; each release's license block is owner-reviewed before",
    "  publication (contract section 10).",
]
open(path, "w", encoding="utf-8", newline="\n").write(
    "\n".join(lines[:q[0]] + paragraph) + "\n")
PY
echo "NOTICES_SHA256=$(sha256sum "$STAGE/LICENSES/THIRD_PARTY_NOTICES.txt" | awk '{print $1}')"

# R3 archive target identity (method mirror of tools/check_release.py:117).
PE=$("$OD" -f "$ARCHIVE" 2>/dev/null | grep -c "file format pe-x86-64") || PE=0
ELF=$("$OD" -f "$ARCHIVE" 2>/dev/null | grep -c "file format elf") || ELF=0
if [ "$PE" -le 0 ] || [ "$ELF" -ne 0 ]; then
    echo "FAIL: R3 archive target identity: pe=$PE elf=$ELF (expected pe>0 elf==0)" >&2
    exit 14
fi
echo "PE_MEMBERS=$PE"
echo "ELF_MEMBERS=$ELF"

# V4 static symbol audit: all 15 ABI v1 entry points DEFINED in the archive.
SONGS=$("$NM" "$ARCHIVE" 2>/dev/null | grep " T song" | awk '{print $3}' | sort -u)
SONGN=$(echo "$SONGS" | grep -c .) || SONGN=0
if [ "$SONGN" -ne 15 ]; then
    echo "FAIL: static symbol audit: $SONGN song* T symbols (expected 15)" >&2
    exit 15
fi
echo "SONG_SYMBOLS=15"
printf "%s\n" "$SONGS" > "$STAGE/../audit-symbols.txt"
while IFS= read -r s; do
    if [ -n "$s" ]; then echo "SYMBOL=$s"; fi
done <<< "$SONGS"

MCOUNT=$("$AR" t "$ARCHIVE" | grep -c .)
if [ "$MCOUNT" -ne "$PE" ]; then
    echo "FAIL: member count $MCOUNT != PE member count $PE" >&2
    exit 24
fi
echo "MEMBER_COUNT=$MCOUNT"

# Contract section 9 runtime-dependency audit base: the undefined-import set.
"$NM" "$ARCHIVE" 2>/dev/null | grep " U " | awk '{print $2}' | sort -u > "$STAGE/../audit-undefined.txt"
BCRYPT=$(grep -E "^BCrypt" "$STAGE/../audit-undefined.txt" | paste -sd, -)
if [ -z "$BCRYPT" ]; then
    echo "FAIL: BCrypt* undefined imports absent (contract section 9 audit base changed)" >&2
    exit 25
fi
echo "UNDEFINED_BCRYPT=$BCRYPT"
echo "UNDEFINED_COUNT=$(grep -c . "$STAGE/../audit-undefined.txt")"
echo "AUDIT_OK"
'@

if (Test-Path $stageRoot) { Remove-Item -Recurse -Force $stageRoot }
New-Item -ItemType Directory -Force -Path $stagePkg | Out-Null

$env:QR_PINNED_ZIP    = $PinnedZip
$env:QR_V010_TAR      = $V010Tarball
$env:QR_V010_SHA      = $V010TarSha256
$env:QR_STAGE_WSL     = $stageWsl
$env:QR_WORKTREE_WSL  = Convert-ToWslPath $ProjectRoot
$env:QR_ARTIFACT_WSL  = Convert-ToWslPath $ArtifactPath
$env:QR_NOTICES_VERSION = "v$Version"
$env:QR_NOTICES_TARGET  = "windows-x86_64"
$env:WSLENV           = "QR_PINNED_ZIP:QR_V010_TAR:QR_V010_SHA:QR_STAGE_WSL:QR_WORKTREE_WSL:QR_ARTIFACT_WSL:QR_NOTICES_VERSION:QR_NOTICES_TARGET"

$auditOut = & wsl.exe -d $WslDistro -- bash (Convert-ToWslPath $auditScript) 2>&1
$auditRc = $LASTEXITCODE
$auditText = ($auditOut | ForEach-Object { $_.ToString() }) -join "`n"
Write-LFText (Join-Path $OutDir "stage-audit.log") $auditText
if ($auditRc -ne 0) {
    Fail "wsl_audit" "audit stage exited $auditRc; see evidence/songcore-172/package/stage-audit.log"
}

$audit = @{}
foreach ($line in $auditOut) {
    $s = $line.ToString()
    if ($s -match '^([A-Z0-9_.]+)=(.*)$') { $audit[$Matches[1]] = $Matches[2] }
}
foreach ($k in @("ZIP_SHA256", "V010_TAR_SHA256", "NOTICES_SHA256", "PE_MEMBERS",
                 "ELF_MEMBERS", "SONG_SYMBOLS", "MEMBER_COUNT", "UNDEFINED_BCRYPT", "UNDEFINED_COUNT")) {
    if (-not $audit.ContainsKey($k)) { Fail "wsl_audit" "audit stage did not report $k" }
}
$summary.audits.pinned_zip_sha256 = $audit["ZIP_SHA256"]
$summary.audits.v010_tarball_sha256 = $audit["V010_TAR_SHA256"]
$summary.audits.third_party_notices_sha256 = $audit["NOTICES_SHA256"]
$summary.audits.pe_members = $audit["PE_MEMBERS"]
$summary.audits.elf_members = $audit["ELF_MEMBERS"]
$summary.audits.member_count = $audit["MEMBER_COUNT"]
$summary.audits.song_symbols = $audit["SONG_SYMBOLS"]
$summary.audits.undefined_bcrypt = $audit["UNDEFINED_BCRYPT"]
$summary.audits.undefined_count = $audit["UNDEFINED_COUNT"]
$summary.checks["R3_archive_target_identity"] = "PASS (objdump -f: $($audit['PE_MEMBERS']) pe-x86-64, $($audit['ELF_MEMBERS']) elf)"

# ------------------------------------------------------------- stage 2: tree
New-Item -ItemType Directory -Force -Path (Join-Path $stagePkg "include"), `
    (Join-Path $stagePkg "lib"), (Join-Path $stagePkg "metadata") | Out-Null
Copy-Item (Join-Path $ProjectRoot "native\include\songcore.h") (Join-Path $stagePkg "include\songcore.h") -Force
Copy-Item $ArtifactPath (Join-Path $stagePkg "lib\libsongcore.a") -Force
Copy-Item (Join-Path $ProjectRoot "LICENSE-MIT")   (Join-Path $stagePkg "LICENSES\LICENSE-MIT") -Force
Copy-Item (Join-Path $ProjectRoot "LICENSE-APACHE")(Join-Path $stagePkg "LICENSES\LICENSE-APACHE") -Force
Copy-Item $ConsumerExample (Join-Path $stagePkg "metadata\external-consumer-example.c") -Force

$stagedHeaderSha = Get-Sha256 (Join-Path $stagePkg "include\songcore.h")
if ($stagedHeaderSha -ne $ExpectedAbiHeaderSha256) {
    Fail "stage" "staged include/songcore.h drift: $stagedHeaderSha"
}
$stagedArtifactSha = Get-Sha256 (Join-Path $stagePkg "lib\libsongcore.a")
if ($stagedArtifactSha -ne $artifactSha) {
    Fail "stage" "staged lib/libsongcore.a sha mismatch: $stagedArtifactSha != $artifactSha"
}
$summary.checks["V2_header_identity"] = "PASS (packaged == source == frozen cf509a5c...78536)"

$symbolsFromAudit = @(Get-Content (Join-Path $stageRoot "audit-symbols.txt") | ForEach-Object { $_.Trim() } | Where-Object { $_ })
if ($symbolsFromAudit.Count -ne 15) { Fail "V4_symbols" "audit symbol list has $($symbolsFromAudit.Count) entries" }
$sortedExpected = @($SymbolList | Sort-Object)
$symbolsSorted = @($symbolsFromAudit | Sort-Object)
$mismatch = @(Compare-Object -ReferenceObject $sortedExpected -DifferenceObject $symbolsSorted)
if ($mismatch.Count -gt 0) {
    Fail "V4_symbols" ("audit symbols != contract section 3 surface: " + (($mismatch | ForEach-Object { $_.InputObject }) -join ","))
}
$summary.checks["V4_symbols_static_audit"] = "PASS (15/15 defined ' T song' symbols == contract section 3 set)"

# ----------------------------------------------------------------- README.md
$readme = @"
# SongCore $Version -- windows-x86_64

SongCore is the local media -> PCM decode component of Qianqian: one local
media file in, one coherent song snapshot out (identity, metadata, artwork,
stream information, source-rate/source-layout Float32 interleaved PCM, seek,
EOF). The ABI authority is include/songcore.h alone (ABI v1, frozen);
packaging never modifies it.

This is a release-substrate verification archive, NOT an RC or release. G4 is
not approved; RC build is not authorized; no tag or Release was created. The
next release tag remains owner-reserved (contract section 14 Q1).

## Contents

    include/songcore.h                  the ONLY ABI description (ABI v1, frozen)
    lib/libsongcore.a                   static artifact (FFmpeg closure merged in,
                                        280 pe-x86-64 members, static-first policy)
    LICENSES/                           FFmpeg COPYING set (extracted from the
                                        sha256-verified pinned n9.0.1 source zip),
                                        THIRD_PARTY_NOTICES.txt (published
                                        songcore-v0.1.0 asset), MIT + Apache-2.0
                                        component texts (repo-root, B4)
    metadata/manifest.json              provenance, schema_version 2
    metadata/symbols.txt                the 15 ABI v1 exports
    metadata/checksums.txt              sha256 of every packaged file except itself
    metadata/external-consumer-example.c  consumer smoke (open->probe->decode->seek->EOF->close)

## Verify the package

    sha256sum -c metadata/checksums.txt   # from inside the package root

## Link against it (C consumer link contract)

Static (the supported consumption path; contract section 9):

    gcc -O2 -Iinclude your.c -Llib -lsongcore -lbcrypt -o your.exe

The explicit link list is exactly: -lsongcore -lbcrypt.

    -lsongcore   the merged-closure static archive (FFmpeg ships in object
                 form inside it; no standalone FFmpeg runtime library is
                 needed or shipped)
    -lbcrypt     REQUIRED: the archive's Win32 entropy path imports
                 BCryptOpenAlgorithmProvider / BCryptGenRandom /
                 BCryptCloseAlgorithmProvider (audited per contract section 9;
                 recorded in metadata/manifest.json runtime_dependencies)

The mingw-w64 runtime (msvcrt-family CRT + Win32 system imports) is provided
by the toolchain that links the consumer; system-DLL allowlist discipline is
codified in tools/check_release.py (SYSTEM_DLLS incl. bcrypt,
bcryptprimitives, msvcrt).

## Consumer smoke

    gcc -O2 -Iinclude metadata/external-consumer-example.c -Llib -lsongcore -lbcrypt -o songcore-consumer-example.exe
    ./songcore-consumer-example.exe <media-file>

## Relink obligations (LGPL-2.1-or-later, contract section 10 O3)

The archive contains the FFmpeg members in object form (effective license
LGPL-2.1-or-later: the codec-base profile keeps gpl/version3/nonfree off and
pulls no external lib* copylibs). The machine-verifiable rebuild inputs --
native/ffmpeg/pin.json (tag->commit + zip sha256), the codec-base capability
profile, the windows-x86_64 target recipe and the Xmake import/replay tools are
part of the source repository. The imported closure manifest is generated
build output and is not in that commit. Until a clean pinned-source import plus
relink recipe is independently executed, O3 remains FIX_REQUIRED; this archive
does not claim an executable relink path.
"@
Write-LFText (Join-Path $stagePkg "README.md") $readme

# ------------------------------------------------- stage 3: manifest binding
$builtUtc = (Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ssZ")
$configureArgs = @($closure.configure_args)
$forbidden = @($configureArgs | Where-Object {
    $_ -like "--enable-gpl*" -or $_ -like "--enable-version3*" -or
    $_ -like "--enable-nonfree*" -or $_ -like "--enable-lib*" })
if ($forbidden.Count -gt 0) {
    Fail "V5_license_args" ("forbidden configure args present: " + ($forbidden -join " "))
}

$manifest = [ordered]@{
    schema_version = 2
    songcore_version = $Version
    songcore_abi = 1
    abi_header_sha256 = $stagedHeaderSha
    target = "windows-x86_64"
    platform = "windows"
    arch = "x86_64"
    toolchain = $recipe.toolchain.family
    ffmpeg_version = $pin.ffmpeg_version
    ffmpeg_commit = $pin.ffmpeg_commit_sha
    ffmpeg_source_sha256 = $pin.source_sha256
    ffmpeg_profile = "codec-base"
    ffmpeg_profile_sha256 = $profileSha
    artifact = "lib/libsongcore.a"
    sha256 = $stagedArtifactSha
    artifact_bytes = $artifactBytes
    archive_members = [int]$audit["MEMBER_COUNT"]
    artifact_shared = $null
    shared_sha256 = $null
    public_symbols = $sortedExpected
    git_sha = $gitHead
    target_identity = [ordered]@{
        id = $recipe.id
        platform = $recipe.platform
        arch = $recipe.arch
        triple = "x86_64-pc-windows-gnu"
        target_os = $recipe.ffmpeg.target_os
        cross_prefix = $recipe.ffmpeg.cross_prefix
        toolchain = [ordered]@{ family = $recipe.toolchain.family; identity = $recipe.toolchain.identity }
        recipe_sha256 = $recipeSha
    }
    tools = [ordered]@{
        cc = $receipt.toolchain.cc
        ar = $receipt.toolchain.ar
        xmake = $receipt.toolchain.xmake
    }
    sdk_ndk = "not applicable this round -- windows-x86_64: mingw-w64 cross, no MSVC/SDK (target recipe toolchain identity, contract section 8)"
    source = [ordered]@{ commit = $gitHead; status = "clean" }
    pin = [ordered]@{
        ffmpeg_tag = $pin.ffmpeg_tag
        ffmpeg_commit_sha = $pin.ffmpeg_commit_sha
        ffmpeg_source_sha256 = $pin.source_sha256
        pin_sha256 = $pinSha
    }
    closure = [ordered]@{
        manifest_sha256 = $closureSha
        toolchain = [ordered]@{
            system = $closure.toolchain.system
            machine = $closure.toolchain.machine
            cc = $closure.toolchain.cc
            cc_ident = $closure.toolchain.cc_ident
            arch = $closure.toolchain.arch
            target_os = $closure.toolchain.target_os
        }
        configure_args = $configureArgs
        units = [ordered]@{
            translation_units = $closure.closure.translation_units
            upstream_sources = $closure.closure.upstream_sources
            generated_sources = $closure.closure.generated_sources
        }
        reference_archives = $closure.reference_archives
    }
    files = [ordered]@{}
    runtime_dependencies = [ordered]@{
        static = [ordered]@{
            bcrypt = "REQUIRED -- audited: nm undefined imports $($audit['UNDEFINED_BCRYPT']) (contract section 9)"
            mingw_w64_runtime = "REQUIRED -- msvcrt-family CRT + Win32 system imports; system-DLL allowlist base tools/check_release.py SYSTEM_DLLS (bcrypt, bcryptprimitives, msvcrt, ...)"
            link_line = "gcc -O2 -Iinclude consumer.c -Llib -lsongcore -lbcrypt -o consumer.exe (nothing else observed required for the static archive)"
        }
        ffmpeg_runtime = "none -- the closure ships in object form inside the archive (contract section 9)"
    }
    license = [ordered]@{
        ffmpeg_effective = "LGPL-2.1-or-later"
        gpl_enabled = $false
        version3_enabled = $false
        nonfree_enabled = $false
        external_libs = @()
        license_files = [ordered]@{}
    }
    reproducibility = [ordered]@{
        recipe = "NOT_ESTABLISHED"
        semantic_abi = "NOT_ESTABLISHED"
        byte = "NOT_ESTABLISHED"
        evidence = ("evidence/songcore-172/package/repro/ (T1-T3 two-root receipt from ops/build-rc.ps1 run 20261029-062848: " +
                    "identity.txt, member-order.verdict, member-diff.verdict, differing-members.txt, rootcause-strings.txt, " +
                    "archive-path-scan.txt, debug-info-sample.txt, songcore-c-object.txt, undefined-imports.txt, closure-identity.txt)")
    }
    built_utc = $builtUtc
    packer = "release-substrate songcore-172 S2 windows package lane (ops/package-rc.ps1; pwsh $($PSVersionTable.PSVersion); WSL2 Ubuntu-24.04 audits: x86_64-w64-mingw32 binutils, xmake v2.9.7; artifact built by x86_64-w64-mingw32-gcc 13-win32)"
    notes = ("songcore_version $Version is caller-supplied: the concrete next release number is owner-reserved " +
             "(contract section 14 Q1); this is a release-substrate verification archive, not an RC or release " +
             "(G4 NOT_APPROVED; RC_BUILD NOT_AUTHORIZED), and must never be published under the songcore-v* " +
             "namespace without owner ratification. " +
             "files{} covers the checksums.txt set minus manifest.json itself (non-circular binding; the set " +
             "arithmetic contract section 8 V1 states since the Issue #330 differential disposition). " +
             "LICENSES/THIRD_PARTY_NOTICES.txt was extracted from the sha256-verified published songcore-v0.1.0 " +
             "asset ($V010TarSha256) and then wording-refreshed per the Issue #330 P3 disposition: identity " +
             "header updated to this package's version/target and the stale component-license paragraph " +
             "replaced by the dual-license statement (LICENSE-MIT/LICENSE-APACHE ship alongside, B4); the " +
             "FFmpeg section stays byte-identical to the v0.1.0 asset. " +
             "The FFmpeg COPYING texts were extracted from the pinned n9.0.1 source zip " +
             "re-verified against pin.json source_sha256 on this run (O1). " +
             "Machine license checks verify presence, hash identity and configure-arg provenance only; they cannot " +
             "certify legal sufficiency (contract section 10 limit).")
}

$manifestPath = Join-Path $stagePkg "metadata\manifest.json"
$symbolsPath = Join-Path $stagePkg "metadata\symbols.txt"
Write-LFText $symbolsPath ((($sortedExpected | ForEach-Object { "$_" }) -join "`n") + "`n")
$licenseFiles = [ordered]@{}
foreach ($n in $CopyingNames) {
    $p = Join-Path $stagePkg "LICENSES\$n"
    if (-not (Test-Path $p)) { Fail "O1_license_texts" "missing staged $n" }
    $licenseFiles[$n] = Get-Sha256 $p
    $expectedZipSha = $audit["SHA_$n"]
    if ($licenseFiles[$n] -ne $expectedZipSha) {
        Fail "O1_license_texts" "$n staged sha != pinned-zip extraction sha"
    }
}
$manifest.license.license_files = $licenseFiles
$summary.checks["O1_license_texts"] = "PASS (4 FFmpeg COPYING texts extracted from the pin-verified source zip; staged shas == extraction shas)"
$summary.checks["O2_B3_notices"] = "PASS (THIRD_PARTY_NOTICES.txt extracted from the sha-verified v0.1.0 asset, then #330 P3 wording-refreshed: identity header + component-license paragraph; hashed in checksums.txt/files{} below)"
$b4Ok = $true
foreach ($pair in @(@("LICENSE-MIT", "LICENSES\LICENSE-MIT"), @("LICENSE-APACHE", "LICENSES\LICENSE-APACHE"))) {
    if ((Get-Sha256 (Join-Path $ProjectRoot $pair[0])) -ne (Get-Sha256 (Join-Path $stagePkg $pair[1]))) { $b4Ok = $false }
}
if (-not $b4Ok) { Fail "B4_component_licenses" "staged LICENSE-MIT/LICENSE-APACHE != repo-root texts at the source commit" }
$summary.checks["B4_component_licenses"] = "PASS (LICENSE-MIT + LICENSE-APACHE byte-identical to repo-root texts)"

Write-LFText $manifestPath ($manifest | ConvertTo-Json -Depth 16)

# files{} + checksums.txt (non-circular binding; see notes differential).
$packaged = @(Get-ChildItem $stagePkg -Recurse -File | ForEach-Object {
    $_.FullName.Substring($stagePkg.Length + 1).Replace("\", "/")
} | Sort-Object)
$expectedContent = @($ZipOrder | Sort-Object)
# checksums.txt is written right after this comparison, so include it as pending.
$packagedWithPending = @(($packaged + @("metadata/checksums.txt")) | Sort-Object -Unique)
$contentDiff = @(Compare-Object -ReferenceObject $expectedContent -DifferenceObject $packagedWithPending)
if ($contentDiff.Count -ne 0) {
    Fail "V1_sets" ("staged content set != contract section 7 set: " +
        (($contentDiff | ForEach-Object { $_.InputObject }) -join ","))
}
$coveredCk = @($packaged | Where-Object { $_ -ne "metadata/checksums.txt" })   # 13 entries (incl. manifest.json)
$coveredMf = @($coveredCk | Where-Object { $_ -ne "metadata/manifest.json" })  # 12 entries (non-circular)
$hashOf = @{}
foreach ($rel in $packaged) { $hashOf[$rel] = Get-Sha256 (Join-Path $stagePkg ($rel -replace "/", "\")) }
$mf = [ordered]@{}
foreach ($rel in $coveredMf) {
    $p = Join-Path $stagePkg ($rel -replace "/", "\")
    $mf[$rel] = [ordered]@{ sha256 = $hashOf[$rel]; bytes = (Get-Item $p).Length }
}
$manifest.files = $mf
Write-LFText $manifestPath ($manifest | ConvertTo-Json -Depth 16)

# Semantic round-trip verification of the exact written bytes: re-parse and
# assert the shapes a broken serializer could silently corrupt. Probed equal
# on Windows PowerShell 5.1.26100 and pwsh 7.4.22; this check, not the host
# version, is what the run's correctness rests on.
$rt = Get-Content $manifestPath -Raw | ConvertFrom-Json
$rtOk =
    ($rt.schema_version -eq 2) -and
    (@($rt.license.external_libs).Count -eq 0) -and
    ($null -eq $rt.shared_sha256) -and
    ($null -eq $rt.artifact_shared) -and
    (@($rt.public_symbols).Count -eq 15) -and
    (@($rt.files.PSObject.Properties).Count -eq 12) -and
    (@($rt.license.license_files.PSObject.Properties).Count -eq 4) -and
    ($rt.sha256 -eq $stagedArtifactSha) -and
    ($rt.license.gpl_enabled -eq $false) -and
    ($rt.license.version3_enabled -eq $false) -and
    ($rt.license.nonfree_enabled -eq $false) -and
    ($rt.closure.configure_args.Count -eq $configureArgs.Count)
if (-not $rtOk) {
    Fail "manifest_roundtrip" ("written manifest.json failed semantic round-trip on " +
        "host $($PSVersionTable.PSVersion) — serializer defect, refusing to package")
}
$summary.checks["manifest_roundtrip"] = "PASS (semantic re-parse of the written manifest.json on $($PSVersionTable.PSVersion))"
# Re-bind the manifest's own hash AFTER the final write: checksums.txt must
# cover the final manifest bytes (the files{} addition changed them).
$hashOf["metadata/manifest.json"] = Get-Sha256 $manifestPath
$checksumsText = (($coveredCk | ForEach-Object { "$($hashOf[$_])  $_" }) -join "`n") + "`n"
Write-LFText (Join-Path $stagePkg "metadata\checksums.txt") $checksumsText
$hashOf["metadata/checksums.txt"] = Get-Sha256 (Join-Path $stagePkg "metadata\checksums.txt")
$summary.checks["V1_sets"] = "PASS (14 packaged files; checksums.txt 13 entries = all except itself, v0.1.0 shape; files{} 12 entries = checksums set minus manifest.json)"

# ------------------------------------------------------- stage 4: V-checks
$checks = [ordered]@{}
$v1rehash = $true
foreach ($rel in $coveredMf) {
    if ((Get-Sha256 (Join-Path $stagePkg ($rel -replace "/", "\"))) -ne $manifest.files[$rel].sha256) { $v1rehash = $false }
}
$checks["V1_files_rehash"] = $v1rehash
$checks["V1_sets"] = (((@($manifest.files.Keys) | Sort-Object) -join "`n") -eq (($coveredMf | Sort-Object) -join "`n"))
$checks["V2_header_sha"] = ($manifest.abi_header_sha256 -eq $stagedHeaderSha -and $stagedHeaderSha -eq $headerSha -and $headerSha -eq $ExpectedAbiHeaderSha256)
$checks["V3_pin_carry_dups"] = (
    $manifest.ffmpeg_commit -eq $manifest.pin.ffmpeg_commit_sha -and $manifest.ffmpeg_commit -eq $pin.ffmpeg_commit_sha -and
    $manifest.ffmpeg_source_sha256 -eq $manifest.pin.ffmpeg_source_sha256 -and $manifest.ffmpeg_source_sha256 -eq $pin.source_sha256 -and
    $manifest.pin.pin_sha256 -eq $pinSha -and $manifest.closure.manifest_sha256 -eq $closureSha -and
    $manifest.ffmpeg_profile_sha256 -eq $profileSha -and
    $closure.target.recipe_sha256 -eq $recipeSha)
$checks["V4_symbols"] = ((($manifest.public_symbols | ForEach-Object { "$_" }) -join "`n") -eq (($sortedExpected | ForEach-Object { "$_" }) -join "`n"))
$checks["V5_license_args"] = ($forbidden.Count -eq 0 -and -not $manifest.license.gpl_enabled -and
                              -not $manifest.license.version3_enabled -and -not $manifest.license.nonfree_enabled)
$repDir = Join-Path $ProjectRoot "evidence\songcore-172\package\repro"
$checks["V6_repro_receipt"] = (
    (Test-Path $repDir) -and ((Get-ChildItem $repDir -Filter "*.txt" | Measure-Object).Count -ge 8) -and
    $manifest.reproducibility.recipe -eq "NOT_ESTABLISHED" -and
    $manifest.reproducibility.semantic_abi -eq "NOT_ESTABLISHED" -and
    $manifest.reproducibility.byte -eq "NOT_ESTABLISHED" -and
    $manifest.reproducibility.evidence.Length -gt 0)
$checks["V7_tools"] = (
    $manifest.tools.cc.Length -gt 0 -and $manifest.tools.ar.Length -gt 0 -and $manifest.tools.xmake.Length -gt 0 -and
    $manifest.sdk_ndk.Contains("not applicable") -and
    $manifest.target_identity.toolchain.family -eq $recipe.toolchain.family -and
    $manifest.target_identity.toolchain.identity -eq $recipe.toolchain.identity -and
    $manifest.target_identity.recipe_sha256 -eq $recipeSha)
# O1 was already hard-verified above (staged sha == pinned-zip extraction sha).
$checks["O1_license_texts"] = $true
$checks["B4_component_licenses"] = $b4Ok
$checks["manifest_roundtrip"] = $rtOk
Write-LFText (Join-Path $OutDir "v-checks.json") ($checks | ConvertTo-Json -Depth 4)
$failed = @($checks.Keys | Where-Object { -not $checks[$_] })
if ($failed.Count -gt 0) { Fail "V_checks" ("contract section 8 machine checks failed: " + ($failed -join ",")) }
$summary.checks["V1_V7_O1_B4"] = "PASS (all machine checks; see v-checks.json)"

# ------------------------------------------------------- stage 5: container
# Replayable by default: a rerun rebuilds the container from a freshly
# staged and re-verified tree. The previous container's sha is recorded so
# an overwrite is always visible in the receipt (-Force is accepted and
# kept only as a compatibility no-op).
if (Test-Path $zipPath) {
    $summary.container.previous_zip_sha256 = Get-Sha256 $zipPath
    $summary.container.rebuilt = $true
}
Add-Type -AssemblyName System.IO.Compression
Add-Type -AssemblyName System.IO.Compression.FileSystem
if (Test-Path $zipPath) { Remove-Item -Force $zipPath }
if ($ZipOrder.Count -ne 14) { Fail "container" "internal: zip order list must be the 14-file content set" }
$fs = [System.IO.File]::Open($zipPath, [System.IO.FileMode]::CreateNew)
$zip = New-Object System.IO.Compression.ZipArchive($fs, [System.IO.Compression.ZipArchiveMode]::Create)
try {
    foreach ($rel in $ZipOrder) {
        $src = Join-Path $stagePkg ($rel -replace "/", "\")
        $entry = $zip.CreateEntry("$pkgRoot/$rel", [System.IO.Compression.CompressionLevel]::Optimal)
        $entry.LastWriteTime = (Get-Item $src).LastWriteTime
        $bytes = [System.IO.File]::ReadAllBytes($src)
        $es = $entry.Open()
        $es.Write($bytes, 0, $bytes.Length)
        $es.Close()
    }
} finally { $zip.Dispose(); $fs.Dispose() }

# ------------------------------------------------ stage 6: post-zip R4/R5
$zipSha = Get-Sha256 $zipPath
$fs2 = [System.IO.File]::Open($zipPath, [System.IO.FileMode]::Open, [System.IO.FileAccess]::Read)
$zr = New-Object System.IO.Compression.ZipArchive($fs2, [System.IO.Compression.ZipArchiveMode]::Read)
try {
    $names = @($zr.Entries | ForEach-Object { $_.FullName })
    $expectedNames = @($ZipOrder | ForEach-Object { "$pkgRoot/$_" })
    $dup = @($names | Group-Object | Where-Object { $_.Count -gt 1 })
    if ($dup.Count -gt 0) {
        Fail "R4_zip" ("duplicate ZIP entries: " + (($dup | ForEach-Object { $_.Name }) -join ","))
    }
    $setDiff = @(Compare-Object -ReferenceObject $expectedNames -DifferenceObject $names)
    if ($setDiff.Count -ne 0) {
        Fail "R4_zip" ("exact package file set mismatch: " + (($setDiff | ForEach-Object { $_.InputObject }) -join ","))
    }
    foreach ($e in $zr.Entries) {
        if ($e.FullName -match '\\') { Fail "R4_zip" "backslash entry name: $($e.FullName)" }
        if ($e.FullName -match '\.\.' -or $e.FullName -match '^[A-Za-z]:') { Fail "R4_zip" "traversal/absolute entry: $($e.FullName)" }
        $attr = $e.ExternalAttributes -shr 16
        if ((($attr -band 0xF000) -eq 0xA000)) { Fail "R4_zip" "symlink member: $($e.FullName)" }
        if ($e.Length -ge 100000000) { Fail "R4_zip" "unexpected member size: $($e.FullName)" }
        $rel = $e.FullName.Substring($pkgRoot.Length + 1)
        $ms = New-Object System.IO.MemoryStream
        $inStream = $e.Open()
        $inStream.CopyTo($ms)
        $inStream.Close()
        $sha256Obj = [System.Security.Cryptography.SHA256]::Create()
        $entrySha = (($sha256Obj.ComputeHash($ms.ToArray())) | ForEach-Object { $_.ToString("x2") }) -join ""
        $sha256Obj.Dispose()
        if ($rel -eq "metadata/checksums.txt" -or $rel -eq "metadata/manifest.json") {
            if ($entrySha -ne $hashOf[$rel]) { Fail "R5_zip" "$rel entry bytes drifted from staged tree" }
        } else {
            if ($entrySha -ne $manifest.files[$rel].sha256) { Fail "R5_zip" "payload hash mismatch: $rel" }
            if ($ms.Length -ne $manifest.files[$rel].bytes) { Fail "R5_zip" "payload size mismatch: $rel" }
        }
    }
} finally { $zr.Dispose(); $fs2.Dispose() }
$summary.checks["R4_zip_exact_set"] = "PASS (14 entries == contract section 7 set; no duplicate/backslash/traversal/symlink)"
$summary.checks["R5_zip_hash_readback"] = "PASS (every entry re-hashed from the container against files{}/checksums)"

# ------------------------------------------------------- stage 7: sidecars
Write-LFText (Join-Path $OutDir "SHA256SUMS") ("$zipSha  $pkgRoot.zip`n")
$summary.container.zip = $zipPath
$summary.container.zip_sha256 = $zipSha
$summary.container.zip_bytes = (Get-Item $zipPath).Length
$summary.container.entry_count = 14
$summary.container.sha256sums_sidecar = (Join-Path $OutDir "SHA256SUMS")
$summary.ok = $true
Save-Summary
Write-Output ("PACKAGE_OK zip=$zipPath sha256=$zipSha bytes=" + (Get-Item $zipPath).Length)
exit 0
