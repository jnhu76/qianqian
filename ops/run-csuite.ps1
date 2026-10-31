# run-csuite.ps1 -- #172 G2 / CORE_RUNTIME Common Formats parity suite runner
# (windows-x86_64 release lane).
#
# Single entry: preflight (toolchain, frozen artifact + ABI header identity,
# canonical corpus identity) -> compile the direct C consumer and the canonical
# layout probe against native/include/songcore.h with the EXPLICIT windows
# static link contract (evidence/songcore-172/link-contract.md section 2:
#   gcc ... <libsongcore.a> -lbcrypt   -- nothing else)
# -> construct the frozen #296/#324 malformed-media copies from the
# identity-verified FLAC fixture (readfault byte 76000 ^= 0x01; seekfault
# 73502/77042/80375 ^= 0x01; truncated first 100000 bytes, sha256-checked
# against the frozen 0aa66168... identity) -> execute the S-BIND section 7.1
# parity scenario per corpus fixture plus the malformed/argument/lifetime
# arms -> write per-case receipts + one machine-readable summary.json under
# evidence/songcore-172/suite-results/<stamp>/ and print a small summary on
# stdout (< 64KB).
#
# Reference discipline (S-BIND section 7.3 / release contract section 11 G2):
# NO second corpus and NO second reference numbers. Every expected value is
# read at run time from the canonical frozen files
#   native/experiments/songcore-equivalence/reference.json          (frames, rate, channels, pcm sha, fixture sha)
#   native/experiments/songcore-equivalence/results/songcore-equivalence.json (frozen Linux direct-C source facts: container, duration_us)
# and passed to the child on argv. Every predicate is asserted INSIDE the
# child (the runner is a witness, same discipline as the #324 cell runner).
# The child exits nonzero on any failed predicate; a missing RESULT line or a
# nonzero exit is a FAIL here. Fail-closed throughout: missing tools, artifact
# or corpus identity drift, compile errors, construction mismatches and any
# case failure abort with a nonzero exit.
#
# The frozen PCM comparison policy is the corpus's own: exact SHA-256 of the
# full-drain Float32 PCM against reference.json pcm_sha256 for ALL fixtures
# (the lossy mp3 fixture's frozen policy is the same deterministic hash — no
# numeric lossy tolerance exists anywhere in the corpus and none is invented
# here). For lossy codecs the suite additionally does NOT assert post-seek
# PCM identity (no frozen tolerance; recorded instead). For lossless codecs
# the post-seek drain must byte-match the sequential slice at the landing
# frame (songcore.h: "for lossless formats the landing is frame-accurate").
#
# Exit codes: 0 all cases passed; 10 preflight/tool failure; 11 frozen
# identity mismatch (artifact/header/corpus/construction); 12 corpus file
# missing; 13 compile/link failure; 1 one or more cases FAILED.
#
# Usage (Windows PowerShell 5.1+):
#   powershell -ExecutionPolicy Bypass -File ops\run-csuite.ps1
#   powershell ... -Gcc C:\Users\fred1\tools\mingw64\bin\gcc.exe
# The gcc resolution order is: -Gcc, then PATH, then the documented WinLibs
# host install C:\Users\fred1\tools\mingw64\bin\gcc.exe. mingw gcc must be
# able to see the artifact; nothing else is required (no cargo, no WSL).
param(
    [string]$ProjectRoot = "",
    [string]$ArtifactPath = "",
    [Parameter(Mandatory=$true)][string]$BuildReceipt,
    [string]$ExpectedArtifactSha256 = "",
    [string]$ExpectedHeaderSha256 = "cf509a5c044735e4813642ac6683691242146e642342a8ac356e98fbd8e78536",
    [string]$Gcc = "",
    [string]$ResultsRoot = ""
)

$ErrorActionPreference = "Stop"

if ($ProjectRoot -eq "") { $ProjectRoot = Split-Path -Parent $PSScriptRoot }
$ProjectRoot = (Resolve-Path $ProjectRoot).Path
if ($ArtifactPath -eq "") { $ArtifactPath = Join-Path $ProjectRoot "native\build\artifacts\libsongcore.a" }
if (-not (Test-Path $BuildReceipt)) { Write-Host "ABORT exit=10 build receipt missing: $BuildReceipt"; exit 10 }
$build = Get-Content $BuildReceipt -Raw | ConvertFrom-Json
if ($build.script -ne "ops/build-rc.ps1" -or -not $build.ok) { Write-Host "ABORT exit=11 invalid build receipt"; exit 11 }
$ExpectedArtifactSha256 = $build.artifacts.libsongcore_a_sha256
if ($build.source.commit -ne (& git -C $ProjectRoot rev-parse HEAD)) { Write-Host "ABORT exit=11 build receipt source mismatch"; exit 11 }
if ($ResultsRoot -eq "") { $ResultsRoot = Join-Path $ProjectRoot "evidence\songcore-172\suite-results" }

