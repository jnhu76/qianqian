param(
    [Parameter(Mandatory=$true)][ValidatePattern('^[0-9a-f]{40}$')][string]$SourceCommit,
    [string]$ProjectRoot = "",
    [string]$Distro = "Ubuntu-24.04",
    [string]$WslWorkRoot = "/home/hoo/q172rc",
    [Parameter(Mandatory=$true)][string]$ClosureSource
)
# This is an existing-closure build verifier, not a closure derivation entry.
# It fails closed unless the caller supplies the already imported closure.
$ErrorActionPreference = "Stop"
if ($ProjectRoot -eq "") { $ProjectRoot = Split-Path -Parent $PSScriptRoot }
$ProjectRoot = (Resolve-Path $ProjectRoot).Path
$build = Join-Path $PSScriptRoot "build-rc.ps1"
& $build -ProjectRoot $ProjectRoot -SourceCommit $SourceCommit -Distro $Distro `
    -WslWorkRoot $WslWorkRoot -ClosureSource $ClosureSource
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
exit 0
