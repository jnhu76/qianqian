#!/usr/bin/env bash
# Validate the two summary captures parse to the same JSON object; install the
# script-written LF summary as the lane's canonical build-rc-summary.json.
set -uo pipefail
LANE=/mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/g4/build-windows
RC=/mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/build/rc/20261030-203045/summary.json
python3 - "$LANE/build-rc.stdout.json" "$RC" <<'PY'
import json, sys
a = json.load(open(sys.argv[1], encoding="utf-8-sig"))
b = json.load(open(sys.argv[2], encoding="utf-8-sig"))
print("stdout_json_equals_file_summary =", a == b)
print("top_keys =", list(a.keys()))
print("ok =", a["ok"])
PY
cp -f "$RC" "$LANE/build-rc-summary.json"
sha256sum "$LANE/build-rc-summary.json" "$RC"
wc -c "$LANE/build-rc-summary.json"
