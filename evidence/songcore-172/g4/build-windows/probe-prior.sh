#!/usr/bin/env bash
# Summarize prior RC summaries + #329 reference archive.
set -uo pipefail
for f in /mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/build/rc/*/summary.json; do
  echo "=== $f"
  python3 - "$f" <<'PY'
import json, sys
d = json.load(open(sys.argv[1]))
print(" started:", d.get("started"))
print(" commit :", (d.get("source") or {}).get("commit"))
print(" ok     :", d.get("ok"))
a = d.get("artifacts") or {}
print(" sha    :", a.get("libsongcore_a_sha256"))
print(" size   :", a.get("libsongcore_a_size"))
print(" pe/elf :", a.get("pe_members"), "/", a.get("elf_members"), " songabi:", a.get("song_abi_symbols"))
st = d.get("steps") or {}
for k, v in st.items():
    if str(v).startswith("FAIL"):
        print(" FAILSTEP:", k, "->", v)
PY
done
echo
echo "=== #329 winner verdict"
python3 - <<'PY'
import json
p = "/mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/q2-crossroot/cmp/windows-winner/verdict.json"
d = json.load(open(p))
for k in sorted(d):
    v = d[k]
    if not isinstance(v, (dict, list)):
        print(f" {k}: {v}")
PY
echo
echo "=== #329 l1-route receipt"
python3 - <<'PY'
import json
p = "/mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/q2-crossroot/build-receipts/windows-win-l1-route.summary.json"
d = json.load(open(p))
print(json.dumps(d, indent=1)[:3000])
PY
