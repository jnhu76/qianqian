#!/usr/bin/env bash
# Lane B: extract both containers into the evidence area (Windows-visible via
# /mnt/c) and cross-check against the packagers' own staged trees.
set -uo pipefail
EV=/mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/g4/package
LDIR=/home/hoo/g4-172/laneB-packages/linux
ROOT=songcore-v0.0.0-preflight-linux-x86_64
WROOT=songcore-v0.0.0-preflight-windows-x86_64

mkdir -p "$EV/linux/extract"
rm -rf "$EV/linux/extract/$ROOT"
tar -xzf "$LDIR/$ROOT.tar.gz" -C "$EV/linux/extract"
echo "linux_extract_rc=$?"
cp -f "$LDIR/$ROOT.tar.gz" "$EV/linux/$ROOT.tar.gz"
cp -f "$LDIR/SHA256SUMS" "$EV/linux/SHA256SUMS"
cp -f "$LDIR/v-checks.json" "$EV/linux/v-checks.json"
cp -f "$LDIR/package-summary.json" "$EV/linux/package-summary.json"

echo "===== extracted tree: linux ====="
find "$EV/linux/extract/$ROOT" -type f | sed "s|$EV/linux/extract/||" | sort
echo "file_count=$(find "$EV/linux/extract/$ROOT" -type f | wc -l)"
echo "--- sha256sum -c ---"
( cd "$EV/linux/extract/$ROOT" && sha256sum -c metadata/checksums.txt >/dev/null && echo CHECKSUMS_OK )

echo "===== WSL stage vs evidence extract (linux) ====="
diff -r "/home/hoo/g4-172/laneB-stage/linux/$ROOT" "$EV/linux/extract/$ROOT" && echo LINUX_STAGE_IDENTICAL

echo "===== windows: packager stage vs evidence extract ====="
diff -r "$EV/win/stage/$WROOT" "$EV/win/extract/$WROOT" && echo WIN_STAGE_IDENTICAL
echo "win_file_count=$(find "$EV/win/extract/$WROOT" -type f | wc -l)"
echo "win_lib_listing:"
find "$EV/win/extract/$WROOT/lib" -maxdepth 2 | sed "s|$EV/win/extract/$WROOT/||"
if [ -e "$EV/win/extract/$WROOT/lib/shared" ]; then echo "WIN_HAS_LIB_SHARED (unexpected)"; else echo "WIN_NO_LIB_SHARED (expected)"; fi
echo "--- sha256sum -c ---"
( cd "$EV/win/extract/$WROOT" && sha256sum -c metadata/checksums.txt >/dev/null && echo CHECKSUMS_OK )

echo "===== stage tree sizes ====="
du -sb "$EV/win/extract/$WROOT" "$EV/linux/extract/$ROOT"

echo "===== V6 evidence pointer files (windows manifest) ====="
for f in identity.txt member-order.verdict member-diff.verdict differing-members.txt rootcause-strings.txt \
         archive-path-scan.txt debug-info-sample.txt songcore-c-object.txt undefined-imports.txt closure-identity.txt; do
  p="/mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/package/repro/$f"
  if [ -f "$p" ]; then echo "present $f"; else echo "MISSING $f"; fi
done
