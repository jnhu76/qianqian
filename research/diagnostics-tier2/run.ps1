param(
  [Parameter(Mandatory=$true)][string]$Exe,
  [Parameter(Mandatory=$true)][string]$Filter,
  [Parameter(Mandatory=$true)][string]$Outlet,
  [Parameter(Mandatory=$true)][string]$Mode,
  [Parameter(Mandatory=$true)][string]$Log,
  [int]$DeadlineSeconds = 20,
  [switch]$SkipClose
)
$ErrorActionPreference = 'Stop'
$lines = [System.Collections.Generic.List[string]]::new()
$lines.Add("UTC=$([DateTime]::UtcNow.ToString('o'))")
$lines.Add("EXE=$Exe SHA256=$((Get-FileHash -LiteralPath $Exe).Hash)")
$lines.Add("FILTER=$Filter OUTLET=$Outlet MODE=$Mode SKIP_CLOSE=$SkipClose")
$psi = [Diagnostics.ProcessStartInfo]::new()
$psi.FileName = (Resolve-Path -LiteralPath $Exe).Path
$psi.Arguments = "--exact $Filter --nocapture --test-threads=1"
$psi.UseShellExecute = $false
$psi.CreateNoWindow = $true
$psi.RedirectStandardInput = $true
$psi.RedirectStandardOutput = $true
$psi.RedirectStandardError = $true
$psi.Environment['Q296_MODE'] = $Mode
$psi.Environment['Q296_OUTLET'] = $Outlet
$psi.Environment['QIANQIAN_AUDIO_LOG'] = '1'
$psi.Environment['RUST_BACKTRACE'] = '0'
$failed = $false
$p = $null
$errorTask = $null
$outTask = $null
$timer = [Diagnostics.Stopwatch]::StartNew()
try {
  $p = [Diagnostics.Process]::Start($psi)
  $outTask = $p.StandardOutput.ReadToEndAsync()
  if ($Mode -eq 'broken' -and -not $SkipClose) {
    $p.StandardError.Close()
    $lines.Add('PARENT_CLOSE -> stdin GO (child corroborates write Err)')
  } elseif ($Mode -ne 'blocked') {
    $errorTask = $p.StandardError.ReadToEndAsync()
  }
  if ($Mode -eq 'blocked') { $lines.Add('CONTROLLED_STDERR_LOCK: child acquisition channel before work; pipe open and unread; no OS-full claim') }
  $p.StandardInput.WriteLine('GO')
  $p.StandardInput.Flush()
  if (-not $p.WaitForExit($DeadlineSeconds * 1000)) {
    $failed = $true
    $lines.Add('WATCHDOG_RED timeout; process killed; never PASS')
    $p.Kill($true)
    if (-not $p.WaitForExit(5000)) { throw 'CLEANUP process did not exit after Kill' }
  }
  if ($p.ExitCode -ne 0) { $failed = $true }
  $lines.Add("EXIT=$($p.ExitCode) ELAPSED_MS=$($timer.ElapsedMilliseconds)")
} catch {
  $failed = $true
  $lines.Add("HARNESS_RED $_")
  if ($null -ne $p -and -not $p.HasExited) { $p.Kill($true); [void]$p.WaitForExit(5000) }
} finally {
  if ($null -ne $outTask -and $outTask.Wait(5000)) { $lines.Add("STDOUT`n$($outTask.Result)") }
  if ($null -ne $errorTask -and $errorTask.Wait(5000)) { $lines.Add("STDERR`n$($errorTask.Result)") }
  $text = $lines -join "`n"
  $expected = if ($Outlet -eq "previous") { "Q296-KNOWN-RESIDUAL" } else { "Q296-PASS" }
  if (-not $text.Contains("running 1 test") -or -not $text.Contains($expected)) { $failed=$true; $lines.Add("ORACLE_RED missing exact execution/result marker") }
  $lines.Add("RUNNER_RESULT=" + $(if ($failed) { "RED" } else { "GREEN" }))
  $lines | Set-Content -LiteralPath $Log -Encoding utf8
  if ($null -ne $p) { $p.Dispose() }
}
$lines | Write-Output

if ($failed) { exit 1 }

