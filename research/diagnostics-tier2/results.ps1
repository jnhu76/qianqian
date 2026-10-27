$ErrorActionPreference='Stop'
$raw='evidence/diagnostics-tier2-296/raw'
$cells=[Collections.Generic.List[object]]::new()
$controls=[ordered]@{blocked='BLOCKED_SINK';stopped='STOPPED_CONSUMER';capacity='CAPACITY_EXHAUSTION';broken='WRITE_FAILURE'}
$outlets=@('8','9','10','panic','previous','ffmpeg-discard','ffmpeg-bridge','decode-read','decode-seek','self-report','flush-report','exit-report')
foreach($outlet in $outlets){
  foreach($mode in $controls.Keys){
    $status='PASS'; $reason='Actual outlet control, scoped to report boundary'; $log=$null
    if($outlet -eq 'ffmpeg-discard'){$status='NOT_APPLICABLE';$reason='Discard callback does not write or queue; actual native installation separately NOT_RUN'}
    elseif($outlet -in @('ffmpeg-bridge','decode-read','decode-seek')){$status='NOT_RUN';$reason='No verified native archive/provenance in inspected environment; #172 boundary'}
    elseif($outlet -eq 'previous'){
      if($mode -eq 'capacity'){$status='NOT_APPLICABLE';$reason='Synchronous predecessor has no alpha slot/beta queue'}
      else{$status='KNOWN_RESIDUAL';$reason='Synchronous predecessor chain; blocked wrapper entry only, stopped controlled gate, broken actual inherited default returns; no arbitrary-hook isolation promise'}
    }
    elseif($outlet -in @('self-report','flush-report','exit-report') -and $mode -in @('stopped','capacity')){
      $status='NOT_APPLICABLE';$reason='Direct stderr write has no independent application consumer/admission/queue; external backpressure belongs to C1/C4'
    }
    if($status -in @('PASS','KNOWN_RESIDUAL')){
      $log="$raw/$outlet-$mode.log"
      $text=Get-Content -LiteralPath $log -Raw
      if(-not $text.Contains('RUNNER_RESULT=GREEN') -or -not $text.Contains('running 1 test') -or -not $text.Contains('EXIT=0 ')){throw "invalid final row $log"}
      if($status -eq 'PASS' -and -not $text.Contains('Q296-PASS')){throw "missing property result $log"}
      if($status -eq 'KNOWN_RESIDUAL' -and -not $text.Contains('Q296-KNOWN-RESIDUAL')){throw "missing residual witness $log"}
    }
    $cells.Add([pscustomobject]@{outlet=$outlet;control=$controls[$mode];status=$status;reason=$reason;log=$log})
  }
}
$mutants=@(Get-Content "$raw/mutations.json" -Raw | ConvertFrom-Json)
if($mutants.Count -ne 23 -or @($mutants | Where-Object {-not $_.exact_red}).Count -ne 0){throw 'final 23 mutation exact-RED receipts incomplete'}
foreach($r in $mutants){
  $text=Get-Content -LiteralPath $r.log -Raw
  if(-not $text.Contains('RUNNER_RESULT=RED') -or -not $text.Contains($r.signature) -or $text.Contains('Q296-PASS')){throw "wrong mutation receipt $($r.id)/$($r.outlet)"}
  if($r.id -eq 'M11-self-report-shutdown-wait' -and (-not $text.Contains('WATCHDOG_RED') -or $r.signature -ne 'M296-SHUTDOWN-DEADLINE-BYPASSED')){throw 'M11 lacks exact watchdog attribution'}
}
foreach($nc in @('NC-open-pipe','NC-zero-tests')){
  if(-not (Get-Content "$raw/$nc.log" -Raw).Contains('RUNNER_RESULT=RED')){throw "missing harness negative $nc"}
}
$mutationFiles=@{
 'M1-sync-stderr'='crates/qianqian-output-wasapi/src/tier2_tests.rs';
 'M2-consumer-wait'='crates/qianqian-output-wasapi/src/tier2_tests.rs';
 'M3-drop-counter'='crates/qianqian-diagnostics/src/pool.rs';
 'M4-failed-export-counter'='crates/qianqian-diagnostics/src/lib.rs';
 'M5-critical-hook-chain'='apps/headless/src/machine_input.rs';
 'M6-exit-join'='apps/headless/src/entry.rs';
 'M7-exit-result'='apps/headless/src/entry.rs';
 'M8-panic-sync-write'='crates/qianqian-diagnostics/src/lib.rs';
 'M9-panic-consumer-wait'='crates/qianqian-diagnostics/src/lib.rs';
 'M10-self-report-counter'='crates/qianqian-diagnostics/src/lib.rs';
 'M11-self-report-shutdown-wait'='crates/qianqian-diagnostics/src/lib.rs'
}
$restoration=@($mutants | Group-Object id | ForEach-Object {
  $r=$_.Group[0]; $file=$mutationFiles[$r.id]; $current=(Get-FileHash -LiteralPath $file).Hash
  if($current -ne $r.base_file_sha256){throw "mutation restoration mismatch $($r.id)"}
  [pscustomobject]@{id=$r.id;file=$file;expected_sha256=$r.base_file_sha256;current_sha256=$current;matches=$true}
})
$restoration | ConvertTo-Json -Depth 4 | Set-Content "$raw/restoration.json"
$sources=@('apps/headless/src/entry.rs','apps/headless/src/tier2_tests.rs','crates/qianqian-output-wasapi/src/lib.rs','crates/qianqian-output-wasapi/src/wasapi.rs','crates/qianqian-output-wasapi/src/tier2_tests.rs','crates/qianqian-diagnostics/src/lib.rs','crates/qianqian-diagnostics/src/pool.rs','research/diagnostics-tier2/support.rs','research/diagnostics-tier2/run.ps1','research/diagnostics-tier2/build.ps1','research/diagnostics-tier2/matrix.ps1','research/diagnostics-tier2/mutations.ps1','research/diagnostics-tier2/results.ps1')
$sources += 'research/diagnostics-tier2/gate.ps1'
$sourceHashes=@($sources | ForEach-Object {[pscustomobject]@{path=$_;sha256=(Get-FileHash -LiteralPath $_).Hash.ToLower()}})
$sourceHashes | ConvertTo-Json -Depth 4 | Set-Content "$raw/source-sha256.json"
$result=[ordered]@{base_sha=(git merge-base HEAD origin/main);captured_utc=[DateTime]::UtcNow.ToString('o');target='x86_64-pc-windows-msvc';source_manifest_sha256=(Get-FileHash "$raw/source-sha256.json").Hash.ToLower();cells=$cells;counts=@($cells | Group-Object status | ForEach-Object {[pscustomobject]@{status=$_.Name;count=$_.Count}});mutation_exact_red=$mutants.Count;harness_negative_red=2;product_differentials=@(@{id='FFMPEG_LOG_ZERO';status='FAIL_PRODUCT';evidence_class='SOURCE_CONFIRMED_CONFIG_GATE_DIFFERENTIAL';native_runtime_impact='NOT_RUN';production_fix='ISOLATED_PROPOSAL_ONLY'});initial_oracle_failures='raw/initial and raw/round1; not final product failures';issue_296_closable=$false;issue_317_audit_delivered=$true}
$result | ConvertTo-Json -Depth 8 | Set-Content "$raw/results.json"
Write-Output '48 final cells validated; 23 exact mutation REDs; 2 harness negatives RED'
