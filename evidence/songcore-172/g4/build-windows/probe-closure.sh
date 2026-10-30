#!/usr/bin/env bash
# Probe the candidate imported closures: manifest target / identity fields.
set -uo pipefail
for d in /home/hoo/q172rc/closure /home/hoo/q172d2-verify/closure; do
  echo "=== $d"
  m="$d/ffmpeg-xmake/manifest.json"
  if [ -f "$m" ]; then
    python3 - "$m" <<'PY'
import json, sys, hashlib
m = json.load(open(sys.argv[1]))
print("target            =", m.get("target"))
print("profile           =", m.get("profile"))
print("profile_sha256    =", m.get("profile_sha256"))
print("units             =", len(m.get("units", [])))
print("ffmpeg_tag        =", m.get("ffmpeg_tag"))
print("ffmpeg_commit_sha =", m.get("ffmpeg_commit_sha"))
print("ffmpeg_source_sha256 =", m.get("ffmpeg_source_sha256"))
print("recipe_sha256     =", (m.get("target") or {}).get("recipe_sha256"))
print("toolchain         =", json.dumps(m.get("toolchain"), sort_keys=True))
print("manifest_sha256   =", hashlib.sha256(open(sys.argv[1],'rb').read()).hexdigest())
PY
  else
    echo "MISSING manifest: $m"
  fi
  for f in "$d/ffmpeg-src/.qianqian-verified" "$d/ffmpeg-xmake/oracle/config_components.h"; do
    if [ -f "$f" ]; then echo "present: $f"; else echo "MISSING: $f"; fi
  done
done
