#!/usr/bin/env bash
# Measure the REAL exit status of ops/build-linux.sh on its clone-failure
# path. Inline `$?` inside `wsl.exe -- bash -c '...'` is unreliable in this
# host (observed: `false; echo RC=$?` printed 0), so the status is written
# by a real script file and the wrapper exits with it.
export HOME=/home/hoo/q172-g4-audit5
export PATH=/home/hoo/.local/bin:$PATH
export QIANQIAN_S3_SOURCE=/nonexistent/src
export QIANQIAN_S3_MAIN=/nonexistent/main
bash /mnt/c/Users/fred1/source/qianqian/ops/build-linux.sh
rc=$?
printf 'SCRIPT_RC=%s\n' "$rc" | tee /mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/g4/p0-entries/build-linux-clone-fail-rc.txt
exit "$rc"
