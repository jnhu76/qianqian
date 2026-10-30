#!/usr/bin/env bash
# g4-s3-verify.sh — G4 linux-x86_64 lane, stage 3: measure the artifacts.
#
# Measures, never trusts:
#   - absolute path / byte size / sha256 of libsongcore.a and libsongcore.so
#   - per-member format census of the merged archive (objdump -f)
#   - exported song* set vs the SONGCORE_API set of native/include/songcore.h
#     for BOTH forms (diff empty)
#   - readelf -d NEEDED of the shared form (no libav*/libsw*/libpostproc)
#   - compile_commands.json witness that -fmacro-prefix-map reached the
#     replayed FFmpeg units
set -Eeuo pipefail
export PATH="$HOME/.local/bin:$PATH"
export LC_ALL=C

ROOT=${G4_ROOT:-$HOME/g4-172/linux-root}
SRC=$ROOT/src
EV=$ROOT/receipts/g4-linux-build
NA=$SRC/native
V=$EV/verify
mkdir -p "$V"
INV=$EV/invocations.tsv
touch "$INV"
rec() { local st=$1 rc=$2; shift 2
  { printf '%s\t%s\t' "$st" "$rc"; printf '%q ' "$@"; printf '\n'; } >> "$INV"; }

cd "$NA"
ART_A=$NA/build/artifacts/libsongcore.a
ART_SO=$NA/build/artifacts/shared/libsongcore.so
[ -f "$ART_A" ]  || { echo "FATAL: $ART_A missing" >&2; exit 1; }
[ -f "$ART_SO" ] || { echo "FATAL: $ART_SO missing" >&2; exit 1; }

FAIL=0
nfail() { echo "VERIFY FAIL: $*" >&2; FAIL=$((FAIL+1)); }

# ---------------------------------------------------------------- identity
file "$ART_A" "$ART_SO" > "$V/file-identity.txt"
readelf -h "$ART_SO" > "$V/readelf-h-so.txt"
readelf -d "$ART_SO" > "$V/readelf-d-so.txt"
readelf -h "$ART_A" >/dev/null 2>&1 || true   # archive: not an ELF, expected

grep -q "current ar archive" "$V/file-identity.txt" || nfail "static is not an ar archive"
grep -q "ELF 64-bit LSB shared object, x86-64" "$V/file-identity.txt" || nfail "shared is not ELF64 x86-64 shared"
grep -q "Machine:.*X86-64" "$V/readelf-h-so.txt" || nfail "shared machine is not X86-64"

# ---------------------------------------------------------------- sizes + hashes
stat -c '%s' "$ART_A"  > "$V/static-bytes.txt"
stat -c '%s' "$ART_SO" > "$V/shared-bytes.txt"
sha256sum "$ART_A" "$ART_SO" > "$V/artifacts-sha256.txt"
STATIC_BYTES=$(cat "$V/static-bytes.txt")
SHARED_BYTES=$(cat "$V/shared-bytes.txt")
STATIC_SHA=$(awk -v p="$ART_A" '$2==p{print $1}' "$V/artifacts-sha256.txt")
SHARED_SHA=$(awk -v p="$ART_SO" '$2==p{print $1}' "$V/artifacts-sha256.txt")

# ---------------------------------------------------------------- member census
ar t "$ART_A" > "$V/ar-members.txt"
MEMBERS=$(wc -l < "$V/ar-members.txt")
objdump -f "$ART_A" > "$V/objdump-f-a.txt"
TOTAL_FMT=$(grep -c "file format" "$V/objdump-f-a.txt" || true)
ELF64_FMT=$(grep -c "file format elf64-x86-64" "$V/objdump-f-a.txt" || true)
grep "file format" "$V/objdump-f-a.txt" | grep -v "file format elf64-x86-64" > "$V/objdump-f-foreign.txt" || true
FOREIGN=$(wc -l < "$V/objdump-f-foreign.txt")
[ "$MEMBERS" -gt 0 ] || nfail "empty archive"
[ "$MEMBERS" = "$TOTAL_FMT" ] || nfail "objdump -f reported $TOTAL_FMT members but ar t has $MEMBERS"
[ "$MEMBERS" = "$ELF64_FMT" ] || nfail "member formats: $ELF64_FMT/$MEMBERS are elf64-x86-64"
[ "$FOREIGN" -eq 0 ] || nfail "foreign member formats: $FOREIGN"

# duplicate member names, as a census detail (never collapsed)
sort "$V/ar-members.txt" | uniq -d > "$V/ar-member-duplicates.txt" || true
DUP_NAMES=$(wc -l < "$V/ar-member-duplicates.txt")

# ---------------------------------------------------------------- ABI export set
grep 'SONGCORE_API' include/songcore.h | grep -oE '\bsong[a-z0-9_]*\(' | tr -d '(' | sort -u \
  > "$V/abi-expected.txt"
nm -D --defined-only "$ART_SO" | awk '$2=="T"||$2=="W"{print $3}' | grep '^song' | sort -u \
  > "$V/abi-exported-so.txt"
