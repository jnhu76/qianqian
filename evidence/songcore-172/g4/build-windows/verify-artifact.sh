#!/usr/bin/env bash
# g4/Windows build lane: full artifact verification battery.
# Reads the artifact produced by ops/build-rc.ps1 in the WSL work root and the
# copied-back artifact in the Windows worktree; writes a log the caller keeps.
set -uo pipefail
WSL_ART=/home/hoo/q172rc/win/native/build/artifacts/libsongcore.a
WT=/mnt/c/Users/fred1/source/qianqian
WIN_ART=$WT/native/build/artifacts/libsongcore.a
LANE=$WT/evidence/songcore-172/g4/build-windows
OBJDUMP=x86_64-w64-mingw32-objdump
NM=x86_64-w64-mingw32-nm
AR=x86_64-w64-mingw32-ar

echo "=== [1] WSL-side sha256/size"
sha256sum "$WSL_ART"
stat -c 'size=%s' "$WSL_ART"

echo
echo "=== [2] copy WSL artifact into /mnt/c lane dir for a local Windows re-hash"
cp -f "$WSL_ART" "$LANE/libsongcore.a.wsl-copy"
sha256sum "$LANE/libsongcore.a.wsl-copy"

echo
echo "=== [3] member census (objdump -f, per member)"
MEMBERS=$($AR t "$WSL_ART" | wc -l)
$OBJDUMP -f "$WSL_ART" > "$LANE/objdump-f.txt" 2>&1
FMT_TOTAL=$(grep -c '^file format' "$LANE/objdump-f.txt" || true)
# objdump prints "<member>: file format <fmt>"; normalize both shapes.
PE=$(grep -cE 'file format pe-x86-64$' "$LANE/objdump-f.txt" || true)
ELF=$(grep -cE 'file format elf' "$LANE/objdump-f.txt" || true)
FOREIGN=$(grep -E 'file format ' "$LANE/objdump-f.txt" | grep -vcE 'file format pe-x86-64$' || true)
echo "ar_members=$MEMBERS"
echo "objdump_format_lines=$FMT_TOTAL"
echo "pe_x86_64=$PE"
echo "elf=$ELF"
echo "foreign_non_pe=$FOREIGN"
if [ "$FOREIGN" -gt 0 ]; then
  echo "-- foreign format lines:"
  grep -E 'file format ' "$LANE/objdump-f.txt" | grep -vE 'file format pe-x86-64$' | head -10
fi

echo
echo "=== [4] 15/15 ABI entry points defined in the archive (nm T)"
EXPECTED="songcore_abi_version song_last_error song_open song_probe song_audio_stream_count song_audio_stream_info song_select_stream song_get_metadata song_get_metadata_count song_get_metadata_entry song_get_artwork_count song_get_artwork_item song_read_pcm song_seek song_close"
$NM "$WSL_ART" > "$LANE/nm.txt" 2>&1
# defined text symbols, normalized (PE nm may print 'T name' or 'name T')
DEFINED=$(awk '$2=="T" {print $3} $1=="T" {print $2}' "$LANE/nm.txt" | sort -u)
MISSING=0
for s in $EXPECTED; do
  if echo "$DEFINED" | grep -qx "$s"; then
    echo "defined: $s"
  else
    echo "MISSING: $s"
    MISSING=$((MISSING+1))
  fi
done
echo "expected_count=15 missing_count=$MISSING"

echo
echo "=== [5] bcrypt contract-9 undefined imports"
for s in BCryptOpenAlgorithmProvider BCryptGenRandom BCryptCloseAlgorithmProvider; do
  if grep -qE "(^| )__imp_${s}( |$)|(^| )${s}( |$)" "$LANE/nm.txt"; then
    echo "PRESENT: $s"
    grep -E "(^| )__imp_${s}( |$)|(^| )${s}( |$)" "$LANE/nm.txt" | head -3
  else
    echo "ABSENT: $s"
  fi
done
echo "-- all BCrypt* nm lines:"
grep -oE "BCrypt[A-Za-z0-9_]*" "$LANE/nm.txt" | sort -u

echo
echo "=== [6] closed closure manifest (Windows worktree consumption path)"
sha256sum "$WT/native/build/ffmpeg-xmake/manifest.json"
stat -c 'size=%s' "$WT/native/build/ffmpeg-xmake/manifest.json"
cmp -s "$WT/native/build/ffmpeg-xmake/manifest.json" /home/hoo/q172rc/win/native/build/ffmpeg-xmake/manifest.json \
  && echo "manifest_identical_to_wsl_root=yes" || echo "manifest_identical_to_wsl_root=NO"

echo
echo "=== [7] Windows copy-back artifact"
sha256sum "$WIN_ART"
stat -c 'size=%s' "$WIN_ART"