$HeaderPath = Join-Path $ProjectRoot "native\include\songcore.h"
$CorpusDir = Join-Path $ProjectRoot "native\experiments\songcore-equivalence\fixtures"
$ReferenceJson = Join-Path $ProjectRoot "native\experiments\songcore-equivalence\reference.json"
$EquivalenceJson = Join-Path $ProjectRoot "native\experiments\songcore-equivalence\results\songcore-equivalence.json"
$LayoutProbeC = Join-Path $ProjectRoot "experiments\songcore-call-comparison\c\layout_probe.c"
$LayoutFrozen = Join-Path $ProjectRoot "experiments\songcore-call-comparison\results\layout-c.txt"
$ConsumerC = Join-Path $ProjectRoot "evidence\songcore-172\suite\csuite_consumer.c"

$stamp = Get-Date -Format "yyyyMMdd-HHmmss"
$evDir = Join-Path $ResultsRoot $stamp
New-Item -ItemType Directory -Force -Path $evDir | Out-Null
$mediaDir = Join-Path $evDir "media"
New-Item -ItemType Directory -Force -Path $mediaDir | Out-Null

$utf8 = New-Object System.Text.UTF8Encoding($false)
function Write-LFText([string]$path, [string]$text) {
    [System.IO.File]::WriteAllText($path, $text.Replace("`r`n", "`n"), $utf8)
}

# --- child execution (UTF-8 decode discipline per #324 run_cell.ps1) -------
function Run-Child([string]$exe, [string[]]$childArgs, [string]$receiptPath,
                   [int]$timeoutSec = 180) {
    $psi = New-Object System.Diagnostics.ProcessStartInfo
    $psi.FileName = $exe
    $psi.Arguments = ($childArgs | ForEach-Object { '"' + ($_ -replace '"', '\"') + '"' }) -join ' '
    $psi.UseShellExecute = $false
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.StandardOutputEncoding = [System.Text.Encoding]::UTF8
    $psi.StandardErrorEncoding = [System.Text.Encoding]::UTF8
    $p = [System.Diagnostics.Process]::Start($psi)
    $stdoutTask = $p.StandardOutput.ReadToEndAsync()
    $stderrTask = $p.StandardError.ReadToEndAsync()
    if (-not $p.WaitForExit($timeoutSec * 1000)) {
        try { $p.Kill() } catch { }
        $p.WaitForExit() | Out-Null
        $out = $stdoutTask.Result; $err = $stderrTask.Result
        Write-LFText $receiptPath ("TIMEOUT after ${timeoutSec}s`n--- stdout ---`n" + $out + "`n--- stderr ---`n" + $err)
        return @{ ExitCode = -1; Stdout = $out; Stderr = $err; TimedOut = $true }
    }
    $out = $stdoutTask.Result; $err = $stderrTask.Result
    Write-LFText $receiptPath ("exit=" + $p.ExitCode + "`n--- stdout ---`n" + $out + "`n--- stderr ---`n" + $err)
    return @{ ExitCode = $p.ExitCode; Stdout = $out; Stderr = $err; TimedOut = $false }
}

