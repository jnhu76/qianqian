#!/usr/bin/env bash
# Lane B: final cross-checks (v0.1.0 carried-field types, direct configure-arg
# grep on the imported closure manifests, release-claim scan).
set -uo pipefail
V010=/home/hoo/s2pkg-v010/songcore-v0.1.0-linux-x86_64.tar.gz
TMP=$(mktemp -d)
tar -xzf "$V010" -C "$TMP" songcore-v0.1.0-linux-x86_64/metadata/manifest.json
echo "===== v0.1.0 carried field types ====="
python3 - "$TMP/songcore-v0.1.0-linux-x86_64/metadata/manifest.json" <<'PY'
import json, sys
m = json.load(open(sys.argv[1]))
for k in ("schema_version", "artifact", "artifact_shared", "shared_sha256", "toolchain", "target"):
    print("%-16s %-10s %r" % (k, type(m.get(k)).__name__, m.get(k)))
print("fields:", len(m))
PY
rm -rf "$TMP"

echo "===== direct configure-arg scan on the imported closure manifests ====="
for m in /mnt/c/Users/fred1/source/qianqian/native/build/ffmpeg-xmake/manifest.json \
         /home/hoo/g4-172/linux-root/native/build/ffmpeg-xmake/manifest.json; do
  echo "-- $m ($(sha256sum "$m" | cut -d' ' -f1))"
  python3 - "$m" <<'PY'
import json, sys
d = json.load(open(sys.argv[1]))
args = d["configure_args"]
forb = [a for a in args if a.startswith(("--enable-gpl", "--enable-version3", "--enable-nonfree", "--enable-lib"))]
print("configure_args=%d forbidden=%s" % (len(args), forb or "none"))
print("sample:", args[:8])
PY
done

echo "===== release-claim scan inside both packages ====="
grep -rniI "BYTE_REPRODUCIBLE\|SEMANTIC_ABI_VERIFIED\|RECIPE_RECONSTRUCTIBLE\|#329\|RELEASE\b" \
  /mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/g4/package/win/extract/songcore-v0.0.0-preflight-windows-x86_64/metadata/manifest.json \
  /mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/g4/package/linux/extract/songcore-v0.0.0-preflight-linux-x86_64/metadata/manifest.json \
  | head -20 || echo "no positive-level or #329 claim in either manifest"
