@echo off
setlocal
if "%~1"=="" (
  echo usage: docs-verify.cmd ^<worktree-root^> [--bootstrap] 1>&2
  exit /b 2
)
set "QQ_ROOT=%~f1"
if not exist "%QQ_ROOT%\website\package.json" (
  echo docs-verify: website\package.json missing under %QQ_ROOT% 1>&2
  exit /b 2
)
pushd "%QQ_ROOT%\website" || exit /b 2
if /I "%~2"=="--bootstrap" (
  call npm ci
  if errorlevel 1 goto :fail
)
if not exist node_modules (
  echo docs-verify: dependencies missing; rerun with --bootstrap 1>&2
  popd
  exit /b 3
)
node scripts\verify.mjs
if errorlevel 1 goto :fail
call npx vitepress build
if errorlevel 1 goto :fail
popd
exit /b 0
:fail
set "QQ_RC=%errorlevel%"
popd
exit /b %QQ_RC%
