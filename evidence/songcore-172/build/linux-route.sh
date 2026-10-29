#!/bin/bash
# songcore-172 D2: Linux route — shared-form no-regression, measured.
# The lin tree already holds the linux-x86_64 FFmpeg closure (import done).
# Phase 1 (baseline): pre-fix songcore.lua (git 40ae32a) -> record artifact set.
# Phase 2 (fix):      post-fix songcore.lua          -> record artifact set + audit.
# Regression = the post-fix artifact set / export surface differs from baseline.
set -uo pipefail
export PATH=/home/hoo/.local/bin:$PATH
L=/home/hoo/q172d2/lin
OUT=/home/hoo/q172d2/linux-green
mkdir -p "$OUT"
cd "$L/native"

echo "=== phase 1: PRE-FIX baseline (songcore.lua @ 40ae32a) ==="
(cd /mnt/c/Users/fred1/source/qianqian-172 && git show 40ae32a:native/build/songcore.lua) > build/songcore.lua
sha256sum build/songcore.lua
xmake f -p linux -a x86_64 -m release -y > /dev/null 2>&1; echo "F_EXIT=$?"
rm -rf build/artifacts
xmake build -r songcore > "$OUT/linux-prefix-build.log" 2>&1
echo "PREFIX_BUILD_EXIT=$?"
tail -3 "$OUT/linux-prefix-build.log"
echo "--- pre-fix artifacts:"
find build/artifacts -type f | sort
find build/artifacts -type f -exec sha256sum {} \; | sort -k2 > "$OUT/prefix-artifacts.txt"
x86_64-w64-mingw32-nm --version > /dev/null 2>&1 || true
nm -D build/artifacts/shared/libsongcore.so 2>/dev/null | awk '$2=="T"||$2=="W"{print $3}' | grep -c "^song" > "$OUT/prefix-so-songcount.txt" || echo 0 > "$OUT/prefix-so-songcount.txt"
echo "pre-fix .so exported song* count: $(cat "$OUT/prefix-so-songcount.txt")"

echo "=== phase 2: POST-FIX ==="
cp /mnt/c/Users/fred1/source/qianqian-172/native/build/songcore.lua build/songcore.lua
sha256sum build/songcore.lua
xmake f -p linux -a x86_64 -m release -y > /dev/null 2>&1; echo "F_EXIT=$?"
rm -rf build/artifacts
xmake build -r songcore > "$OUT/linux-postfix-build.log" 2>&1
echo "POSTFIX_BUILD_EXIT=$?"
tail -3 "$OUT/linux-postfix-build.log"
echo "--- post-fix artifacts:"
find build/artifacts -type f | sort
find build/artifacts -type f -exec sha256sum {} \; | sort -k2 > "$OUT/postfix-artifacts.txt"
nm -D build/artifacts/shared/libsongcore.so 2>/dev/null | awk '$2=="T"||$2=="W"{print $3}' | grep -c "^song" > "$OUT/postfix-so-songcount.txt" || echo 0 > "$OUT/postfix-so-songcount.txt"
echo "post-fix .so exported song* count: $(cat "$OUT/postfix-so-songcount.txt")"

echo "=== phase 3: regression comparison ==="
echo "--- artifact set diff (pre vs post):"
diff "$OUT/prefix-artifacts.txt" "$OUT/postfix-artifacts.txt" && echo "ARTIFACT_SETS_IDENTICAL"
echo "--- explicit forms on linux (post-fix):"
xmake build songcore_shared > /dev/null 2>&1; echo "EXPLICIT_shared_EXIT=$?"
xmake build songcore_static > /dev/null 2>&1; echo "EXPLICIT_static_EXIT=$?"

echo "=== phase 4: ELF audit of post-fix artifacts ==="
objdump -f build/artifacts/libsongcore.a 2>/dev/null | grep -c "file format elf64-x86-64" || true
objdump -f build/artifacts/shared/libsongcore.so 2>/dev/null | grep -m1 "file format"
nm -D build/artifacts/shared/libsongcore.so | grep " T song" | wc -l
echo LINUX_DONE
