# Run from repository root on native Windows after building the two test bins.
# Mutations are temporary, sequential, byte-restored even on failure.
param([string]$Only = '')
$ErrorActionPreference='Stop'
$root=(Get-Location).Path
$raw=Join-Path $root 'evidence/diagnostics-tier2-296/raw'
$receipts=[Collections.Generic.List[object]]::new()
if ($Only -ne '' -and (Test-Path "$raw/mutations.json")) {
  foreach ($r in @(Get-Content "$raw/mutations.json" -Raw | ConvertFrom-Json)) {
    if ($r.id -ne $Only) { $receipts.Add($r) }
  }
}
function Run-Mutation([string]$id,[string]$file,[string]$old,[string]$replacement,[string[]]$sites,[string]$mode,[string]$signature) {
  if ($Only -ne '' -and $Only -ne $id) { return }
  $path=Join-Path $root $file
  $bytes=[IO.File]::ReadAllBytes($path)
  $hash=(Get-FileHash -LiteralPath $path).Hash
  $original=[IO.File]::ReadAllText($path)
  if (-not $original.Contains($old)) { throw "mutation $id anchor absent" }
  try {
    [IO.File]::WriteAllText($path,$original.Replace($old,$replacement),[Text.UTF8Encoding]::new($false))
    $bins=& "$root/research/diagnostics-tier2/build.ps1" -Log "$raw/$id-build.log"
    foreach($site in $sites) {
      $isWasapi=$site -in @('8','9','10')
      $exe=if($isWasapi){$bins.wasapi}else{$bins.headless}
      $filter=if($isWasapi){'tier2_tests::tier2_child'}else{'entry::tier2_tests::tier2_child'}
      $log="$raw/$id-$site.log"
      & "$root/research/diagnostics-tier2/run.ps1" -Exe $exe -Filter $filter -Outlet $site -Mode $mode -Log $log -DeadlineSeconds 8 | Out-Null
      $text=Get-Content -LiteralPath $log -Raw
      $rowSignature=$signature
      if($id -eq 'M3-drop-counter'){$rowSignature=if($site -eq 'panic'){'PANIC-REJECT-ORACLE'}else{'TARGET-REJECT-ORACLE'}}
      if($id -eq 'M10-self-report-counter'){$rowSignature=if($site -eq 'self-report'){'SELF-DIAGNOSTIC-PREMISE'}else{'FLUSH-SELF-REPORT-ORACLE'}}
      $killed=$text.Contains('RUNNER_RESULT=RED') -and $text.Contains($rowSignature) -and -not $text.Contains('Q296-PASS')
      if ($id -eq 'M11-self-report-shutdown-wait') { $killed = $killed -and $text.Contains('WATCHDOG_RED') }
      $receipts.Add([pscustomobject]@{id=$id;outlet=$site;mode=$mode;signature=$rowSignature;exact_red=$killed;log=$log.Substring($root.Length+1);base_file_sha256=$hash})
      Write-Output "$id outlet=$site exact_red=$killed"
      if(-not $killed){throw "mutation $id/$site survived or wrong red"}
    }
  } finally {
    [IO.File]::WriteAllBytes($path,$bytes)
    if((Get-FileHash -LiteralPath $path).Hash -ne $hash){throw "RESTORATION FAILED $file"}
    $receipts | ConvertTo-Json -Depth 5 | Set-Content "$raw/mutations.json"
  }
}

