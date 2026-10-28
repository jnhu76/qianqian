param(
  [Parameter(Mandatory=$true)][string]$Python,
  [string]$Log='evidence/diagnostics-tier2-296/raw/pre-push.log'
)
$ErrorActionPreference='Stop'
$pythonPath=(Resolve-Path -LiteralPath $Python).Path
$shim=Join-Path ([IO.Path]::GetTempPath()) ('q296-python-'+[Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $shim | Out-Null
# Windows execution aliases may point at the Store rather than an interpreter.
# Use an explicit interpreter only for this process and its hook children.
[IO.File]::WriteAllText((Join-Path $shim 'python3.cmd'), "@echo off`r`n`"$pythonPath`" %*`r`n")
$bashPython=$pythonPath.Replace('\','/')
[IO.File]::WriteAllText((Join-Path $shim 'python3'), "#!/bin/sh`nexec '$bashPython' " + '"$@"' + "`n", [Text.UTF8Encoding]::new($false))
$savedPath=$env:PATH
$savedUtf8=$env:PYTHONUTF8
try {
  $env:PATH="$shim;C:\Program Files\Git\bin;C:\Program Files\Git\usr\bin;$savedPath"
  $env:PYTHONUTF8='1'
  lefthook run pre-push --all-files *> $Log
  $code=$LASTEXITCODE
  Add-Content -LiteralPath $Log -Value "GATE_EXIT=$code"
  [pscustomobject]@{python=$pythonPath;python_utf8='1';shim=$shim;git_bash='C:\Program Files\Git\bin';exit=$code} | ConvertTo-Json | Set-Content 'evidence/diagnostics-tier2-296/raw/gate-environment.json'
  if($code -ne 0){throw "pre-push gate failed: $Log"}
} finally { $env:PATH=$savedPath; $env:PYTHONUTF8=$savedUtf8 }
