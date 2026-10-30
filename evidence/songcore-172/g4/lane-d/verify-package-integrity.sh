#!/usr/bin/env bash
set -u
R=/mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/g4/package
L=$R/linux/extract/songcore-v0.0.0-preflight-linux-x86_64
W=$R/win/extract/songcore-v0.0.0-preflight-windows-x86_64

echo "== README-documented verification command: sha256sum -c (linux) =="
( cd "$L" && sha256sum -c metadata/checksums.txt ); echo "linux_sha256sum_c_exit=$?"
echo "== README-documented verification command: sha256sum -c (windows) =="
( cd "$W" && sha256sum -c metadata/checksums.txt ); echo "win_sha256sum_c_exit=$?"

echo "== manifest files{}/license_files{} vs actual bytes (size + sha) =="
python3 - "$L" "$W" <<'PY'
import sys, json, os, hashlib
for root in sys.argv[1:]:
    m = json.load(open(root + "/metadata/manifest.json", encoding="utf-8"))
    bad = []
    for rel, rec in m["files"].items():
        p = os.path.join(root, rel)
        data = open(p, "rb").read()
        if hashlib.sha256(data).hexdigest() != rec["sha256"] or len(data) != rec["bytes"]:
            bad.append(rel)
    lic = m["license"]["license_files"]
    lic_bad = [n for n, h in lic.items()
               if hashlib.sha256(open(os.path.join(root, "LICENSES", n), "rb").read()).hexdigest() != h]
    print(root.rsplit("/", 1)[-1],
          "files_entries", len(m["files"]), "files_bad", bad,
          "license_files_entries", len(lic), "license_files_bad", lic_bad,
          "manifest_self_listed", "metadata/manifest.json" in m["files"],
          "checksums_listed_in_files", "metadata/checksums.txt" in m["files"])
PY
echo "manifest_files_exit=$?"

echo "== files on disk not covered by files{} (excluding manifest.json) =="
python3 - "$L" "$W" <<'PY'
import sys, json, os
for root in sys.argv[1:]:
    m = json.load(open(root + "/metadata/manifest.json", encoding="utf-8"))
    on_disk = set()
    for dp, _, fns in os.walk(root):
        for fn in fns:
            on_disk.add(os.path.relpath(os.path.join(dp, fn), root).replace("\\", "/"))
    covered = set(m["files"]) | {"metadata/manifest.json"}
    print(root.rsplit("/", 1)[-1], "uncovered", sorted(on_disk - covered))
PY
echo "coverage_exit=$?"

echo "== license text line endings / BOM =="
for T in "$L" "$W"; do
  for f in "$T"/LICENSES/COPYING.*; do
    printf '%s ' "$(basename "$f")"
    python3 -c "import sys;d=open(sys.argv[1],'rb').read();print('bom',d[:3]==b'\xef\xbb\xbf','crlf',b'\r\n' in d,'bytes',len(d))" "$f"
  done
done
echo "eol_exit=$?"
