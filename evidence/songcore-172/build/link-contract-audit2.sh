#!/bin/bash
# songcore-172 D2: audit follow-up — fix the consumer harness (add main) and
# rerun the two experiments that failed only on the missing entry point;
# census cross-checks for pthread/win32-API-class symbols.
set -uo pipefail
export PATH=/home/hoo/.local/bin:$PATH
A=/home/hoo/q172d2/audit
WT=/mnt/c/Users/fred1/source/qianqian-172

{
cat << 'EOF'
#include "songcore.h"
#include <stdio.h>
/* ABI-complete reference probe: taking the address of every song* ABI
 * function forces the linker to pull every defining member of the merged
 * archive, approximating the real consumer's transitive closure. */
static void *const song_abi_refs[] = {
EOF
while read -r sym; do
  printf '  (void *)&%s,\n' "$sym"
done < "$A/win-song-abi.txt"
cat << 'EOF'
};
int main(void) {
    printf("%zu\n", sizeof(song_abi_refs) / sizeof(song_abi_refs[0]));
    return song_abi_refs[0] == 0;
}
EOF
} > "$A/consumer/consumer.c"

echo "--- win ABI-complete WITH -lbcrypt (expect clean 0):"
x86_64-w64-mingw32-gcc "$A/consumer/consumer.c" -I"$A/consumer" \
  -L/home/hoo/q172d2/win/native/build/artifacts -lsongcore -lbcrypt \
  -o "$A/consumer/win-consumer-bcrypt.exe" > "$A/win-link-bcrypt.log" 2>&1
echo "WIN_BCRYPT_EXIT=$?"
grep -m3 "undefined reference" "$A/win-link-bcrypt.log" || echo "(no undefined references — linked)"

echo "--- lin ABI-complete WITH -lm -lpthread (expect clean 0):"
gcc "$A/consumer/consumer.c" -I"$A/consumer" \
  -L/home/hoo/q172d2/lin/native/build/artifacts -lsongcore -lm -lpthread \
  -o "$A/consumer/lin-consumer-libs" > "$A/lin-link-libs.log" 2>&1
echo "LIN_LIBS_EXIT=$?"
grep -m3 "undefined reference" "$A/lin-link-libs.log" || echo "(no undefined references — linked)"

echo "--- lin ABI-complete WITHOUT -lm (with -lpthread only; expect math failures):"
gcc "$A/consumer/consumer.c" -I"$A/consumer" \
  -L/home/hoo/q172d2/lin/native/build/artifacts -lsongcore -lpthread \
  -o "$A/consumer/lin-consumer-nomath" > "$A/lin-link-nomath.log" 2>&1
echo "LIN_NOMATH_EXIT=$?"
grep -m3 "undefined reference" "$A/lin-link-nomath.log" || echo "(linked)"

echo "=== census cross-checks ==="
echo "--- pthread-class refs in linux archive:"
grep -E "pthread" "$A/lin-external-census.txt" | head -10
echo "(count: $(grep -cE 'pthread' "$A/lin-external-census.txt"))"
echo "--- libm (math) refs sample in linux archive:"
grep -cE "^(log2f?|expf?|powf?|cosf?|sinf?|atan2?|sqrtf?|floorf?)$" "$A/lin-external-census.txt"
echo "--- win32 API-class refs in windows archive (non-CRT system libs):"
grep -E "^(BCrypt|Crypt|Sleep|WaitFor|Create|Open|Close|Set|Get|Query|Load|Free|Initialize|Delete|Enter|Leave|Release|Tls|Virtual|Local|Global|Heap|Map|Unmap|Read|Write|Device|Cancel|Register|Deregister|time|Time)" "$A/win-external-census.txt" | head -25
echo "--- win CRT/provider check: what provides memset etc. (msvcrt via mingw spec) — sample:"
grep -E "^(memset|memcpy|malloc|free|strcasecmp)" "$A/win-external-census.txt" | head -5
echo AUDIT2_DONE
