# run_sink_row.ps1 - #288 A4/E-7 tier-1 sink-control row runner.
# Spawns the child test binary with stderr attached to a pipe that this
# parent then closes (broken), never reads (blocked), or drains (healthy),
# and records exit code, duration, stdout rows and (healthy only) stderr.
param(
  [Parameter(Mandatory=$true)][string]$Exe,
  [Parameter(Mandatory=$true)][string]$Filter,
  [Parameter(Mandatory=$true)][string]$Mode,
  [string]$Gate = "1",
  [string]$Facility = "0",
  [string]$Hook = "",
  [string]$VolumeFault = "0",
  [string]$TimeoutSec = "90",
  [string]$ExtraArgs = ""
)
$psi = New-Object System.Diagnostics.ProcessStartInfo
$psi.FileName = $Exe
$psi.Arguments = "--test-threads 1 $Filter $ExtraArgs"
$psi.UseShellExecute = $false
$psi.RedirectStandardOutput = $true
$psi.RedirectStandardError = $true
$psi.EnvironmentVariables["RUST_BACKTRACE"] = "1"
if ($Gate -eq "1") { $psi.EnvironmentVariables["QIANQIAN_AUDIO_LOG"] = "1" }
  else { $psi.EnvironmentVariables.Remove("QIANQIAN_AUDIO_LOG") | Out-Null }
$psi.EnvironmentVariables["QIANQIAN_A4_MODE"] = $Mode
if ($Facility -eq "1") { $psi.EnvironmentVariables["QIANQIAN_A4_FACILITY"] = "1" }
if ($Hook -ne "") { $psi.EnvironmentVariables["QIANQIAN_A4_HOOK"] = $Hook }
if ($VolumeFault -eq "1") { $psi.EnvironmentVariables["QIANQIAN_A4_VOLUME_FAULT"] = "1" }

$sw = [System.Diagnostics.Stopwatch]::StartNew()
$p = [System.Diagnostics.Process]::Start($psi)
$stdoutTask = $p.StandardOutput.ReadToEndAsync()
$stderrTask = $null
switch ($Mode) {
  "broken"  { $p.StandardError.Close() }
  "blocked" { }   # never read: the child's own flood fills and parks it
  "healthy" { $stderrTask = $p.StandardError.ReadToEndAsync() }
}
$timedOut = $false
if (-not $p.WaitForExit([int]$TimeoutSec * 1000)) {
  $timedOut = $true
  try { $p.Kill() } catch {}
  $p.WaitForExit()
}
$sw.Stop()
$stdout = $stdoutTask.Result
$stderr = ""
if ($Mode -eq "healthy" -and $stderrTask -ne $null) { $stderr = $stderrTask.Result }

"===ROW_BEGIN==="
"exit_code=$($p.ExitCode)"
"duration_ms=$($sw.ElapsedMilliseconds)"
"timed_out=$timedOut"
"--- stdout ---"
$stdout
"--- stderr ---"
$stderr
"===ROW_END==="
