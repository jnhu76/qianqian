# run-binding-gate.ps1 -- #172 G3 / BINDING_RUNTIME canonical binding-gate
# entry (windows-x86_64 release lane).
#
# Why this script exists (structural, recorded in evidence/songcore-172/
# verdicts/s2-binding.md section 1): the two SongCore binding crates are
# excluded from the main workspace by committed design (their Cargo.toml
# comment blocks: they bind the separately-built SongCore native artifact
# and "compile as their own unit with this lockfile"), so
#   cargo test -p qianqian-songcore-sys -p qianqian-decode-songcore
# cannot resolve -p from the repo root (exit 101, "did not match any
# packages"). Independently, the host-default x86_64-pc-windows-msvc
# target cannot LINK the mingw-w64 libsongcore.a (12 unique LNK2019:
# __mingw_*/gettimeofday/usleep/mkstemp/fdopen/sincosf/isatty — mingw CRT
# extensions absent from MSVC/UCRT; linking mingw runtime libs would be a
# new non-system runtime dependency, forbidden without a reviewed contract
# update, docs/release/songcore-release-contract.md section 9). The
# canonical invocation is therefore: run INSIDE the decode crate's own
# workspace, under the x86_64-pc-windows-gnu target, with the mingw
# toolchain first on PATH and the mingw-flavored ar selected. This is the
# same environment family the C suite (ops/run-csuite.ps1) and #324 used.
#
# What it does: preflight (artifact + ABI header identity, mingw toolchain
# presence) -> cd crates/qianqian-decode-songcore -> cargo test -p
# qianqian-songcore-sys -p qianqian-decode-songcore --target
# x86_64-pc-windows-gnu -> propagate cargo's REAL exit code (a failing test
# is never masked), writing the full stdout+stderr receipt under
# evidence/songcore-172/binding-parity/ and printing a concise summary.
#
# Exit codes: 0 all tests passed; 10 preflight/tool failure; 11 frozen
# identity mismatch; cargo's own exit code otherwise.
#
# Usage (Windows PowerShell 5.1+):
#   powershell -ExecutionPolicy Bypass -File ops\run-binding-gate.ps1
param(
    [string]$ProjectRoot = "",
    [Parameter(Mandatory=$true)][string]$BuildReceipt,
    [string]$Cargo = "cargo",
    [string]$ReceiptDir = ""
)

$ErrorActionPreference = "Stop"

if ($ProjectRoot -eq "") { $ProjectRoot = Split-Path -Parent $PSScriptRoot }
$ProjectRoot = (Resolve-Path $ProjectRoot).Path
if ($ReceiptDir -eq "") { $ReceiptDir = Join-Path $ProjectRoot "evidence\songcore-172\binding-parity" }

$ArtifactPath = Join-Path $ProjectRoot "native\build\artifacts\libsongcore.a"
$HeaderPath = Join-Path $ProjectRoot "native\include\songcore.h"
$CrateDir = Join-Path $ProjectRoot "crates\qianqian-decode-songcore"
$build = Get-Content $BuildReceipt -Raw | ConvertFrom-Json
if ($build.script -ne "ops/build-rc.ps1" -or -not $build.ok) { Write-Host "ABORT exit=10 invalid build receipt"; exit 10 }
$ExpectedArtifactSha256 = $build.artifacts.libsongcore_a_sha256
if ($build.source.commit -ne (& git -C $ProjectRoot rev-parse HEAD)) { Write-Host "ABORT exit=11 build receipt source mismatch"; exit 11 }
$ExpectedHeaderSha256 = "cf509a5c044735e4813642ac6683691242146e642342a8ac356e98fbd8e78536"

$stamp = Get-Date -Format "yyyyMMdd-HHmmss"
New-Item -ItemType Directory -Force -Path $ReceiptDir | Out-Null
$receiptPath = Join-Path $ReceiptDir ("gate-" + $stamp + ".log")

$utf8 = New-Object System.Text.UTF8Encoding($false)
function Write-LFText([string]$path, [string]$text) {
    [System.IO.File]::WriteAllText($path, $text.Replace("`r`n", "`n"), $utf8)
}

