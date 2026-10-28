# run_cell.ps1 - #324 Windows native Tier-2 cell runner.
#
# Spawns ONE fresh child process per fault cell, attaches an independent
# anonymous pipe to its stderr, and records a receipt. The runner is a witness
# and a cleanup watchdog, NEVER the correctness oracle: every predicate is
# asserted inside the child.
#
#   -Mode drain   : the parent reads the child's stderr concurrently. Used where
#                   a healthy real sink must stay usable (seek C1 / C4 healthy).
#   -Mode ignore  : the parent NEVER reads stderr. While the child runs it only
#                   PEEKS the queue (non-destructive, PeekNamedPipe) and records
#                   the maximum occupancy it saw; the child never gets data back.
#
# Env file format: one `KEY=VALUE` per line; `KEY=__UNSET__` removes the
# variable from the child environment.
param(
  [Parameter(Mandatory=$true)][string]$Exe,
  [Parameter(Mandatory=$true)][string]$Cell,
  [string]$Test = "",
  [string]$Mode = "ignore",
  [string]$EnvFile = "",
  [string]$TimeoutSec = "120",
  [string]$OutFile = "",
  [switch]$Ignored
)

if (-not ("Q324.Native" -as [type])) {
  Add-Type -Namespace Q324 -Name Native -MemberDefinition @'
[DllImport("kernel32.dll", SetLastError=true)]
public static extern bool PeekNamedPipe(IntPtr h, IntPtr buf, uint blen, IntPtr bread, out uint avail, out uint left);
'@
}

$envPairs = @()
if ($EnvFile -ne "") {
  Get-Content -LiteralPath $EnvFile | ForEach-Object {
    if ($_ -match '^([^=]+)=(.*)$') { $envPairs += ,@($matches[1], $matches[2]) }
  }
}

$psi = New-Object System.Diagnostics.ProcessStartInfo
$psi.FileName = $Exe
if ($Test -ne "") {
  if ($Ignored) {
    $psi.Arguments = "--ignored --exact $Test --nocapture --test-threads=1"
  } else {
    $psi.Arguments = "--exact $Test --nocapture --test-threads=1"
  }
} else {
  $psi.Arguments = "--nocapture --test-threads=1"
}
$psi.UseShellExecute = $false
$psi.RedirectStandardOutput = $true
$psi.RedirectStandardError = $true
$psi.StandardOutputEncoding = [System.Text.Encoding]::UTF8
$psi.StandardErrorEncoding = [System.Text.Encoding]::UTF8
foreach ($pair in $envPairs) {
  if ($pair[1] -eq "__UNSET__") { $psi.EnvironmentVariables.Remove($pair[0]) | Out-Null }
  else { $psi.EnvironmentVariables[$pair[0]] = $pair[1] }
}
$psi.EnvironmentVariables["RUST_BACKTRACE"] = "1"

$exeSha = (Get-FileHash -LiteralPath $Exe -Algorithm SHA256).Hash.ToLowerInvariant()
$sw = [System.Diagnostics.Stopwatch]::StartNew()
$p = [System.Diagnostics.Process]::Start($psi)
$stdoutTask = $p.StandardOutput.ReadToEndAsync()
$stderrTask = $null
$stderrHandle = [IntPtr]::Zero
if ($Mode -eq "drain") {
  $stderrTask = $p.StandardError.ReadToEndAsync()
} else {
  try { $stderrHandle = $p.StandardError.BaseStream.SafeFileHandle.DangerousGetHandle() } catch { $stderrHandle = [IntPtr]::Zero }
}

# Wait with a watchdog, peeking (never reading) the stderr queue while alive.
$timedOut = $false
$peekOk = 0
$peekTotal = 0
$maxAvail = 0
$deadline = [DateTime]::UtcNow.AddSeconds([int]$TimeoutSec)
while (-not $p.HasExited) {
  if ([DateTime]::UtcNow -gt $deadline) {
    $timedOut = $true
    try { $p.Kill() } catch {}
    break
  }
  if ($Mode -ne "drain" -and $stderrHandle -ne [IntPtr]::Zero) {
    $avail = 0; $left = 0
    $peekTotal++
    if ([Q324.Native]::PeekNamedPipe($stderrHandle, [IntPtr]::Zero, 0, [IntPtr]::Zero, [ref]$avail, [ref]$left)) {
      $peekOk++
      if ($avail -gt $maxAvail) { $maxAvail = $avail }
    }
  }
  Start-Sleep -Milliseconds 50
}
$p.WaitForExit()
$sw.Stop()

$stdout = $stdoutTask.Result
$stderr = ""
if ($Mode -eq "drain" -and $null -ne $stderrTask) { $stderr = $stderrTask.Result }
$peekText = if ($Mode -eq "drain") { "drained_by_parent" } else { "peek_ok=$peekOk/$peekTotal max_avail=$maxAvail never_read=true" }

$envText = ($envPairs | ForEach-Object { "$($_[0])=$($_[1])" }) -join ";"
$lines = @(
  "===CELL_BEGIN===",
  "cell=$Cell",
  "test=$Test",
  "mode=$Mode",
  "exe=$Exe",
  "exe_sha256=$exeSha",
  "env=$envText",
  "exit_code=$($p.ExitCode)",
  "timed_out=$timedOut",
  "duration_ms=$($sw.ElapsedMilliseconds)",
  "stderr_mode=$Mode",
  "stderr_peek=$peekText",
  "--- stdout ---",
  $stdout,
  "--- stderr ---",
  $stderr,
  "===CELL_END==="
)
$receipt = $lines -join "`n"
if ($OutFile -ne "") { Set-Content -LiteralPath $OutFile -Value $receipt -Encoding utf8 }
Write-Output $receipt
