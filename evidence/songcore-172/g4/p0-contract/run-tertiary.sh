#!/usr/bin/env bash
# p0-contract lane, tertiary (read-only): locate windows closure manifest, read
# the §8 V1 receipt check, and sample the RC package set arithmetic.
set -u
SRC=/home/hoo/Source/qianqian
WIN=/mnt/c/Users/fred1/source/qianqian
OUT=$WIN/evidence/songcore-172/g4/p0-contract
cd "$SRC" || { echo "FATAL cd"; exit 99; }
mark(){ echo; echo "########## $* ##########"; }

mark "E1 windows closure candidates"
for d in /home/hoo/q172rc/closure /home/hoo/q172d2-verify/closure; do
  echo "-- $d"
  if [ -d "$d" ]; then ls -la "$d" | head -20; else echo "  NOT_A_DIR"; fi
done
echo "-- find manifest.json under those roots"
find /home/hoo/q172rc /home/hoo/q172d2-verify -maxdepth 4 -name 'manifest.json' 2>/dev/null | head -10
echo "exit=$?"

mark "E2 any windows closure manifest: target.recipe_sha256"
for m in $(find /home/hoo/q172rc /home/hoo/q172d2-verify -maxdepth 4 -name 'manifest.json' 2>/dev/null | head -10); do
  echo "-- $m"
  sha256sum "$m"
  python3 - "$m" <<'PY'
import json,sys
try:
    d=json.load(open(sys.argv[1]))
except Exception as e:
    print("  UNREADABLE: %s" % e); raise SystemExit(0)
t=d.get('target') or {}
print("  schema=%s target.id=%s target.target_os=%s target.cross_prefix=%s" % (d.get('schema'), t.get('id'), t.get('target_os'), t.get('cross_prefix')))
print("  recipe_sha256=%s" % t.get('recipe_sha256'))
print("  eq_windows_json=%s" % (t.get('recipe_sha256') == '3ba0a9e686d169f0e00ba46ade11a8f8658f204537713eabe200633ad5f8492d'))
PY
  echo "  exit=$?"
done

mark "E3 §8 V1 receipt 'checks' contents"
python3 - "$SRC/evidence/songcore-172/package/package-summary.json" <<'PY'
import json,sys
d=json.load(open(sys.argv[1]))
print("version=%s version_status=%s ok=%s" % (d.get('version'), d.get('version_status'), d.get('ok')))
ch=d.get('checks')
print("checks_type=%s" % type(ch).__name__)
print(json.dumps(ch, indent=2, sort_keys=True)[:3000])
PY
echo "exit=$?"

mark "E4 contract V1 wording vs measured receipt"
grep -n 'V1_sets\|14 package files\|13 checksum entries\|12 files' docs/release/songcore-release-contract.md
echo "exit=$?"
echo "########## DONE ##########"
