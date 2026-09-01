@echo off
rem E10-A0: build the native Windows WASAPI probe (bench-only).
rem Invoked from WSL as: cmd.exe /c tools\pcm_a0_build.bat
pushd "%~dp0\.."
call "C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Auxiliary\Build\vcvars64.bat" >nul
if not exist build\pcm-a0 mkdir build\pcm-a0
cl /nologo /O2 /utf-8 /W3 /TP bench\pcm\windows\qn_wasapi_probe.c /Fe:build\pcm-a0\qn_wasapi_probe.exe /link ole32.lib avrt.lib uuid.lib
set RC=%ERRORLEVEL%
popd
exit /b %RC%
