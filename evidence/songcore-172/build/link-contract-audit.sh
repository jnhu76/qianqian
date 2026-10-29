#!/bin/bash
# songcore-172 D2: system-dependency audit for the link contract.
# Inputs: the GREEN static artifacts (windows mingw + linux host).
# Outputs (in /home/hoo/q172d2/audit/):
#   win-undefined-by-member.txt   per-member undefined symbols (mingw archive)
#   win-external-census.txt       unique undefined symbols (mingw archive)
#   lin-undefined-by-member.txt   per-member undefined symbols (linux archive)
#   lin-external-census.txt       unique undefined symbols (linux archive)
#   win-song-abi.txt              the song* T symbols (ABI surface)
#   link experiments: consumer links with/without -lbcrypt (win) and
#                     with/without -lm -lpthread (linux)
set -uo pipefail
export PATH=/home/hoo/.local/bin:$PATH
A=/home/hoo/q172d2/audit
rm -rf "$A" && mkdir -p "$A"
WIN=/home/hoo/q172d2/win/native/build/artifacts/libsongcore.a
LIN=/home/hoo/q172d2/lin/native/build/artifacts/libsongcore.a
WT=/mnt/c/Users/fred1/source/qianqian-172

echo "=== windows archive census ==="
x86_64-w64-mingw32-nm -A --undefined-only "$WIN" > "$A/win-undefined-by-member.txt" 2>&1
sort -u -k3 "$A/win-undefined-by-member.txt" | awk '{print $3}' | sort -u > "$A/win-external-census.txt"
echo "win archive members: $(x86_64-w64-mingw32-objdump -f "$WIN" 2>/dev/null | grep -c 'file format pe-x86-64')"
echo "win undefined-symbol rows: $(wc -l < "$A/win-undefined-by-member.txt"), unique: $(wc -l < "$A/win-external-census.txt")"
echo "--- members referencing BCrypt*:"
grep "BCrypt" "$A/win-undefined-by-member.txt" | sort -u
echo "--- song* ABI surface:"
x86_64-w64-mingw32-nm "$WIN" 2>/dev/null | grep " T song" | awk '{print $3}' | sort -u > "$A/win-song-abi.txt"
cat "$A/win-song-abi.txt"

echo "=== linux archive census ==="
nm -A --undefined-only "$LIN" > "$A/lin-undefined-by-member.txt" 2>&1
sort -u -k3 "$A/lin-undefined-by-member.txt" | awk '{print $3}' | sort -u > "$A/lin-external-census.txt"
echo "lin archive members: $(objdump -f "$LIN" 2>/dev/null | grep -c 'file format elf64-x86-64')"
echo "lin undefined-symbol rows: $(wc -l < "$A/lin-undefined-by-member.txt"), unique: $(wc -l < "$A/lin-external-census.txt")"

echo "=== link experiments: windows (mingw cross), ABI-complete consumer ==="
mkdir -p "$A/consumer"
cp "$WT/native/include/songcore.h" "$A/consumer/"
{
cat << 'EOF'
#include "songcore.h"
#include <stdio.h>
/* ABI-complete reference probe: taking the address of every song* ABI
 * function forces the linker to pull every defining member of the merged
 * archive, approximating the real consumer's transitive closure. */
extern int main_probe(void);
static void *const song_abi_refs[] = {
EOF
# one line per song* symbol
sed 's/.*/  (void *)\&(&),/' "$A/win-song-abi.txt" | sed 's/(void \*)\&(/(void *)\&/' > /dev/null
while read -r sym; do
  printf '  (void *)&%s,\n' "$sym"
done < "$A/win-song-abi.txt"
cat << 'EOF'
};
int main_probe(void) {
    printf("%p\n", (void *)song_abi_refs[0]);
    return (int)(song_abi_refs[0] != 0);
}
EOF
} > "$A/consumer/consumer.c"
cat "$A/consumer/consumer.c" | head -12

echo "--- win link WITHOUT -lbcrypt (expect failure if BCrypt* reachable):"
x86_64-w64-mingw32-gcc "$A/consumer/consumer.c" -I"$A/consumer" \
  -L/home/hoo/q172d2/win/native/build/artifacts -lsongcore \
  -o "$A/consumer/win-consumer-nobcrypt.exe" > "$A/win-link-nobcrypt.log" 2>&1
echo "WIN_NOBCRYPT_EXIT=$?"
grep -m3 "undefined reference" "$A/win-link-nobcrypt.log" || echo "(no undefined references — linked)"

echo "--- win link WITH -lbcrypt:"
x86_64-w64-mingw32-gcc "$A/consumer/consumer.c" -I"$A/consumer" \
  -L/home/hoo/q172d2/win/native/build/artifacts -lsongcore -lbcrypt \
  -o "$A/consumer/win-consumer-bcrypt.exe" > "$A/win-link-bcrypt.log" 2>&1
echo "WIN_BCRYPT_EXIT=$?"
grep -m3 "undefined reference" "$A/win-link-bcrypt.log" || echo "(no undefined references — linked)"

echo "--- win minimal consumer (abi_version only), no -lbcrypt:"
cat > "$A/consumer/minimal.c" << 'EOF'
#include "songcore.h"
int main(void) { return (int)songcore_abi_version(); }
EOF
x86_64-w64-mingw32-gcc "$A/consumer/minimal.c" -I"$A/consumer" \
  -L/home/hoo/q172d2/win/native/build/artifacts -lsongcore \
  -o "$A/consumer/win-minimal.exe" > "$A/win-link-minimal.log" 2>&1
echo "WIN_MINIMAL_EXIT=$?"
grep -m3 "undefined reference" "$A/win-link-minimal.log" || echo "(no undefined references — linked)"

echo "=== link experiments: linux ==="
echo "--- lin link WITHOUT -lm -lpthread (ABI-complete consumer):"
gcc "$A/consumer/consumer.c" -I"$A/consumer" \
  -L/home/hoo/q172d2/lin/native/build/artifacts -lsongcore \
  -o "$A/consumer/lin-consumer-nolibs" > "$A/lin-link-nolibs.log" 2>&1
echo "LIN_NOLIBS_EXIT=$?"
grep -m5 "undefined reference" "$A/lin-link-nolibs.log" || echo "(no undefined references — linked)"

echo "--- lin link WITH -lm -lpthread:"
gcc "$A/consumer/consumer.c" -I"$A/consumer" \
  -L/home/hoo/q172d2/lin/native/build/artifacts -lsongcore -lm -lpthread \
  -o "$A/consumer/lin-consumer-libs" > "$A/lin-link-libs.log" 2>&1
echo "LIN_LIBS_EXIT=$?"
grep -m3 "undefined reference" "$A/lin-link-libs.log" || echo "(no undefined references — linked)"

echo AUDIT_DONE
