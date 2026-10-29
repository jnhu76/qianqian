#!/bin/bash
# songcore-172 D2: GREEN verification v2 on the mingw (windows-x86_64) session.
# 1. xmake build -r songcore     -> exit 0, static artifacts only
# 2. xmake build songcore_static -> exit 0 (explicit static still fine)
# 3. xmake build songcore_shared -> exit != 0 with recipe-policy message
# 4. xmake build songcore_artifact_gate -> exit != 0 (explicit gate)
# 5. no-recipe session (plat cross) -> gate fires "no target recipe matches"
# 6. artifact identity: 280 pe-x86-64 members, 0 elf, 15 song* ABI symbols
# 7. undefined-symbol census of the merged archive (link-contract input)
set -uo pipefail
export PATH=/home/hoo/.local/bin:$PATH
W=/home/hoo/q172d2/win
OUT=/home/hoo/q172d2/green
mkdir -p "$OUT"
cd "$W/native"

cp /mnt/c/Users/fred1/source/qianqian-172/native/build/songcore.lua build/songcore.lua
sha256sum build/songcore.lua

rm -rf build/artifacts
xmake build -r songcore > "$OUT/green-songcore.log" 2>&1
RC1=$?
echo "GREEN_songcore_exit=$RC1"
tail -4 "$OUT/green-songcore.log"
echo "--- artifacts after GREEN songcore build:"
ls -la build/artifacts/
[ -d build/artifacts/shared ] && { echo "UNEXPECTED shared dir:"; ls -la build/artifacts/shared/; } || echo "no shared dir (expected)"

echo "=== explicit songcore_static ==="
xmake build songcore_static > "$OUT/green-static.log" 2>&1
echo "GREEN_songcore_static_exit=$?"

echo "=== explicit songcore_shared (expect fail-closed) ==="
xmake build songcore_shared > "$OUT/green-shared-explicit.log" 2>&1
echo "GREEN_songcore_shared_exit=$?"
tail -4 "$OUT/green-shared-explicit.log"

echo "=== explicit songcore_artifact_gate (expect fail-closed) ==="
xmake build songcore_artifact_gate > "$OUT/green-gate-explicit.log" 2>&1
echo "GREEN_songcore_gate_exit=$?"
tail -4 "$OUT/green-gate-explicit.log"

echo "=== no-recipe session (plat cross, fake sdk) ==="
xmake f -p cross -a x86_64 -m release --sdk=/tmp/q172d2-fake-sdk -y > "$OUT/green-cross-config.log" 2>&1
echo "CROSS_CONFIG_EXIT=$?"
tail -2 "$OUT/green-cross-config.log"
xmake build songcore > "$OUT/green-cross-build.log" 2>&1
echo "CROSS_BUILD_EXIT=$? (expect nonzero: gate says no recipe matches)"
tail -4 "$OUT/green-cross-build.log"

echo "=== restore mingw session ==="
xmake f -p mingw -a x86_64 -m release -y > /dev/null 2>&1
echo "RECONFIG_EXIT=$?"

echo "=== artifact identity audit ==="
if [ -f build/artifacts/libsongcore.a ]; then
  x86_64-w64-mingw32-objdump -f build/artifacts/libsongcore.a > "$OUT/objdump-f.txt" 2>&1
  echo "pe_x86_64_members=$(grep -c 'file format pe-x86-64' "$OUT/objdump-f.txt")"
  echo "elf_members=$(grep -c 'file format elf' "$OUT/objdump-f.txt")"
  echo "song_abi_symbols=$(x86_64-w64-mingw32-nm build/artifacts/libsongcore.a 2>/dev/null | grep -c ' T song')"
  sha256sum build/artifacts/libsongcore.a
else
  echo "FATAL: libsongcore.a missing"
fi
echo GREEN_DONE