nm "$ART_A" | awk '$2=="T"||$2=="A"{print $3}' | grep '^song' | sort -u \
  > "$V/abi-exported-a.txt"
EXPECTED_N=$(wc -l < "$V/abi-expected.txt")
SO_N=$(wc -l < "$V/abi-exported-so.txt")
A_N=$(wc -l < "$V/abi-exported-a.txt")
diff -u "$V/abi-expected.txt" "$V/abi-exported-so.txt" > "$V/abi-diff-so.txt" || nfail ".so song* set differs from songcore.h"
diff -u "$V/abi-expected.txt" "$V/abi-exported-a.txt"  > "$V/abi-diff-a.txt"  || nfail ".a song* set differs from songcore.h"
[ -s "$V/abi-diff-so.txt" ] && nfail ".so abi diff not empty"
[ -s "$V/abi-diff-a.txt" ]  && nfail ".a abi diff not empty"
[ "$EXPECTED_N" -eq 15 ] || nfail "songcore.h SONGCORE_API entry count is $EXPECTED_N, expected 15"

# ---------------------------------------------------------------- NEEDED audit
sed -n 's/.*(NEEDED).*\[\(.*\)\]/\1/p' "$V/readelf-d-so.txt" > "$V/needed-libs.txt"
grep -E '^(libav|libsw|libpostproc)' "$V/needed-libs.txt" > "$V/needed-forbidden.txt" || true
FORBIDDEN=$(wc -l < "$V/needed-forbidden.txt")
[ "$FORBIDDEN" -eq 0 ] || nfail "shared NEEDs forbidden FFmpeg libraries: $(tr '\n' ' ' < "$V/needed-forbidden.txt")"
NEEDED_LIST=$(tr '\n' ' ' < "$V/needed-libs.txt" | sed 's/ $//')

# ---------------------------------------------------------------- flag witness
CC=$EV/compile-commands/compile_commands.json
if [ -f "$CC" ]; then
  python3 - "$CC" "$V/compile-commands-witness.txt" <<'PY'
import json, sys
entries = json.load(open(sys.argv[1]))
macro = [e for e in entries if "-fmacro-prefix-map" in e.get("command", " ".join(e.get("arguments", [])))]
filemap = [e for e in entries if "-ffile-prefix-map" in e.get("command", " ".join(e.get("arguments", [])))]
ffsrc = [e for e in entries if "ffmpeg-src" in e.get("command", " ".join(e.get("arguments", [])))]
sample = ""
if macro:
    args = macro[0].get("arguments") or macro[0].get("command", "").split()
    sample = " ".join(args)
lines = [
    f"compile_commands_entries={len(entries)}",
    f"fmacro_prefix_map_entries={len(macro)}",
    f"ffile_prefix_map_entries={len(filemap)}",
    f"ffmpeg_src_entries={len(ffsrc)}",
    "sample_flag_entry=" + sample[:500],
]
open(sys.argv[2], "w").write("\n".join(lines) + "\n")
print("\n".join(lines))
PY
else
  echo "compile_commands.json missing" > "$V/compile-commands-witness.txt"
  nfail "compile_commands.json missing (flag delivery witness not measured)"
fi
CM_ENTRIES=$(sed -n 's/^compile_commands_entries=//p' "$V/compile-commands-witness.txt")
CM_MACRO=$(sed -n 's/^fmacro_prefix_map_entries=//p' "$V/compile-commands-witness.txt")
[ "${CM_MACRO:-0}" -eq 279 ] || nfail "macro-prefix-map entries ${CM_MACRO:-0} != 279"

# ---------------------------------------------------------------- facts
{
  echo "static_path=$ART_A"
  echo "static_bytes=$STATIC_BYTES"
  echo "static_sha256=$STATIC_SHA"
  echo "shared_path=$ART_SO"
  echo "shared_bytes=$SHARED_BYTES"
  echo "shared_sha256=$SHARED_SHA"
  echo "members_total=$MEMBERS"
  echo "members_elf64=$ELF64_FMT"
  echo "members_foreign=$FOREIGN"
  echo "member_duplicate_names=$DUP_NAMES"
  echo "abi_expected_count=$EXPECTED_N"
  echo "abi_so_count=$SO_N"
  echo "abi_a_count=$A_N"
  echo "abi_diff_so_bytes=$(stat -c '%s' "$V/abi-diff-so.txt")"
  echo "abi_diff_a_bytes=$(stat -c '%s' "$V/abi-diff-a.txt")"
  echo "needed=$NEEDED_LIST"
  echo "needed_forbidden_count=$FORBIDDEN"
  echo "compile_commands_entries=$CM_ENTRIES"
  echo "compile_commands_macro_map=$CM_MACRO"
  echo "verify_failures=$FAIL"
} > "$V/verify-facts.env"

if [ "$FAIL" -ne 0 ]; then
  echo "VERIFY: FAIL ($FAIL checks)" >&2
  exit 1
fi
rec verify-measurements 0 bash g4-s3-verify.sh
echo "VERIFY: PASS"
cat "$V/verify-facts.env"
