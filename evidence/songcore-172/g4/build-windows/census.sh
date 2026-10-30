#!/usr/bin/env bash
# Corrected per-member census + closed manifest identity check.
set -uo pipefail
LANE=/mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/g4/build-windows
F=$LANE/objdump-f.txt
echo "format_lines=$(grep -c 'file format ' "$F")"
echo "pe_x86_64=$(grep -c 'file format pe-x86-64$' "$F")"
echo "non_pe_format_lines=$(grep 'file format ' "$F" | grep -vc 'file format pe-x86-64$')"
echo "in_archive_lines=$(grep -c '^In archive' "$F")"
grep 'file format ' "$F" | grep -v 'file format pe-x86-64$' | head -5
echo
echo "=== closed manifest identity (Windows worktree consumption path)"
python3 - <<'PY'
import json, hashlib
p = "/mnt/c/Users/fred1/source/qianqian/native/build/ffmpeg-xmake/manifest.json"
b = open(p, "rb").read()
m = json.loads(b)
t = m.get("target") or {}
print("target.id        =", t.get("id"))
print("target.platform  =", t.get("platform"))
print("target.arch      =", t.get("arch"))
print("recipe_sha256    =", t.get("recipe_sha256"))
print("units            =", len(m.get("units", [])))
print("manifest_sha256  =", hashlib.sha256(b).hexdigest())
print("bytes            =", len(b))
PY
