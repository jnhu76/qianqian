#!/usr/bin/env bash
# Stage A TUI scenario runner (from WSL; drives the real Windows host).
#
# The ConPTY child must NOT inherit the WSL interop console — under that
# console the pseudoconsole attribute is ignored and the child renders
# to the interop terminal instead of the pipe. Each scenario therefore
# runs detached through Start-Process (hidden window), with
# stdout/stderr redirected to files on the Windows side; the driver's
# exit code rides through $p.ExitCode.
#
# Usage: tools/run-tui.sh <run-number> SCENARIO [SCENARIO...]
set -uo pipefail

RUN="${1:?usage: run-tui.sh <run-number> SCENARIO...}"
shift
REPO="$(cd "$(dirname "$0")/../../.." && pwd)"
HARNESS="$REPO/research/transport-dogfood"
STAGE_WSL="/mnt/c/Users/Public/qianqian-dogfood"
LOGDIR="$HARNESS/evidence/logs"
EXE="$HARNESS/target/x86_64-pc-windows-gnu/release/tuidriver.exe"
HEADLESS="$REPO/target/x86_64-pc-windows-gnu/release/qianqian-headless.exe"
MEDIA="C:\\Users\\Public\\qianqian-dogfood"
OUT="C:\\Users\\Public\\qianqian-dogfood\\evidence"

mkdir -p "$LOGDIR"
cp "$EXE" "$STAGE_WSL/tuidriver.exe"
cp "$HEADLESS" "$STAGE_WSL/qianqian-headless.exe"

SCEN="$*"
PS_SCEN=$(printf "'%s'," $SCEN | sed 's/,$//')

powershell.exe -NoProfile -Command "
    \$p = Start-Process -FilePath 'C:\\Users\\Public\\qianqian-dogfood\\tuidriver.exe' \`
        -ArgumentList '--exe','C:\\Users\\Public\\qianqian-dogfood\\qianqian-headless.exe', \`
            '--media','$MEDIA','--out','$OUT',$PS_SCEN \`
        -WindowStyle Hidden -Wait -PassThru \`
        -RedirectStandardOutput 'C:\\Users\\Public\\qianqian-dogfood\\driver-stdout-$RUN.txt' \`
        -RedirectStandardError 'C:\\Users\\Public\\qianqian-dogfood\\driver-stderr-$RUN.txt'
    exit \$p.ExitCode
"
DRIVER_RC=$?
tr -d '\r' < "$STAGE_WSL/driver-stdout-$RUN.txt" 2>/dev/null
echo "driver exit: $DRIVER_RC"
cp "$STAGE_WSL/driver-stdout-$RUN.txt" "$LOGDIR/tui-run$RUN.stdout" 2>/dev/null || true
cp "$STAGE_WSL/driver-stderr-$RUN.txt" "$LOGDIR/tui-run$RUN.stderr" 2>/dev/null || true
exit "$DRIVER_RC"
