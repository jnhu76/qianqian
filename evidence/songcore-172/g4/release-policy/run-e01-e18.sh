#!/usr/bin/env bash
# Lane F evidence runner: E01-E18 release-policy causal controls, read-only w.r.t. tracked source.
set -uo pipefail
export PATH="$HOME/.local/bin:$PATH"
cd /home/hoo/Source/qianqian || exit 91
EVID=/mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/g4/release-policy
mkdir -p "$EVID"
START=$(date -u +%Y-%m-%dT%H:%M:%SZ)
python3 tools/test_release.py --report "$EVID/release-controls.json" > "$EVID/e01-e18.log" 2>&1
rc=$?
END=$(date -u +%Y-%m-%dT%H:%M:%SZ)
{
  echo "argv: python3 tools/test_release.py --report $EVID/release-controls.json"
  echo "cwd: /home/hoo/Source/qianqian"
  echo "source_sha: $(git rev-parse HEAD)"
  echo "start_utc: $START"
  echo "end_utc: $END"
  echo "exit_code: $rc"
} > "$EVID/e01-e18.exit.txt"
echo "exit_code=$rc"
tail -n 3 "$EVID/e01-e18.log"
