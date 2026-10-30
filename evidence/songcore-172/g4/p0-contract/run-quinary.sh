#!/usr/bin/env bash
# p0-contract lane, quinary (read-only): exact §7 content set on the staged RC
# package and inside the RC zip; checksums/files arithmetic.
set -u
SRC=/home/hoo/Source/qianqian
WIN=/mnt/c/Users/fred1/source/qianqian
OUT=$WIN/evidence/songcore-172/g4/p0-contract
PKG=$SRC/evidence/songcore-172/package/stage/songcore-v0.2.0-rc.1-windows-x86_64
ZIP=$SRC/evidence/songcore-172/package/songcore-v0.2.0-rc.1-windows-x86_64.zip
cd "$SRC" || exit 99
mark(){ echo; echo "########## $* ##########"; }

mark "G1 full staged file list"
find "$PKG" -type f | sed "s|$PKG/||" | sort > "$OUT/rc-staged-files.txt"
echo "exit=$?"; cat "$OUT/rc-staged-files.txt"
echo "count=$(wc -l < "$OUT/rc-staged-files.txt")"

mark "G2 zip entries (python zipfile: all entries vs file entries)"
python3 - "$ZIP" > "$OUT/rc-zip-entries.txt" 2>&1 <<'PY'
import zipfile,sys
z=zipfile.ZipFile(sys.argv[1])
names=z.namelist()
print("all_entries=%d" % len(names))
files=[n for n in names if not n.endswith('/')]
print("file_entries=%d" % len(files))
print("dir_entries=%d" % (len(names)-len(files)))
for n in sorted(names): print("  %s" % n)
PY
echo "exit=$?"; cat "$OUT/rc-zip-entries.txt"

mark "G3 checksums.txt raw lines"
awk '{printf "%2d: %s\n", NR, $0}' "$PKG/metadata/checksums.txt" > "$OUT/rc-checksums-lines.txt" 2>&1
echo "exit=$?"; cat "$OUT/rc-checksums-lines.txt"
echo "nonempty_lines=$(awk 'NF' "$PKG/metadata/checksums.txt" | wc -l)"
echo "sha256_rows=$(awk 'NF>=2 && $1 ~ /^[0-9a-f]{64}$/' "$PKG/metadata/checksums.txt" | wc -l)"

mark "G4 set equality: checksums rows == all files minus checksums.txt"
awk 'NF>=2 && $1 ~ /^[0-9a-f]{64}$/{print $2}' "$PKG/metadata/checksums.txt" | sed 's|^\*||' | sed 's|^\./||' | sort > "$OUT/rc-checksums-paths.txt"
grep -v '^metadata/checksums.txt$' "$OUT/rc-staged-files.txt" > "$OUT/rc-staged-files-minus-checksums.txt"
echo "checksums_paths=$(wc -l < "$OUT/rc-checksums-paths.txt")"
echo "staged_minus_checksums=$(wc -l < "$OUT/rc-staged-files-minus-checksums.txt")"
if diff -u "$OUT/rc-staged-files-minus-checksums.txt" "$OUT/rc-checksums-paths.txt" > "$OUT/rc-checksums-set-diff.txt" 2>&1; then
  echo "G4=SET_EQUAL"
else
  echo "G4=SET_DIFF"; cat "$OUT/rc-checksums-set-diff.txt"
fi

mark "G5 files{} keys vs checksums minus manifest.json"
python3 - "$PKG" > "$OUT/rc-files-vs-checksums.txt" 2>&1 <<'PY'
import json,sys,os
pkg=sys.argv[1]
d=json.load(open(os.path.join(pkg,'metadata','manifest.json')))
fk=sorted((d.get('files') or {}).keys())
rows=[]
for line in open(os.path.join(pkg,'metadata','checksums.txt'), encoding='utf-8'):
    parts=line.split()
    if len(parts)>=2:
        rows.append(parts[1].lstrip('*').lstrip('./'))
rows=sorted(rows)
print("files_keys=%d checksums_rows=%d" % (len(fk), len(rows)))
print("files_keys_minus_expected=%s" % sorted(set(fk)-set(rows)))
print("checksums_minus_files_keys=%s" % sorted(set(rows)-set(fk)))
PY
echo "exit=$?"; cat "$OUT/rc-files-vs-checksums.txt"

mark "G6 §8 V1 claim vs measured"
grep -n 'windows RC 14 package files' docs/release/songcore-release-contract.md
echo "exit=$?"
echo "########## DONE ##########"
