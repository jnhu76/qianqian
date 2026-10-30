#!/usr/bin/env bash
# p0-contract lane, secondary (read-only): §8 recipe_sha256 identity against the
# imported closure manifests, §2 equality-tooling surface, §8 V1 receipt shape.
set -u
SRC=/home/hoo/Source/qianqian
WIN=/mnt/c/Users/fred1/source/qianqian
OUT=$WIN/evidence/songcore-172/g4/p0-contract
cd "$SRC" || { echo "FATAL cd"; exit 99; }
mark(){ echo; echo "########## $* ##########"; }

mark "D1 imported closure manifests: target.recipe_sha256"
for m in /home/hoo/qianqian-172-linux/native/build/ffmpeg-xmake/manifest.json \
         /home/hoo/q172rc/closure/manifest.json \
         /home/hoo/q172d2-verify/closure/manifest.json; do
  echo "-- $m"
  if [ -f "$m" ]; then
    python3 - "$m" <<'PY'
import json,sys
d=json.load(open(sys.argv[1]))
t=d.get('target') or {}
print("  schema=%s target.id=%s target.platform=%s target.arch=%s" % (d.get('schema'), t.get('id'), t.get('platform'), t.get('arch')))
print("  target.target_os=%s target.cross_prefix=%s target.recipe_sha256=%s" % (t.get('target_os'), t.get('cross_prefix'), t.get('recipe_sha256')))
print("  recipe_sha256_matches_measured_windows_json=%s" % (t.get('recipe_sha256') == '3ba0a9e686d169f0e00ba46ade11a8f8658f204537713eabe200633ad5f8492d'))
print("  recipe_sha256_matches_measured_linux_json=%s" % (t.get('recipe_sha256') == '79c307e731427bf1d94ab2ed43d7729c91d1d747a02392ee84170616467cdb6d'))
PY
    echo "  exit=$?"
  else
    echo "  ABSENT"
  fi
done

mark "D2 measured recipe sha256 vs those manifests"
sha256sum native/ffmpeg/targets/linux-x86_64.json native/ffmpeg/targets/windows-x86_64.json
echo "exit=$?"

mark "D3 §2 equality tooling: who reads the xmake project version"
grep -rn --include='*.py' --include='*.sh' --include='*.ps1' --include='*.lua' --include='*.yml' \
  -e 'set_version' -e 'project_version' -e 'xmake.lua' tools ops .github native 2>/dev/null | head -30
echo "exit(grep)=$?"
echo "-- tools/test_release.py context for songcore-v0.1.0 literal"
sed -n '55,80p' tools/test_release.py
echo "exit=$?"

mark "D4 §8 V1 receipt shape (existing archive, read-only)"
REC=$SRC/evidence/songcore-172/package/package-summary.json
if [ -f "$REC" ]; then
  sha256sum "$REC"
  python3 - "$REC" <<'PY'
import json,sys
d=json.load(open(sys.argv[1]))
print("top_keys=%s" % ",".join(sorted(d.keys())))
for k in ('V1_sets','v1_sets','package_files','checksums_entries','files_entries'):
    if k in d: print("%s=%s" % (k, json.dumps(d[k], sort_keys=True)))
PY
  echo "exit=$?"
else
  echo "ABSENT: $REC"
fi

mark "D5 contract §2/§4/§6/§8 exact lines of record (source-text citations)"
grep -n 'version identity\|manifest.songcore_version\|native/xmake.lua:2' docs/release/songcore-release-contract.md
echo "exit=$?"
grep -n 'shared: false, status: provisional\|linux target json\|windows target json' docs/release/songcore-release-contract.md
echo "exit=$?"
echo "########## DONE ##########"
