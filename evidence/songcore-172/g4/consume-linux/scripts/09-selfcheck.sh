#!/usr/bin/env bash
# Lane C (linux) - stage 09: final self-consistency check of the lane evidence.
#   * regenerate evidence-sha256.txt (now also covers stage.json)
#   * verify every sha256 recorded inside stage.json against the file on disk
set -u
EVID=/mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/g4/consume-linux
LOG=$EVID/logs/stage09-selfcheck.log
: > "$LOG"

find "$EVID" -type f ! -name evidence-sha256.txt -print0 | sort -z | xargs -0 sha256sum > "$EVID/evidence-sha256.txt"
echo "evidence files hashed: $(wc -l < "$EVID/evidence-sha256.txt")" | tee -a "$LOG"

python3 - "$EVID" <<'PY' 2>&1 | tee -a "$LOG"
import json, hashlib, sys, pathlib
evid = pathlib.Path(sys.argv[1])
repo = pathlib.Path("/mnt/c/Users/fred1/source/qianqian")
d = json.load(open(evid / "stage.json"))
bad = ok = 0
for p, h in zip(d["evidence_paths"], d["evidence_sha256"]):
    if h.startswith("SELF") or h.startswith("REGENERATED"):
        print("SKIP(self-referential):", p); continue
    f = repo / p
    if not f.exists():
        print("MISSING:", p); bad += 1; continue
    a = hashlib.sha256(f.read_bytes()).hexdigest()
    if a == h:
        ok += 1
    else:
        print("MISMATCH:", p, "recorded", h, "actual", a); bad += 1
print(f"stage.json evidence hashes: {ok} verified, {bad} mismatched")
print("verdict field:", d["verdict"], "| not_run:", len(d["not_run"]), "| observations:", len(d["observations"]))
PY
