# Attribute two Windows workspace REDs without changing their existing oracle.
param([Parameter(Mandatory=$true)][string]$Python)
$ErrorActionPreference='Stop'
$base='98ff343467b66890a979edd54d21e14f0275c574'
$raw='evidence/diagnostics-tier2-296/raw'
$scratch=Join-Path ([IO.Path]::GetTempPath()) ('q296-baseline-'+[Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $scratch | Out-Null
& $Python -c @'
import pathlib,subprocess,sys
base,root=sys.argv[1:]
for case in ('LF','CRLF'):
 p=pathlib.Path(root)/case;p.mkdir()
 for name in ('render_order_oracle.rs','wasapi.rs'):
  b=subprocess.check_output(['git','show',base+':crates/qianqian-output-wasapi/src/'+name]).replace(b'\r\n',b'\n')
  if case=='CRLF':b=b.replace(b'\n',b'\r\n')
  (p/name).write_bytes(b)
 (p/'main.rs').write_text('#[path="render_order_oracle.rs"]\nmod render_order_oracle;\n')
'@ $base $scratch
if($LASTEXITCODE -ne 0){throw 'baseline extraction failed'}
$rlib=(Get-ChildItem target/debug/deps/libqianqian_audio_api-*.rlib | Sort-Object LastWriteTime -Descending | Select-Object -First 1).FullName
if(-not $rlib){throw 'build the workspace first'}
$dependency=(Resolve-Path target/debug/deps).Path
$receipts=[Collections.Generic.List[object]]::new()
foreach($case in @('LF','CRLF')){
 $folder=Join-Path $scratch $case
 $exe=Join-Path $folder 'baseline.exe'
 rustc --edition=2024 --test (Join-Path $folder 'main.rs') -L "dependency=$dependency" --extern "qianqian_audio_api=$rlib" -o $exe *> "$raw/baseline-$case-build.log"
 if($LASTEXITCODE -ne 0){throw "baseline $case did not compile"}
 foreach($test in @('a_park_slice_that_skips_its_quiescent_publication_is_rejected','a_publication_moved_into_a_comment_is_rejected')){
  & $exe --exact "render_order_oracle::tests::$test" --nocapture *> "$raw/baseline-$case-$test.log"
  $code=$LASTEXITCODE
  $expected=if($case -eq 'LF'){0}else{101}
  $receipts.Add([pscustomobject]@{base_sha=$base;case=$case;test=$test;exit=$code;expected_exit=$expected;matches=($code -eq $expected);exe_sha256=(Get-FileHash $exe).Hash;audio_rlib_sha256=(Get-FileHash $rlib).Hash;oracle_sha256=(Get-FileHash (Join-Path $folder 'render_order_oracle.rs')).Hash;source_sha256=(Get-FileHash (Join-Path $folder 'wasapi.rs')).Hash})
 }
}
$receipts | ConvertTo-Json -Depth 4 | Set-Content "$raw/baseline-oracle.json"
if(@($receipts | Where-Object {-not $_.matches}).Count -ne 0){throw 'baseline attribution differs from prediction'}
Write-Output 'Baseline main oracle: LF 2/2 GREEN, CRLF same 2/2 RED; no production change'
