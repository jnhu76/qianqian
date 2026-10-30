#!/usr/bin/env bash
# p0-contract lane: make stage.json's evidence_paths/evidence_sha256 parallel
# (drop the self-reference), then re-emit stage.sha256 and the final manifest.
set -u
OUT=/mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/g4/p0-contract
cd "$OUT" || exit 99
python3 - "$OUT" <<'PY'
import json, os, sys
p = os.path.join(sys.argv[1], 'stage.json')
s = json.load(open(p, encoding='utf-8'))
before = len(s['evidence_paths'])
s['evidence_paths'] = [x for x in s['evidence_paths'] if not x.endswith('/stage.json')]
with open(p, 'w', encoding='utf-8', newline='\n') as f:
    json.dump(s, f, indent=2, sort_keys=False); f.write('\n')
print("evidence_paths %d -> %d, evidence_sha256 %d" % (before, len(s['evidence_paths']), len(s['evidence_sha256'])))
assert len(s['evidence_paths']) == len(s['evidence_sha256'])
PY
echo "exit=$?"
sha256sum stage.json > stage.sha256
echo "exit=$?"; cat stage.sha256
find . -maxdepth 1 -type f ! -name 'evidence-final-sha256.txt' | sed 's|^\./||' | sort > /tmp/final-files2.txt
sha256sum $(cat /tmp/final-files2.txt) > evidence-final-sha256.txt
echo "exit=$?"
echo "final_files=$(wc -l < /tmp/final-files2.txt) final_lines=$(wc -l < evidence-final-sha256.txt)"
echo "== DONE =="
