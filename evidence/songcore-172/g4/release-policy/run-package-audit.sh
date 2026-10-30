#!/usr/bin/env bash
# Lane F: read-only content audit of the two existing SongCore packages.
set -uo pipefail
EVID=/mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/g4/release-policy
ZIP=/mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/package/songcore-v0.2.0-rc.1-windows-x86_64.zip
TGZ=/home/hoo/s2pkg-v010/songcore-v0.1.0-linux-x86_64.tar.gz
{
  echo "source_sha: $(git -C /home/hoo/Source/qianqian rev-parse HEAD)"
  echo "start_utc: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
} > "$EVID/package-audit.meta.txt"

python3 "$EVID/check_package_security.py" "$ZIP" \
  --expected-root songcore-v0.2.0-rc.1-windows-x86_64 \
  --expected-sha256 7b4d0615d25af464739373a0a3565f77f3208ce9c263f59e578d443801536415 \
  --out "$EVID/package-audit-windows-zip.json" > "$EVID/package-audit-windows-zip.log" 2>&1
echo "zip_audit_exit=$?" >> "$EVID/package-audit.meta.txt"

python3 "$EVID/check_package_security.py" "$TGZ" \
  --expected-root songcore-v0.1.0-linux-x86_64 \
  --expected-sha256 018e0cbdfd8d83b6967bbba8e899ae6dbcc2ccf690c5294bd48e2ef2699f6815 \
  --out "$EVID/package-audit-linux-targz.json" > "$EVID/package-audit-linux-targz.log" 2>&1
echo "targz_audit_exit=$?" >> "$EVID/package-audit.meta.txt"
echo "end_utc: $(date -u +%Y-%m-%dT%H:%M:%SZ)" >> "$EVID/package-audit.meta.txt"
echo "=== zip (exit recorded in meta) ==="; head -c 1200 "$EVID/package-audit-windows-zip.log"
echo "=== tar (exit recorded in meta) ==="; head -c 1200 "$EVID/package-audit-linux-targz.log"
echo "=== meta ==="; cat "$EVID/package-audit.meta.txt"
