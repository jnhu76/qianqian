#!/usr/bin/env bash
# p0-contract lane, quaternary (read-only): §2 identity chain measured against
# the staged RC package + §8 key-collision shape in a real schema-2 manifest.
set -u
SRC=/home/hoo/Source/qianqian
WIN=/mnt/c/Users/fred1/source/qianqian
OUT=$WIN/evidence/songcore-172/g4/p0-contract
cd "$SRC" || { echo "FATAL cd"; exit 99; }
mark(){ echo; echo "########## $* ##########"; }

mark "F1 package receipt identity fields"
python3 - "$SRC/evidence/songcore-172/package/package-summary.json" > "$OUT/rc-receipt-identity.txt" 2>&1 <<'PY'
import json,sys
d=json.load(open(sys.argv[1]))
for k in ('version','version_status','container','out_dir','package_root','artifact','artifact_sha256','artifact_bytes','source','project','script'):
    print("%s=%s" % (k, json.dumps(d.get(k))))
PY
echo "exit=$?"; cat "$OUT/rc-receipt-identity.txt"

mark "F2 staged RC package tree"
ls -la "$SRC/evidence/songcore-172/package" > "$OUT/rc-package-dir.txt" 2>&1
echo "exit=$?"; cat "$OUT/rc-package-dir.txt"

mark "F3 staged schema-2 manifest identity fields"
MF=$(find "$SRC/evidence/songcore-172/package" -path '*metadata/manifest.json' | head -1)
echo "manifest=$MF"
if [ -n "$MF" ]; then
  sha256sum "$MF"
  python3 - "$MF" "$SRC" > "$OUT/rc-manifest-identity.txt" 2>&1 <<'PY'
import json,sys,os
mp, src = sys.argv[1], sys.argv[2]
d=json.load(open(mp))
print("schema_version=%s songcore_version=%s songcore_abi=%s" % (d.get('schema_version'), d.get('songcore_version'), d.get('songcore_abi')))
print("abi_header_sha256=%s" % d.get('abi_header_sha256'))
print("target=%r (type %s)" % (d.get('target'), type(d.get('target')).__name__))
print("toolchain=%r (type %s)" % (d.get('toolchain'), type(d.get('toolchain')).__name__))
ti=d.get('target_identity') or {}
print("target_identity.id=%s triple=%s recipe_sha256=%s" % (ti.get('id'), ti.get('triple'), ti.get('recipe_sha256')))
print("target_identity.toolchain=%s" % json.dumps(ti.get('toolchain')))
pin=d.get('pin') or {}
print("pin=%s" % json.dumps(pin, sort_keys=True))
print("ffmpeg_commit=%s ffmpeg_source_sha256=%s ffmpeg_profile_sha256=%s" % (d.get('ffmpeg_commit'), d.get('ffmpeg_source_sha256'), d.get('ffmpeg_profile_sha256')))
print("carried_duplicates_equal=%s" % (
    d.get('ffmpeg_commit') == pin.get('ffmpeg_commit_sha') and
    d.get('ffmpeg_source_sha256') == pin.get('ffmpeg_source_sha256')))
print("tools=%s" % json.dumps(d.get('tools'), sort_keys=True))
print("sdk_ndk=%r" % d.get('sdk_ndk'))
print("runtime_dependencies=%s" % json.dumps(d.get('runtime_dependencies'), sort_keys=True))
print("files_count=%s" % len(d.get('files') or {}))
print("public_symbols=%s" % ",".join(d.get('public_symbols') or []))
rep=d.get('reproducibility') or {}
print("reproducibility=%s" % json.dumps(rep, sort_keys=True))
print("package_root_dirname=%s" % os.path.basename(os.path.dirname(os.path.dirname(mp))))
PY
  echo "exit=$?"; cat "$OUT/rc-manifest-identity.txt"
else
  echo "no staged manifest found"
fi

mark "F4 §2 equality chain, measured values side by side"
echo "xmake set_version        = $(awk -F'"' '/set_version/{print $2}' native/xmake.lua)"
echo "xmake set_project        = $(awk -F'"' '/set_project/{print $2}' native/xmake.lua)"
echo "RC manifest songcore_version = $(awk -F= '/^schema_version=/{next} /songcore_version=/{print $2}' "$OUT/rc-manifest-identity.txt" 2>/dev/null | head -1)"
echo "RC package dir name      = $(awk -F= '/^package_root_dirname=/{print $2}' "$OUT/rc-manifest-identity.txt" 2>/dev/null)"
echo "git tag list (songcore)  = $(git tag -l 'songcore-v*' | tr '\n' ' ')"

mark "F5 contract §2 tag-form vs the RC version string"
grep -n 'songcore-vMAJOR.MINOR.PATCH\|decimal components' docs/release/songcore-release-contract.md
echo "exit=$?"

mark "F6 checksums/files set arithmetic measured on the staged package"
PKG=$(dirname "$(dirname "$MF")" 2>/dev/null)
echo "pkg=$PKG"
if [ -n "$PKG" ] && [ -d "$PKG" ]; then
  echo "packaged_files=$(find "$PKG" -type f | wc -l)"
  echo "checksums_entries=$(awk 'NF' "$PKG/metadata/checksums.txt" 2>/dev/null | wc -l)"
  echo "files_entries=$(python3 -c "import json,sys;print(len(json.load(open(sys.argv[1])).get('files') or {}))" "$PKG/metadata/manifest.json")"
  echo "exit=$?"
fi
echo "########## DONE ##########"
