#!/usr/bin/env bash
# Second real-condition run of ops/build-linux.sh with the TRUE exit status
# captured by a script file (not inline $?), so the receipt can state it.
export PATH="$HOME/.local/bin:$PATH"
LOG=/mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/g4/p0-entries/build-linux-real-run2.log
bash /mnt/c/Users/fred1/source/qianqian/ops/build-linux.sh > "$LOG" 2>&1
rc=$?
{
  printf 'SCRIPT_RC=%s\n' "$rc"
  printf 'workspace_head=%s\n' "$(git -C /mnt/c/Users/fred1/source/qianqian rev-parse HEAD)"
  printf 'summary_head=%s\n' "$(python3 -c "import json,sys;print(json.load(open(sys.argv[1]))['head'])" /tmp/s3-summary.json 2>/dev/null)"
  printf 'summary_verdict=%s\n' "$(python3 -c "import json,sys;print(json.load(open(sys.argv[1]))['verdict'])" /tmp/s3-summary.json 2>/dev/null)"
  printf 'summary_clone=%s\n' "$(python3 -c "import json,sys;print(json.load(open(sys.argv[1]))['clone'])" /tmp/s3-summary.json 2>/dev/null)"
  printf 'summary_closure=%s\n' "$(python3 -c "import json,sys;print(json.load(open(sys.argv[1]))['closure'])" /tmp/s3-summary.json 2>/dev/null)"
  printf 'summary_static_sha=%s\n' "$(python3 -c "import json,sys;print(json.load(open(sys.argv[1]))['artifacts']['static']['sha256'])" /tmp/s3-summary.json 2>/dev/null)"
} | tee /mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/g4/p0-entries/build-linux-real-run2-rc.txt
cp /tmp/s3-summary.json /mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/g4/p0-entries/build-linux-run2-summary.json 2>/dev/null || true
exit "$rc"
