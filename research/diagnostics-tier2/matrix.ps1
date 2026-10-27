# Native Windows, run from repository root. Every armed row is a fresh child.
$ErrorActionPreference='Stop'
$raw='evidence/diagnostics-tier2-296/raw'
$bins=& ./research/diagnostics-tier2/build.ps1 -Log "$raw/final-build.log"
foreach($site in @('8','9','10','panic','exit-report','self-report','flush-report','previous')){
  $modes=if($site -eq 'previous'){@('blocked','stopped','broken')} elseif($site -in @('exit-report','self-report','flush-report')){@('healthy','blocked','broken')} else {@('healthy','blocked','stopped','capacity','broken')}
  foreach($mode in $modes){
    $isWasapi=$site -in @('8','9','10')
    $exe=if($isWasapi){$bins.wasapi}else{$bins.headless}
    $filter=if($isWasapi){'tier2_tests::tier2_child'}else{'entry::tier2_tests::tier2_child'}
    $log="$raw/$site-$mode.log"
    & ./research/diagnostics-tier2/run.ps1 -Exe $exe -Filter $filter -Outlet $site -Mode $mode -Log $log | Out-Null
    if(-not (Get-Content $log -Raw).Contains('RUNNER_RESULT=GREEN')){throw "row failed: $site/$mode"}
    Write-Output "$site/$mode GREEN"
  }
}
# Harness negatives: retained-open pipe must refute the broken premise, and
# a misspelled test filter must never turn zero executed tests into GREEN.
& ./research/diagnostics-tier2/run.ps1 -Exe $bins.headless -Filter entry::tier2_tests::tier2_child -Outlet panic -Mode broken -SkipClose -Log "$raw/NC-open-pipe.log" | Out-Null
if(-not (Get-Content "$raw/NC-open-pipe.log" -Raw).Contains('PREMISE broken sink must fail inside child')){throw 'canary negative wrong RED'}
& ./research/diagnostics-tier2/run.ps1 -Exe $bins.headless -Filter no_such_test -Outlet panic -Mode healthy -Log "$raw/NC-zero-tests.log" | Out-Null
if(-not (Get-Content "$raw/NC-zero-tests.log" -Raw).Contains('ORACLE_RED missing exact execution/result marker')){throw 'zero-test negative wrong RED'}
