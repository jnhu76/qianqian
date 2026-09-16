# F1 physical gate: real SongCore decode + real WASAPI render + real CLI stop.
# Runs the SCRIPTABLE transport (--machine play): redirected pipes cannot
# drive the interactive terminal shell, which `play` opens since the
# reference-player slice.
# Stop section: start `--machine play`, wait for the activation witness
# line, let the device enter steady rendering, send `stop` through stdin,
# require exit 0 with the Stopped outcome and quiet disposal.
# EOF section: play to natural end with stdin closed immediately; require
# exit 0 with the Completed outcome - F1 must not confuse EOF with stop.
param(
    [int]$StopIterations = 20,
    [int]$EofIterations = 10,
    [string]$Exe = "C:\qianqian-gate\f1\target\x86_64-pc-windows-gnu\release\qianqian-headless.exe",
    [string]$Fixture = "C:\qianqian-gate\f1\native\experiments\songcore-equivalence\fixtures\alac-long.m4a"
)
$ErrorActionPreference = "Stop"

function Run-Episode([string]$StdinLine, [bool]$SendStop, [string]$ExpectMatch) {
    $psi = New-Object System.Diagnostics.ProcessStartInfo
    $psi.FileName = $Exe
    $psi.Arguments = "--machine play `"$Fixture`""
    $psi.RedirectStandardInput = $true
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.UseShellExecute = $false
    $p = [System.Diagnostics.Process]::Start($psi)
    # Async drains from launch: a child blocked on a full stdout/stderr
    # pipe can neither process 'stop' nor exit, which would surface as a
    # fake watchdog timeout. Neither stream may be read only at the end.
    $errTask = $p.StandardError.ReadToEndAsync()
    if ($SendStop) {
        $witness = $false
        while ($null -ne ($line = $p.StandardOutput.ReadLine())) {
            if ($line.StartsWith("playing ")) { $witness = $true; break }
        }
        if (-not $witness) {
            $p.StandardInput.WriteLine('quit-never-wired')
            return @{ Ok = $false; Why = "no playing witness"; Exit = -1 }
        }
        # Witness consumed synchronously; drain the remainder without
        # blocking (sync reads are complete before the async begins).
        $outTask = $p.StandardOutput.ReadToEndAsync()
        Start-Sleep -Milliseconds 400
        $p.StandardInput.WriteLine($StdinLine)
    } else {
        $p.StandardInput.Close()
        $outTask = $p.StandardOutput.ReadToEndAsync()
    }
    if (-not $p.WaitForExit(20000)) {
        $p.Kill()
        return @{ Ok = $false; Why = "timeout"; Exit = -1 }
    }
    $out = $outTask.GetAwaiter().GetResult()
    $err = $errTask.GetAwaiter().GetResult()
    # 'render aborted: data plane stopped' is the EXPECTED one-shot stop
    # diagnostic on the stopped path; a real failure says failed/error
    # and a latched teardown violation says warning:.
    $ok = ($p.ExitCode -eq 0) -and ($out -match $ExpectMatch) -and
          ($err -notmatch 'warning:') -and
          ($err -notmatch 'failed') -and ($err -notmatch 'error')
    return @{ Ok = $ok; Why = "exit=$($p.ExitCode) out=$($out -replace "`n",' | ') err=$($err -replace "`n",' | ')"; Exit = $p.ExitCode }
}

$stopPass = 0; $stopFail = 0
for ($i = 1; $i -le $StopIterations; $i++) {
    $r = Run-Episode 'stop' $true 'stopped before completion'
    if ($r.Ok) { $stopPass++ } else { $stopFail++; Write-Output "STOP[$i] FAIL: $($r.Why)" }
}
Write-Output "STOP GATE: $stopPass/$StopIterations passed, $stopFail failed"

$eofPass = 0; $eofFail = 0
for ($i = 1; $i -le $EofIterations; $i++) {
    $r = Run-Episode '' $false 'EOF: played out completely'
    if ($r.Ok) { $eofPass++ } else { $eofFail++; Write-Output "EOF[$i] FAIL: $($r.Why)" }
}
Write-Output "EOF GATE: $eofPass/$EofIterations passed, $eofFail failed"
