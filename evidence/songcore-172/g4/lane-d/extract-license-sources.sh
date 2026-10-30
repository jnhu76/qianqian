#!/usr/bin/env bash
# Lane D: extract FFmpeg COPYING texts from the pinned zip and the v0.1.0 asset,
# hash them, and compare against the shipped package copies.
set -u
OUT=/home/hoo/g4-172/laneD-license
mkdir -p "$OUT/zip" "$OUT/v010"

ZIP=/home/hoo/q172d2-verify/win/native/build/downloads/ffmpeg-n9.0.1.zip
echo "== zip sha256 =="
sha256sum "$ZIP"
echo "zip_sha256_exit=$?"

echo "== zip top-level entry =="
python3 - "$ZIP" <<'PY'
import sys, zipfile
z = zipfile.ZipFile(sys.argv[1])
names = z.namelist()
roots = sorted({n.split('/')[0] for n in names})
print("roots:", roots)
pat = ("COPYING.GPLv2","COPYING.GPLv3","COPYING.LGPLv2.1","COPYING.LGPLv3")
for p in pat:
    hits = [n for n in names if n.split('/')[-1] == p]
    print("match", p, hits)
print("total_entries", len(names))
PY
echo "zip_list_exit=$?"

echo "== extract COPYING.* from zip =="
python3 - "$ZIP" "$OUT/zip" <<'PY'
import sys, zipfile, os, hashlib
z = zipfile.ZipFile(sys.argv[1]); out = sys.argv[2]
pat = ("COPYING.GPLv2","COPYING.GPLv3","COPYING.LGPLv2.1","COPYING.LGPLv3")
for p in pat:
    hits = [n for n in z.namelist() if n.split('/')[-1] == p]
    assert len(hits) == 1, (p, hits)
    data = z.read(hits[0])
    open(os.path.join(out, p), "wb").write(data)
    print(p, len(data), hashlib.sha256(data).hexdigest(), "<-", hits[0])
PY
echo "zip_extract_exit=$?"

echo "== v0.1.0 asset =="
TAR=/home/hoo/s2pkg-v010/songcore-v0.1.0-linux-x86_64.tar.gz
sha256sum "$TAR"
echo "tar_sha256_exit=$?"
tar -tzf "$TAR" | sort
echo "tar_list_exit=$?"
tar -xzf "$TAR" -C "$OUT/v010"
echo "tar_extract_exit=$?"
echo "-- v0.1.0 LICENSES --"
ls -l "$OUT/v010"/*/LICENSES/
echo "v010_ls_exit=$?"
sha256sum "$OUT/v010"/*/LICENSES/*
echo "v010_sha_exit=$?"

echo "== profile sha256 =="
sha256sum /home/hoo/Source/qianqian/native/ffmpeg/profiles/codec-base.json
echo "profile_sha_exit=$?"
