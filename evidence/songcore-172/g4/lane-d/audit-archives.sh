#!/usr/bin/env bash
set -u
R=/mnt/c/Users/fred1/source/qianqian
L=$R/evidence/songcore-172/g4/package/linux/extract/songcore-v0.0.0-preflight-linux-x86_64
W=$R/evidence/songcore-172/g4/package/win/extract/songcore-v0.0.0-preflight-windows-x86_64

for T in "$L" "$W"; do
  echo "=== $T ==="
  echo "-- ar t count --"
  ar t "$T/lib/libsongcore.a" > /tmp/members.txt 2>/tmp/ar.err
  echo "ar_exit=$?"
  wc -l < /tmp/members.txt
  echo "-- distinct top prefixes --"
  sed 's#/.*##' /tmp/members.txt | sort | uniq -c | sort -rn | head -20
  echo "-- ffmpeg-looking members (libav*/libsw*) --"
  grep -c '^lib\(av\|sw\)' /tmp/members.txt
  echo "-- sample --"
  grep '^lib\(av\|sw\)' /tmp/members.txt | head -5
  echo "-- songcore members --"
  grep -i 'songcore' /tmp/members.txt | head -5
  echo "-- member extension histogram --"
  sed 's/.*\.//' /tmp/members.txt | sort | uniq -c | head
  echo "-- ar stderr --"
  cat /tmp/ar.err
done

echo "== checksums.txt self-consistency (linux) =="
python3 - "$L" <<'PY'
import sys, hashlib, os
root = sys.argv[1]
listed = {}
for line in open(os.path.join(root, "metadata/checksums.txt"), encoding="utf-8"):
    line = line.rstrip("\n")
    if not line: continue
    h, p = line.split(None, 1)
    listed[p.strip()] = h
actual = {}
for dp, _, fns in os.walk(root):
    for fn in fns:
        fp = os.path.join(dp, fn)
        rel = os.path.relpath(fp, root).replace("\\", "/")
        if rel == "metadata/checksums.txt": continue
        actual[rel] = hashlib.sha256(open(fp, "rb").read()).hexdigest()
print("listed", len(listed), "actual_excl_checksums", len(actual))
print("missing_from_checksums", sorted(set(actual) - set(listed)))
print("extra_in_checksums", sorted(set(listed) - set(actual)))
print("hash_mismatch", sorted(k for k in set(listed) & set(actual) if listed[k] != actual[k]))
PY
echo "linux_checksum_exit=$?"

echo "== checksums.txt self-consistency (windows) =="
python3 - "$W" <<'PY'
import sys, hashlib, os
root = sys.argv[1]
listed = {}
for line in open(os.path.join(root, "metadata/checksums.txt"), encoding="utf-8"):
    line = line.rstrip("\n")
    if not line: continue
    h, p = line.split(None, 1)
    listed[p.strip()] = h
actual = {}
for dp, _, fns in os.walk(root):
    for fn in fns:
        fp = os.path.join(dp, fn)
        rel = os.path.relpath(fp, root).replace("\\", "/")
        if rel == "metadata/checksums.txt": continue
        actual[rel] = hashlib.sha256(open(fp, "rb").read()).hexdigest()
print("listed", len(listed), "actual_excl_checksums", len(actual))
print("missing_from_checksums", sorted(set(actual) - set(listed)))
print("extra_in_checksums", sorted(set(listed) - set(actual)))
print("hash_mismatch", sorted(k for k in set(listed) & set(actual) if listed[k] != actual[k]))
PY
echo "win_checksum_exit=$?"

echo "== O4 forbidden configure args scan =="
python3 - "$L" "$W" <<'PY'
import sys, json, re
for root in sys.argv[1:]:
    m = json.load(open(root + "/metadata/manifest.json", encoding="utf-8"))
    args = m["closure"]["configure_args"]
    bad = [a for a in args if re.match(r"^--enable-(gpl|version3|nonfree|lib)", a)]
    print(root.rsplit("/",1)[-1], "nargs", len(args), "forbidden", bad,
          "license", m["license"]["ffmpeg_effective"],
          "gpl", m["license"]["gpl_enabled"],
          "v3", m["license"]["version3_enabled"],
          "nonfree", m["license"]["nonfree_enabled"],
          "external_libs", m["license"]["external_libs"])
PY
echo "o4_scan_exit=$?"
