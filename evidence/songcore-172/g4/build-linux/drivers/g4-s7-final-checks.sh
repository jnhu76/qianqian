#!/usr/bin/env bash
# g4-s7-final-checks.sh — final measured snapshot; writes EV/final-checks.txt
# and nothing else. Run before the summary generator so the file is hashed.
set -Eeuo pipefail
export LC_ALL=C
ROOT=/home/hoo/g4-172/linux-root
EV=$ROOT/receipts/g4-linux-build
F=$EV/final-checks.txt
: > "$F"

{
echo "== full dynamic defined-export census of the shared artifact =="
nm -D --defined-only "$ROOT/src/native/build/artifacts/shared/libsongcore.so" \
  | awk '$2=="T"||$2=="W"||$2=="D"||$2=="B"{print $2, $3}' | sort > "$EV/so-all-exports.txt"
wc -l < "$EV/so-all-exports.txt"
echo "-- non-song exports (expected: none) --"
grep -v ' song' "$EV/so-all-exports.txt" | head -20 || true
echo "== archive member name occurrence census =="
sort "$EV/verify/ar-members.txt" | uniq -c | sort -rn | head -12

echo "== toolchain lines =="
uname -sr
gcc --version | head -1
xmake --version >"$EV/xmake-version-raw.txt" 2>&1 || true
sed 's/\x1b\[[0-9;]*m//g' "$EV/xmake-version-raw.txt" | head -1

echo "== artifact stat =="
stat -c '%s bytes  %n' "$ROOT/src/native/build/artifacts/libsongcore.a" "$ROOT/src/native/build/artifacts/shared/libsongcore.so"
sha256sum "$ROOT/src/native/build/artifacts/libsongcore.a" "$ROOT/src/native/build/artifacts/shared/libsongcore.so"
} | tee -a "$F"
echo "final checks written: $F"