function Sha256File([string]$path) {
    return (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant()
}

$caseResults = New-Object System.Collections.Generic.List[object]
function Add-Case([string]$id, [string]$verdict, [hashtable]$detail) {
    $row = @{ id = $id; result = $verdict }
    foreach ($k in $detail.Keys) { $row[$k] = $detail[$k] }
    $script:caseResults.Add([pscustomobject]$row)
    if ($verdict -eq "PASS") { Write-Host ("[PASS] " + $id) }
    else { Write-Host ("[FAIL] " + $id) }
}

function Abort([int]$code, [string]$why) {
    Write-Host ("ABORT exit=" + $code + " " + $why)
    # emit the summary even for aborts so the evidence dir explains itself
    $s = [pscustomobject]@{
        schema = "songcore-csuite-summary/1"; stamp = $stamp
        verdict = "FAIL"; abort = $why; exit_code = $code
        artifact = $ArtifactPath; artifact_sha256 = $ExpectedArtifactSha256
        cases = $script:caseResults
    }
    Write-LFText (Join-Path $evDir "summary.json") ($s | ConvertTo-Json -Depth 6)
    exit $code
}

# ===========================================================================
# 1. Preflight: toolchain + frozen identities
# ===========================================================================
if ($Gcc -ne "") {
    if (-not (Test-Path $Gcc)) { Abort 10 "gcc not found at -Gcc path: $Gcc" }
    $gccExe = (Resolve-Path $Gcc).Path
} elseif (Get-Command gcc -ErrorAction SilentlyContinue) {
    $gccExe = (Get-Command gcc).Source
} elseif (Test-Path "C:\Users\fred1\tools\mingw64\bin\gcc.exe") {
    $gccExe = "C:\Users\fred1\tools\mingw64\bin\gcc.exe"
    Write-Host "NOTE: gcc not on PATH; using documented WinLibs host install"
} else {
    Abort 10 "no mingw gcc on PATH and no documented host install"
}
$gccVersion = (& $gccExe --version | Select-Object -First 1)
Write-Host ("GCC " + $gccVersion)

if (-not (Test-Path $ArtifactPath)) { Abort 10 "artifact missing: $ArtifactPath" }
$artifactSha = Sha256File $ArtifactPath
if ($artifactSha -ne $ExpectedArtifactSha256) {
    Abort 11 ("artifact sha mismatch: got " + $artifactSha + " want " + $ExpectedArtifactSha256)
}
if (-not (Test-Path $HeaderPath)) { Abort 10 "header missing: $HeaderPath" }
$headerSha = Sha256File $HeaderPath
if ($headerSha -ne $ExpectedHeaderSha256) {
    Abort 11 ("header sha mismatch: got " + $headerSha + " want " + $ExpectedHeaderSha256)
}
Write-Host ("ARTIFACT sha256 " + $artifactSha)
Write-Host ("HEADER   sha256 " + $headerSha)

foreach ($f in @($ReferenceJson, $EquivalenceJson, $LayoutProbeC, $LayoutFrozen, $ConsumerC)) {
    if (-not (Test-Path $f)) { Abort 12 "canonical file missing: $f" }
}
$ref = Get-Content -LiteralPath $ReferenceJson -Raw | ConvertFrom-Json
$eq = Get-Content -LiteralPath $EquivalenceJson -Raw | ConvertFrom-Json

# ===========================================================================
# 2. Compile (explicit link contract: archive + -lbcrypt, nothing else)
# ===========================================================================
$consumerExe = Join-Path $evDir "csuite_consumer.exe"
$layoutExe = Join-Path $evDir "layout_probe.exe"
$compileLog = Join-Path $evDir "compile.log"
$logLines = New-Object System.Collections.Generic.List[string]
$logLines.Add("consumer link line: gcc -O2 -std=c11 -Wall -Wextra -I <native/include> <consumer.c> <libsongcore.a> -lbcrypt")

$incDir = Join-Path $ProjectRoot "native\include"
& $gccExe -O2 -std=c11 -Wall -Wextra -I $incDir $ConsumerC $ArtifactPath -lbcrypt -o $consumerExe 2>&1 | ForEach-Object { $logLines.Add("cc: $_") }
if ($LASTEXITCODE -ne 0) { Write-LFText $compileLog ($logLines -join "`n"); Abort 13 "consumer compile/link failed (exit $LASTEXITCODE)" }
$logLines.Add("consumer compile OK -> " + $consumerExe)

& $gccExe -O2 -std=c11 -Wall -Wextra -I $incDir $LayoutProbeC -o $layoutExe 2>&1 | ForEach-Object { $logLines.Add("cc: $_") }
if ($LASTEXITCODE -ne 0) { Write-LFText $compileLog ($logLines -join "`n"); Abort 13 "layout probe compile failed (exit $LASTEXITCODE)" }
$logLines.Add("layout probe compile OK -> " + $layoutExe)
Write-LFText $compileLog ($logLines -join "`n")
Write-Host "COMPILE OK (link contract: libsongcore.a -lbcrypt)"

# ===========================================================================
# 3. ABI/layout gate: canonical probe output vs the frozen committed output
# ===========================================================================
$layoutReceipt = Run-Child $layoutExe @() (Join-Path $evDir "abi-layout-frozen.log")
$layoutNow = Join-Path $evDir "layout-now.txt"
Write-LFText $layoutNow $layoutReceipt.Stdout
$frozenText = [System.IO.File]::ReadAllText($LayoutFrozen).Replace("`r`n", "`n")
$nowText = $layoutReceipt.Stdout.Replace("`r`n", "`n")
$layoutOk = ($layoutReceipt.ExitCode -eq 0) -and ($frozenText -eq $nowText)
$frozenLines = ($frozenText -split "`n").Count
if ($layoutOk) { Add-Case "abi-layout-frozen" "PASS" @{ lines = $frozenLines; frozen = $LayoutFrozen } }
else { Add-Case "abi-layout-frozen" "FAIL" @{ exit = $layoutReceipt.ExitCode; frozen = $LayoutFrozen } }

# ===========================================================================
# 4. Corpus identity + frozen malformed-media construction
# ===========================================================================
$fixtures = @{}
foreach ($fx in $ref.fixtures) {
    $path = Join-Path $CorpusDir $fx.file
    if (-not (Test-Path $path)) { Abort 12 ("corpus fixture missing: " + $fx.file) }
    $sha = Sha256File $path
    if ($sha -ne $fx.fixture_sha256) {
        Abort 11 ("fixture identity drift: " + $fx.file + " got " + $sha + " want " + $fx.fixture_sha256)
    }
    $observed = $eq.correctness.($fx.id).observed
    if ($null -eq $observed) {
        Abort 12 ("no frozen Linux direct-C observation for fixture " + $fx.id)
    }
    $fixtures[$fx.id] = @{
        id = $fx.id; path = $path; file = $fx.file; codec = $fx.codec
        sample_rate = [int64]$fx.sample_rate; channels = [int64]$fx.channels
        pcm_frames = [uint64]$fx.pcm_frames; pcm_sha256 = $fx.pcm_sha256
        container = $observed.container; duration_us = [int64]$observed.duration_us
    }
    Write-Host ("CORPUS " + $fx.file + " sha256 " + $sha)
}

# frozen #296/#324 constructions (native-tier2-296 MATRIX.md + windows
# native-tier2-324 MATRIX.md; byte-exact, built from the identity-verified FLAC)
$flacBytes = [System.IO.File]::ReadAllBytes($fixtures["flac-16-44-stereo"].path)
$rf = [byte[]]$flacBytes.Clone(); $rf[76000] = $rf[76000] -bxor 0x01
$rfPath = Join-Path $mediaDir "readfault-76000.flac"
[System.IO.File]::WriteAllBytes($rfPath, $rf)
$sf = [byte[]]$flacBytes.Clone()
foreach ($off in @(73502, 77042, 80375)) { $sf[$off] = $sf[$off] -bxor 0x01 }
$sfPath = Join-Path $mediaDir "seekfault-73502-77042-80375.flac"
[System.IO.File]::WriteAllBytes($sfPath, $sf)
$tr = New-Object byte[] 100000
[Array]::Copy($flacBytes, $tr, 100000)
$trPath = Join-Path $mediaDir "truncated-100000.flac"
[System.IO.File]::WriteAllBytes($trPath, $tr)
$trSha = Sha256File $trPath
if ($trSha -ne "0aa66168770d5d36072dccf7fec728647717290f96bf13335775144f3438b39f") {
    Abort 11 ("truncated construction drift: got " + $trSha + " want 0aa66168... (frozen #296/#324 identity)")
}
$gbPath = Join-Path $mediaDir "garbage-nomedia.bin"
[System.IO.File]::WriteAllBytes($gbPath, [System.Text.Encoding]::ASCII.GetBytes("QIANQIAN-CSUITE this is not a media file at all 0123456789abcdef"))
$emPath = Join-Path $mediaDir "empty.bin"
[System.IO.File]::WriteAllBytes($emPath, [byte[]]@())
Write-Host ("CONSTRUCTED truncated-100000.flac sha256 " + $trSha + " (== frozen)")

# ===========================================================================
# 5. Run the suite
# ===========================================================================
function Invoke-Case([string]$id, [string[]]$childArgs) {
    $receipt = Join-Path $evDir ($id + ".log")
    $r = Run-Child $consumerExe $childArgs $receipt
    $stdout = $r.Stdout
    $subCases = ([regex]::Matches($stdout, "CASE \S+ (PASS|FAIL)")).Count
    $subFails = ([regex]::Matches($stdout, "CASE \S+ FAIL")).Count
    $m = [regex]::Match($stdout, "MODE \S+ CASES=(\d+) FAILED=(\d+) RESULT=(\w+)")
    $ok = ($r.ExitCode -eq 0) -and (-not $r.TimedOut) -and $m.Success -and ($m.Groups[3].Value -eq "PASS")
    $detail = @{ exit = $r.ExitCode; sub_cases = $subCases; sub_fails = $subFails; receipt = $receipt }
    if ($m.Success) { $detail.mode_cases = [int]$m.Groups[1].Value; $detail.mode_failed = [int]$m.Groups[2].Value }
    Add-Case $id ($(if ($ok) { "PASS" } else { "FAIL" })) $detail
}

# 5a. per-fixture parity (S-BIND 7.1 steps 01-11,13)
foreach ($id in @("mp3-cbr-id3v23", "flac-16-44-stereo", "alac-16-44-stereo", "alac-long")) {
    $fx = $fixtures[$id]
    $lossless = 0
    if ($fx.codec -in @("flac", "alac")) { $lossless = 1 }
    Invoke-Case ("parity." + $id) @(
        "parity", $fx.path, $fx.codec, $fx.container, "$($fx.duration_us)",
        "$($fx.sample_rate)", "$($fx.channels)", "$($fx.pcm_frames)",
        $fx.pcm_sha256, "$lossless")
}

# 5b. malformed / truncated arms (#296/#324 frozen constructions)
Invoke-Case "readfault-flac" @("readfault", $rfPath)
Invoke-Case "seekfault-flac" @("seekfault", $sfPath)
Invoke-Case "truncated-flac" @("truncated", $trPath, "$($fixtures['flac-16-44-stereo'].pcm_frames)")
Invoke-Case "garbage-nomedia" @("garbage", $gbPath)
Invoke-Case "garbage-empty" @("garbage", $emPath)

# 5c. stable status/error classes + lifetime arms
$flac = $fixtures["flac-16-44-stereo"]
Invoke-Case "argcheck-stable-classes" @("argcheck", $flac.path)
Invoke-Case "closecheck-reopen-stable" @("closecheck", $flac.path, "$($flac.pcm_frames)", $flac.pcm_sha256)

# ===========================================================================
# 6. Summary
# ===========================================================================
$passed = @($caseResults | Where-Object { $_.result -eq "PASS" }).Count
$failed = @($caseResults | Where-Object { $_.result -eq "FAIL" }).Count
$verdict = "PASS"; if ($failed -gt 0) { $verdict = "FAIL" }

$summary = [pscustomobject]@{
    schema = "songcore-csuite-summary/1"
    stamp = $stamp
    verdict = $verdict
    cases_total = $caseResults.Count
    cases_passed = $passed
    cases_failed = $failed
    artifact = $ArtifactPath
    artifact_sha256 = $artifactSha
    header = $HeaderPath
    header_sha256 = $headerSha
    reference_policy = "exact pcm_sha256 from native/experiments/songcore-equivalence/reference.json for ALL fixtures (the corpus's own frozen deterministic hash policy, lossy included); no numeric lossy tolerance exists in the corpus and none was invented; lossless post-seek drains byte-match the sequential slice at the landing frame"
    link_contract = "gcc -O2 -std=c11 <consumer.c> <libsongcore.a> -lbcrypt (evidence/songcore-172/link-contract.md section 2)"
    gcc = $gccVersion
    frozen_constructions = @{
        readfault = "flac-16-44-stereo byte 76000 ^= 0x01 (#296/#324 decode-read)"
        seekfault = "flac-16-44-stereo bytes 73502/77042/80375 ^= 0x01 (#296/#324 decode-seek)"
        truncated = "flac-16-44-stereo first 100000 bytes, sha256 0aa66168770d5d36072dccf7fec728647717290f96bf13335775144f3438b39f"
    }
    cases = $caseResults
}
$summaryPath = Join-Path $evDir "summary.json"
Write-LFText $summaryPath ($summary | ConvertTo-Json -Depth 6)

Write-Host ""
Write-Host ("SUITE " + $verdict + " cases=" + $caseResults.Count + " passed=" + $passed + " failed=" + $failed)
Write-Host ("SUMMARY " + $summaryPath)
$stdoutBytes = [System.Text.Encoding]::UTF8.GetByteCount(($caseResults | ForEach-Object { $_.id + "=" + $_.result }) -join "`n")
Write-Host ("STDOUT-SUMMARY-BYTES ~" + ($stdoutBytes + 512) + " (< 65536)")
if ($failed -gt 0) { exit 1 }
exit 0
