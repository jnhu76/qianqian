param([Parameter(Mandatory=$true)][string]$Log)
$ErrorActionPreference='Stop'
$jsonl="$Log.artifacts.jsonl"
cargo test -p qianqian-output-wasapi -p qianqian-headless --lib --no-run --message-format=json 1> $jsonl 2> $Log
if($LASTEXITCODE -ne 0){throw "build failed; not a mutation kill: $Log"}
$executables=@{}
foreach($line in Get-Content -LiteralPath $jsonl){
  $item=$line | ConvertFrom-Json
  if($item.reason -eq 'compiler-artifact' -and $item.profile.test -and $item.target.kind -contains 'lib' -and $item.executable){
    $executables[$item.target.name]=$item.executable
  }
}
foreach($name in @('qianqian_output_wasapi','qianqian_headless')){
  if(-not $executables.ContainsKey($name)){throw "missing actual test artifact: $name"}
}
[pscustomobject]@{wasapi=$executables['qianqian_output_wasapi'];headless=$executables['qianqian_headless']}