$anchor='pub(crate) fn before_emit(level: Level) {'
Run-Mutation 'M1-sync-stderr' 'crates/qianqian-output-wasapi/src/tier2_tests.rs' $anchor ($anchor + @'

    if target(level) {
        support::line("M296-SYNC-STDERR");
        let _ = std::io::Write::write_all(&mut std::io::stderr(), b"M296 synchronous producer write\n");
    }
'@) @('8','9','10') 'blocked' 'M296-SYNC-STDERR'

Run-Mutation 'M2-consumer-wait' 'crates/qianqian-output-wasapi/src/tier2_tests.rs' $anchor ($anchor + @'

    if target(level) {
        support::line("M296-CONSUMER-WAIT");
        loop { std::thread::park(); }
    }
'@) @('8','9','10') 'stopped' 'M296-CONSUMER-WAIT'

Run-Mutation 'M3-drop-counter' 'crates/qianqian-diagnostics/src/pool.rs' 'self.rejected.fetch_add(1, Ordering::Relaxed);' 'self.rejected.fetch_add(0, Ordering::Relaxed);' @('8','9','10','panic') 'capacity' 'ORACLE'

Run-Mutation 'M4-failed-export-counter' 'crates/qianqian-diagnostics/src/lib.rs' 'inner.export_failed.fetch_add(1, Ordering::Relaxed);' 'inner.export_failed.fetch_add(0, Ordering::Relaxed);' @('8','9','10','panic') 'broken' 'LOSS-ORACLE'

Run-Mutation 'M5-critical-hook-chain' 'apps/headless/src/machine_input.rs' 'if qianqian_diagnostics::is_alpha_producer() {' 'if false && qianqian_diagnostics::is_alpha_producer() {' @('panic') 'broken' 'HOOK-CHAIN-ORACLE'

Run-Mutation 'M6-exit-join' 'apps/headless/src/entry.rs' 'let Some(bound) = wait else {' @'
println!("M296-JOIN-DETACHED");
    if let Ok(handle) = spawned { let _ = handle.join(); return; }
    let Some(bound) = wait else {
'@ @('exit-report') 'blocked' 'M296-JOIN-DETACHED'

Run-Mutation 'M7-exit-result' 'apps/headless/src/entry.rs' 'tier2_tests::report_returned(result.is_ok());' 'tier2_tests::report_returned(true);' @('exit-report') 'broken' 'REPORT-RESULT-ORACLE'

Run-Mutation 'M8-panic-sync-write' 'crates/qianqian-diagnostics/src/lib.rs' 'pub fn note_critical_panic() -> Disposition {' @'
pub fn note_critical_panic() -> Disposition {
    println!("M296-PANIC-SYNC");
    let _ = std::io::Write::write_all(&mut std::io::stderr(), b"synchronous panic diagnostic\n");
'@ @('panic') 'blocked' 'M296-PANIC-SYNC'

Run-Mutation 'M9-panic-consumer-wait' 'crates/qianqian-diagnostics/src/lib.rs' 'pub fn note_critical_panic() -> Disposition {' @'
pub fn note_critical_panic() -> Disposition {
    if std::env::var("Q296_MODE").as_deref() == Ok("stopped") {
        println!("M296-PANIC-CONSUMER-WAIT");
        loop { std::thread::park(); }
    }
'@ @('panic') 'stopped' 'M296-PANIC-CONSUMER-WAIT'

Run-Mutation 'M10-self-report-counter' 'crates/qianqian-diagnostics/src/lib.rs' 'inner.self_diagnostics.fetch_add(1, Ordering::Relaxed);' 'inner.self_diagnostics.fetch_add(0, Ordering::Relaxed);' @('self-report','flush-report') 'broken' 'ORACLE'

Run-Mutation 'M11-self-report-shutdown-wait' 'crates/qianqian-diagnostics/src/lib.rs' 'if started.elapsed() >= deadline {' @'
if started.elapsed() >= deadline {
    static M296_MARKED: std::sync::Once = std::sync::Once::new();
    M296_MARKED.call_once(|| println!("M296-SHUTDOWN-DEADLINE-BYPASSED"));
}
if false && started.elapsed() >= deadline {
'@ @('self-report','flush-report') 'blocked' 'M296-SHUTDOWN-DEADLINE-BYPASSED'

& "$root/research/diagnostics-tier2/build.ps1" -Log "$raw/restored-build.log" | Out-Null
