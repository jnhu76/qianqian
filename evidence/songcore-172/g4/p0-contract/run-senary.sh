#!/usr/bin/env bash
# p0-contract lane, senary (read-only): §8 V1 verified against the actual ZIP
# container (the stage tree is not the container); §2 version plumbing in ops/.
set -u
SRC=/home/hoo/Source/qianqian
WIN=/mnt/c/Users/fred1/source/qianqian
OUT=$WIN/evidence/songcore-172/g4/p0-contract
SCRATCH=/home/hoo/q172-g4-p0
ZIP=$SRC/evidence/songcore-172/package/songcore-v0.2.0-rc.1-windows-x86_64.zip
cd "$SRC" || exit 99
mark(){ echo; echo "########## $* ##########"; }

mark "H1 extract RC zip to scratch and re-hash every entry"
rm -rf "$SCRATCH/rc"; mkdir -p "$SCRATCH/rc"
python3 - "$ZIP" "$SCRATCH/rc" <<'PY'
import zipfile,sys,os
z=zipfile.ZipFile(sys.argv[1]); z.extractall(sys.argv[2]); print("extracted=%d" % len(z.namelist()))
PY
echo "exit=$?"
ROOT="$SCRATCH/rc/songcore-v0.2.0-rc.1-windows-x86_64"
ls -R "$ROOT" > "$OUT/rc-extracted-tree.txt" 2>&1
echo "exit=$?"
find "$ROOT" -type f | sed "s|$ROOT/||" | sort > "$OUT/rc-extracted-files.txt"
echo "extracted_file_count=$(wc -l < "$OUT/rc-extracted-files.txt")"; cat "$OUT/rc-extracted-files.txt"

mark "H2 V1 re-hash: every checksums row against extracted bytes"
python3 - "$ROOT" > "$OUT/rc-v1-rehash.txt" 2>&1 <<'PY'
import hashlib,json,os,sys
root=sys.argv[1]
rows=[]
for line in open(os.path.join(root,'metadata','checksums.txt'), encoding='utf-8'):
    parts=line.split()
    if len(parts)==2: rows.append((parts[0], parts[1].lstrip('*').lstrip('./')))
bad=[]
for want,rel in rows:
    p=os.path.join(root,*rel.split('/'))
    got=hashlib.sha256(open(p,'rb').read()).hexdigest() if os.path.isfile(p) else 'MISSING'
    if got!=want: bad.append((rel,want,got))
print("checksums_rows=%d mismatch=%d" % (len(rows), len(bad)))
for b in bad: print("  MISMATCH %s want=%s got=%s" % b)
files=json.load(open(os.path.join(root,'metadata','manifest.json'))).get('files') or {}
bad2=[]
for rel,meta in files.items():
    p=os.path.join(root,*rel.split('/'))
    got=hashlib.sha256(open(p,'rb').read()).hexdigest() if os.path.isfile(p) else 'MISSING'
    if got!=meta.get('sha256') or os.path.getsize(p)!=meta.get('bytes'):
        bad2.append((rel,meta.get('sha256'),got,meta.get('bytes')))
print("files_keys=%d mismatch=%d" % (len(files), len(bad2)))
for b in bad2: print("  MISMATCH %s" % (b,))
cs=set(rel for _,rel in rows); fk=set(files)
print("files_keys == checksums_minus_manifest = %s" % (fk == (cs-{'metadata/manifest.json'})))
print("checksums == all_files_minus_self = %s" % (cs == (set(os.path.relpath(os.path.join(d,f),root).replace(os.sep,'/') for d,_,fs in os.walk(root) for f in fs) - {'metadata/checksums.txt'})))
print("manifest_has_self_or_checksums = %s" % bool({'metadata/manifest.json','metadata/checksums.txt'} & fk))
PY
echo "exit=$?"; cat "$OUT/rc-v1-rehash.txt"

mark "H3 V2: packaged header vs source header"
sha256sum "$ROOT/include/songcore.h" "$SRC/native/include/songcore.h" > "$OUT/rc-v2-header.txt" 2>&1
echo "exit=$?"; cat "$OUT/rc-v2-header.txt"

mark "H4 V3: RC manifest pin vs pin.json"
python3 - "$ROOT/metadata/manifest.json" "$SRC/native/ffmpeg/pin.json" > "$OUT/rc-v3-pin.txt" 2>&1 <<'PY'
import json,sys
m=json.load(open(sys.argv[1])); p=json.load(open(sys.argv[2]))
pin=m.get('pin') or {}
print("manifest.ffmpeg_commit      =%s" % m.get('ffmpeg_commit'))
print("manifest.pin.ffmpeg_commit_sha=%s" % pin.get('ffmpeg_commit_sha'))
print("pin.json.ffmpeg_commit_sha  =%s" % p.get('ffmpeg_commit_sha'))
print("manifest.ffmpeg_source_sha256=%s" % m.get('ffmpeg_source_sha256'))
print("manifest.pin.ffmpeg_source_sha256=%s" % pin.get('ffmpeg_source_sha256'))
print("pin.json.source_sha256      =%s" % p.get('source_sha256'))
print("V3_chain_1=%s" % (m.get('ffmpeg_commit')==pin.get('ffmpeg_commit_sha')==p.get('ffmpeg_commit_sha')))
print("V3_chain_2=%s" % (m.get('ffmpeg_source_sha256')==pin.get('ffmpeg_source_sha256')==p.get('source_sha256')))
print("V3_tag=%s" % (pin.get('ffmpeg_tag')==p.get('ffmpeg_tag')))
print("V3_pin_sha=%s" % (pin.get('pin_sha256')==__import__('hashlib').sha256(open(sys.argv[2],'rb').read()).hexdigest()))
print("V3_profile_sha=%s" % (m.get('ffmpeg_profile_sha256')=='57fb70ed2c29a8464d939c611b3c5ed1bc99879438eb7f159530dfa52de554fa'))
PY
echo "exit=$?"; cat "$OUT/rc-v3-pin.txt"

mark "H5 §2 version plumbing in ops/"
grep -n 'version' ops/package-rc.ps1 | head -40
echo "exit=$?"
echo "-- xmake.lua handling in ops/build-rc.ps1 around line 150-160"
sed -n '145,170p' ops/build-rc.ps1
echo "exit=$?"

mark "H6 does any release/package script read the xmake project version?"
grep -rn --include='*.ps1' --include='*.py' --include='*.sh' -e 'set_version' -e 'xmake show' -e 'project.*version' ops tools 2>/dev/null | head -20
echo "exit=$?"
echo "########## DONE ##########"
