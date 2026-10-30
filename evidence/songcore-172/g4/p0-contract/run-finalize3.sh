#!/usr/bin/env bash
# p0-contract lane, finalize pass B: verify stage.json against real bytes, then
# emit the final manifest (which covers stage.json and the run log).
set -u
WIN=/mnt/c/Users/fred1/source/qianqian
OUT=$WIN/evidence/songcore-172/g4/p0-contract
cd "$OUT" || exit 99

echo "== B1 sha256sum -c evidence-sha256.txt (stable set) =="
sha256sum -c evidence-sha256.txt > /tmp/b1.txt 2>&1
echo "exit=$?"
grep -c ': OK$' /tmp/b1.txt
grep -v ': OK$' /tmp/b1.txt | head -5

echo "== B2 stage.json evidence_paths exist and hashes match =="
python3 - "$OUT" <<'PY'
import hashlib, json, os, sys
out = sys.argv[1]
s = json.load(open(os.path.join(out, 'stage.json'), encoding='utf-8'))
paths = s['evidence_paths']; shas = s['evidence_sha256']
print("paths=%d shas=%d" % (len(paths), len(shas)))
missing = []; mismatch = []
repo = os.path.dirname(os.path.dirname(os.path.dirname(os.path.dirname(out))))
for p, want in zip(paths, shas):
    full = p if os.path.isabs(p) else os.path.join(repo, p)
    if not os.path.isfile(full):
        missing.append(p); continue
    got = hashlib.sha256(open(full, 'rb').read()).hexdigest()
    if got != want:
        mismatch.append((p, want, got))
print("missing=%d mismatch=%d" % (len(missing), len(mismatch)))
print("stage_json_self_listed=%s" % any(p.endswith('stage.json') for p in paths[:-1]))
for m in missing: print("  MISSING %s" % m)
for m in mismatch: print("  MISMATCH %s want=%s got=%s" % m)
PY
echo "exit=$?"

echo "== B3 final manifest =="
sha256sum stage.json > stage.sha256
echo "exit=$?"; cat stage.sha256
find . -maxdepth 1 -type f ! -name 'evidence-final-sha256.txt' | sed 's|^\./||' | sort > /tmp/final-files.txt
sha256sum $(cat /tmp/final-files.txt) > evidence-final-sha256.txt
echo "exit=$?"
echo "final_files=$(wc -l < /tmp/final-files.txt) final_lines=$(wc -l < evidence-final-sha256.txt)"
echo "== DONE =="
