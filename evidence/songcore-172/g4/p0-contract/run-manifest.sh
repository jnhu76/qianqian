#!/usr/bin/env bash
# p0-contract lane: final stable manifest. Nothing in the evidence directory is
# being written while this runs (stdout goes outside the dir).
set -u
OUT=/mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/g4/p0-contract
cd "$OUT" || exit 99
rm -f evidence-final-sha256.txt
find . -maxdepth 1 -type f ! -name 'evidence-final-sha256.txt' | sed 's|^\./||' | sort > /dev/shm/final-files.txt
sha256sum $(cat /dev/shm/final-files.txt) > evidence-final-sha256.txt
echo "exit=$?"
echo "final_files=$(wc -l < /dev/shm/final-files.txt) manifest_lines=$(wc -l < evidence-final-sha256.txt)"
echo "-- stage.sha256"; cat stage.sha256
echo "-- stage.json summary"
python3 - "$OUT" <<'PY'
import json,sys,os
s=json.load(open(os.path.join(sys.argv[1],'stage.json'),encoding='utf-8'))
print("stage_id=%s verdict=%s" % (s['stage_id'], s['verdict']))
print("source_sha=%s" % s['source_sha'])
print("evidence_paths=%d evidence_sha256=%d argv=%d exit_codes=%d not_run=%d" % (
  len(s['evidence_paths']), len(s['evidence_sha256']), len(s['argv']), len(s['exit_codes']), len(s['not_run'])))
print("input_aggregate=%s" % s['input_sha']['aggregate_sha256_of_sha256-inputs.txt'])
PY
echo "exit=$?"
echo "== DONE =="
