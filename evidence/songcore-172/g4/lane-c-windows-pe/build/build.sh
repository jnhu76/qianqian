#!/usr/bin/env bash
# Lane C (windows) — cross-build the PE consumer against the packaged
# windows-x86_64 SongCore archive. Every command's exit code is recorded.
set -u
PKG=/mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/g4/package/win/extract/songcore-v0.0.0-preflight-windows-x86_64
LANE=/mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/g4/lane-c-windows-pe
WORK=$HOME/lane-c-pe
OUT=$WORK/out
LOGS=$WORK/logs
mkdir -p "$OUT" "$LOGS"
cd "$WORK" || exit 90

exec > >(tee "$LOGS/build.log") 2>&1

run() {
  local tag="$1"; shift
  echo "### CMD[$tag]: $*"
  "$@"
  local rc=$?
  echo "### EXIT[$tag]=$rc"
  return $rc
}

echo "=== toolchain identity ==="
run tc1 x86_64-w64-mingw32-gcc --version
run tc2 x86_64-w64-mingw32-gcc -dumpmachine
run tc3 x86_64-w64-mingw32-ar --version
run tc4 x86_64-w64-mingw32-objdump --version
run tc5 uname -a
run tc6 git -C /mnt/c/Users/fred1/source/qianqian rev-parse HEAD

echo "=== package inputs (the exact files this consumer builds against) ==="
run in1 sha256sum "$PKG/include/songcore.h" "$PKG/lib/libsongcore.a"
echo "--- package contains no shared form (windows ships static-only) ---"
ls -la "$PKG/lib"
find "$PKG" -iname '*dll*' -o -iname '*.so*' -o -iname '*shared*' | sort

echo "=== consumer source identity ==="
run src1 sha256sum "$LANE/src/pe-consumer.c"

echo "=== contract section 9 archive audit: undefined BCrypt imports ==="
x86_64-w64-mingw32-nm -A -u "$PKG/lib/libsongcore.a" 2>/dev/null | grep -i bcrypt
echo "### EXIT[nm-bcrypt]=$?"

echo
echo "=== NEGATIVE CONTROL: link WITHOUT -lbcrypt (expected to FAIL) ==="
x86_64-w64-mingw32-gcc -O2 -I"$PKG/include" "$LANE/src/pe-consumer.c" \
  -L"$PKG/lib" -lsongcore -o "$OUT/pe-consumer-nobcrypt.exe" 2> "$LOGS/link-wo-bcrypt.log"
echo "### EXIT[link-wo-bcrypt]=$?"
echo "--- stderr (log: $LOGS/link-wo-bcrypt.log) ---"
cat "$LOGS/link-wo-bcrypt.log"

echo
echo "=== LINK: contract section 9 windows static link line (with -lbcrypt) ==="
echo "### CMD[link]: x86_64-w64-mingw32-gcc -O2 -I<Pkg>/include pe-consumer.c -L<Pkg>/lib -lsongcore -lbcrypt -o $OUT/pe-consumer.exe"
x86_64-w64-mingw32-gcc -O2 -I"$PKG/include" "$LANE/src/pe-consumer.c" \
  -L"$PKG/lib" -lsongcore -lbcrypt -o "$OUT/pe-consumer.exe" 2> "$LOGS/link-with-bcrypt.log"
LINK_RC=$?
echo "### EXIT[link-with-bcrypt]=$LINK_RC"
cat "$LOGS/link-with-bcrypt.log"

if [ "$LINK_RC" -ne 0 ]; then
  echo "BUILD_RESULT=FAIL"
  exit "$LINK_RC"
fi

echo
echo "=== artifact identity ==="
run exe_sha sha256sum "$OUT/pe-consumer.exe"
run exe_file file "$OUT/pe-consumer.exe"
run exe_pefmt x86_64-w64-mingw32-objdump -f "$OUT/pe-consumer.exe"

echo
echo "=== consumer object: proves this source has NO static BCrypt reference ==="
x86_64-w64-mingw32-gcc -O2 -I"$PKG/include" -c "$LANE/src/pe-consumer.c" -o "$OUT/pe-consumer.o"
echo "### EXIT[compile-obj]=$?"
echo "--- nm -u pe-consumer.o | grep -i bcrypt (empty means none) ---"
x86_64-w64-mingw32-nm -u "$OUT/pe-consumer.o" | grep -i bcrypt
echo "### EXIT[obj-bcrypt-grep]=$? (1 = no matches = no consumer-side BCrypt import)"

echo
echo "=== PE import table: DLL names ==="
x86_64-w64-mingw32-objdump -p "$OUT/pe-consumer.exe" | grep -i "DLL Name" | sort -u
echo
echo "=== PE import table: bcrypt.dll entries (the contract section 9 dependency) ==="
x86_64-w64-mingw32-objdump -p "$OUT/pe-consumer.exe" | grep -A16 -i "DLL Name: bcrypt.dll"

echo
echo "BUILD_RESULT=PASS"