function Sha256File([string]$path) {
    return (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant()
}

# --- preflight (fail-closed) ----------------------------------------------
if (-not (Test-Path $ArtifactPath)) {
    Write-Host "ABORT exit=10 artifact missing: $ArtifactPath"
    exit 10
}
$artifactSha = Sha256File $ArtifactPath
if ($artifactSha -ne $ExpectedArtifactSha256) {
    Write-Host ("ABORT exit=11 artifact sha mismatch: got " + $artifactSha + " want " + $ExpectedArtifactSha256)
    exit 11
}
if (-not (Test-Path $HeaderPath)) {
    Write-Host "ABORT exit=10 header missing: $HeaderPath"
    exit 10
}
$headerSha = Sha256File $HeaderPath
if ($headerSha -ne $ExpectedHeaderSha256) {
    Write-Host ("ABORT exit=11 header sha mismatch: got " + $headerSha + " want " + $ExpectedHeaderSha256)
    exit 11
}

# mingw gcc (the configured windows-gnu linker): PATH first, then the
# documented WinLibs host install (same resolution order as run-csuite.ps1)
$mingwBin = $null
if (Get-Command x86_64-w64-mingw32-gcc -ErrorAction SilentlyContinue) {
    $mingwBin = (Get-Command x86_64-w64-mingw32-gcc).Source
} elseif (Test-Path "C:\Users\fred1\tools\mingw64\bin\x86_64-w64-mingw32-gcc.exe") {
    $mingwBin = "C:\Users\fred1\tools\mingw64\bin\x86_64-w64-mingw32-gcc.exe"
} else {
    Write-Host "ABORT exit=10 no x86_64-w64-mingw32-gcc on PATH and no documented host install"
    exit 10
}
$mingwDir = Split-Path -Parent $mingwBin
$env:PATH = "$mingwDir;$env:PATH"
# rustc's windows-gnu runtime references BCrypt* (see .cargo/config.toml);
# the mingw-flavored ar must be selected for the target (WinLibs ships
# gcc-ar; WORKBRIEF/324 precedent)
$env:CARGO_TARGET_X86_64_PC_WINDOWS_GNU_AR = "x86_64-w64-mingw32-gcc-ar"

$gccVersion = (& $mingwBin --version | Select-Object -First 1)
$cargoVersion = (& $Cargo --version | Select-Object -First 1)
Write-Host "ARTIFACT sha256 $artifactSha"
Write-Host "HEADER   sha256 $headerSha"
Write-Host "MINGW    $gccVersion"
Write-Host "CARGO    $cargoVersion"
Write-Host "TARGET   x86_64-pc-windows-gnu (crate workspace: $CrateDir)"

# --- the gate (real exit code, full receipt) -------------------------------
$lines = New-Object System.Collections.Generic.List[string]
$lines.Add("canonical binding gate: ops/run-binding-gate.ps1")
$lines.Add("artifact: $ArtifactPath sha256 $artifactSha")
$lines.Add("header:   $HeaderPath sha256 $headerSha")
$lines.Add("mingw:    $gccVersion")
$lines.Add("cargo:    $cargoVersion")
$lines.Add("command:  cargo test -p qianqian-songcore-sys -p qianqian-decode-songcore --target x86_64-pc-windows-gnu (cwd $CrateDir)")
$lines.Add("")

$psi = New-Object System.Diagnostics.ProcessStartInfo
$psi.FileName = $Cargo
$psi.Arguments = 'test -p qianqian-songcore-sys -p qianqian-decode-songcore --target x86_64-pc-windows-gnu'
$psi.WorkingDirectory = $CrateDir
$psi.UseShellExecute = $false
$psi.RedirectStandardOutput = $true
$psi.RedirectStandardError = $true
$psi.StandardOutputEncoding = [System.Text.Encoding]::UTF8
$psi.StandardErrorEncoding = [System.Text.Encoding]::UTF8
$p = [System.Diagnostics.Process]::Start($psi)
$outTask = $p.StandardOutput.ReadToEndAsync()
$errTask = $p.StandardError.ReadToEndAsync()
if (-not $p.WaitForExit(900000)) {
    try { $p.Kill() } catch { }
    $p.WaitForExit() | Out-Null
    $lines.Add("TIMEOUT after 900s")
    $lines.Add($outTask.Result)
    $lines.Add($errTask.Result)
    Write-LFText $receiptPath ($lines -join "`n")
    Write-Host "GATE FAIL exit=-1 (timeout); receipt: $receiptPath"
    exit -1
}
$out = $outTask.Result
$err = $errTask.Result
$exitCode = $p.ExitCode
$lines.Add("--- stdout (exit=$exitCode) ---")
$lines.Add($out)
if ($err.Trim().Length -gt 0) {
    $lines.Add("--- stderr ---")
    $lines.Add($err)
}
Write-LFText $receiptPath ($lines -join "`n")

# concise stdout: the per-target test results + verdict only
$out -split "`n" | Where-Object { $_ -match "^(test result|     Running|   Doc-tests|error)" } | ForEach-Object { Write-Host $_.TrimEnd() }
if ($exitCode -eq 0) {
    Write-Host "GATE PASS exit=0; receipt: $receiptPath"
} else {
    Write-Host "GATE FAIL exit=$exitCode; receipt: $receiptPath"
}
exit $exitCode
