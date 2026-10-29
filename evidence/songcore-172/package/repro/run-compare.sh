#!/usr/bin/env bash
# s2-package repro receipt: contract section 12 T1-T3 measured comparison of
# the two roots produced by ops/build-rc.ps1 -BuildDir (run 20261029-062848).
# Binary-safe member extraction per the frozen method note (ar x, not ar p).
set -euo pipefail
WT=/mnt/c/Users/fred1/source/qianqian-172
OUT=$WT/evidence/songcore-172/package/repro
mkdir -p "$OUT" /home/hoo/s2pkg-repro/r1 /home/hoo/s2pkg-repro/r2
R1=$WT/native/build/artifacts/libsongcore.a
R2=$WT/evidence/songcore-172/build/rc/20261029-062848/r2-artifacts/libsongcore.a
cd /home/hoo/s2pkg-repro

{
  echo "R1=$R1"; sha256sum "$R1"; stat -c "R1_bytes=%s" "$R1"
  echo "R2=$R2"; sha256sum "$R2"; stat -c "R2_bytes=%s" "$R2"
} > "$OUT/identity.txt"

AR=x86_64-w64-mingw32-ar
rm -f r1/* r2/*
( cd r1 && "$AR" x "$R1" )
( cd r2 && "$AR" x "$R2" )

"$AR" t "$R1" > "$OUT/r1-members.txt"
"$AR" t "$R2" > "$OUT/r2-members.txt"
if diff -u "$OUT/r1-members.txt" "$OUT/r2-members.txt" > "$OUT/member-order.diff"; then
  echo "MEMBER_ORDER=IDENTICAL ($(wc -l < "$OUT/r1-members.txt") members)" > "$OUT/member-order.verdict"
else
  echo "MEMBER_ORDER=DIFFERS" > "$OUT/member-order.verdict"
fi

"$AR" tv "$R1" | head -3 > "$OUT/ar-header-r1-sample.txt"
"$AR" tv "$R2" | head -3 > "$OUT/ar-header-r2-sample.txt"

( cd r1 && sha256sum * | sort -k2 ) > "$OUT/r1-member-shas.txt"
( cd r2 && sha256sum * | sort -k2 ) > "$OUT/r2-member-shas.txt"

paste -d" " <(sort -k2 "$OUT/r1-member-shas.txt") <(sort -k2 "$OUT/r2-member-shas.txt") \
  | awk '{ if ($1 != $3) print $2, $1, $3 }' > "$OUT/differing-members.txt"
SAME=$(paste -d" " <(sort -k2 "$OUT/r1-member-shas.txt") <(sort -k2 "$OUT/r2-member-shas.txt") \
  | awk '{ if ($1 == $3) n++ } END { print n+0 }')
DIFF=$(wc -l < "$OUT/differing-members.txt")
echo "MEMBERS_TOTAL=$(wc -l < "$OUT/r1-members.txt") IDENTICAL=$SAME DIFFERING=$DIFF" > "$OUT/member-diff.verdict"

: > "$OUT/rootcause-strings.txt"
while read -r m s1 s2; do
  h1=$(grep -aoc "/home/hoo/q172rc/" "r1/$m" || true)
  h2=$(grep -aoc "/home/hoo/q172rc-s2pkg/" "r2/$m" || true)
  echo "$m r1root_hits=$h1 r2root_hits=$h2" >> "$OUT/rootcause-strings.txt"
done < "$OUT/differing-members.txt"

{
  echo "== archive-level absolute-path scan (grep -aoc counts) =="
  echo "R1 /home/hoo/q172rc/       : $(grep -aoc "/home/hoo/q172rc/" "$R1" || true)"
  echo "R1 /home/hoo/q172rc-s2pkg/ : $(grep -aoc "/home/hoo/q172rc-s2pkg/" "$R1" || true)"
  echo "R1 /home/hoo/Source/ (old) : $(grep -aoc "/home/hoo/Source/" "$R1" || true)"
  echo "R2 /home/hoo/q172rc/       : $(grep -aoc "/home/hoo/q172rc/" "$R2" || true)"
  echo "R2 /home/hoo/q172rc-s2pkg/ : $(grep -aoc "/home/hoo/q172rc-s2pkg/" "$R2" || true)"
  echo "R2 /home/hoo/Source/ (old) : $(grep -aoc "/home/hoo/Source/" "$R2" || true)"
} > "$OUT/archive-path-scan.txt"

M1=$(awk '{print $1}' "$OUT/differing-members.txt" | head -1)
if [ -n "$M1" ]; then
  {
    echo "sample differing member: $M1"
    echo "-- differing byte count (cmp -l): $(cmp -l "r1/$M1" "r2/$M1" | wc -l)"
    echo "-- size: r1=$(stat -c %s "r1/$M1") r2=$(stat -c %s "r2/$M1")"
    echo "-- /home/hoo path strings in r1 copy:"
    strings -a "r1/$M1" | grep "/home/hoo/" | sort -u | head -8
    echo "-- /home/hoo path strings in r2 copy:"
    strings -a "r2/$M1" | grep "/home/hoo/" | sort -u | head -8
    echo "-- r1 sections (.text/.rdata/.debug*):"
    x86_64-w64-mingw32-objdump -h "r1/$M1" | awk '$2 ~ /^\.(text|rdata|debug)/ {print $2, $3}'
    echo "-- r2 sections (.text/.rdata/.debug*):"
    x86_64-w64-mingw32-objdump -h "r2/$M1" | awk '$2 ~ /^\.(text|rdata|debug)/ {print $2, $3}'
  } > "$OUT/debug-info-sample.txt"
fi

if ls r1/songcore_ffmpeg.c.obj >/dev/null 2>&1; then
  s1=$(sha256sum r1/songcore_ffmpeg.c.obj | awk '{print $1}')
  s2=$(sha256sum r2/songcore_ffmpeg.c.obj | awk '{print $1}')
  {
    echo "songcore_ffmpeg.c.obj r1=$s1"
    echo "songcore_ffmpeg.c.obj r2=$s2"
    if [ "$s1" = "$s2" ]; then echo "SONGCORE_C_OBJECT=BYTE_IDENTICAL"; else echo "SONGCORE_C_OBJECT=DIFFERS"; fi
  } > "$OUT/songcore-c-object.txt"
fi

{
  echo "== undefined import audit (runtime-dependency surface, contract S9) =="
  echo "-- R1 BCrypt*:"; x86_64-w64-mingw32-nm "$R1" | grep " U BCrypt" | sort -u || true
  echo "-- R2 BCrypt*:"; x86_64-w64-mingw32-nm "$R2" | grep " U BCrypt" | sort -u || true
  x86_64-w64-mingw32-nm "$R1" | grep " U " | sort -u > "$OUT/r1-undefined-syms.txt"
  x86_64-w64-mingw32-nm "$R2" | grep " U " | sort -u > "$OUT/r2-undefined-syms.txt"
  if diff -q "$OUT/r1-undefined-syms.txt" "$OUT/r2-undefined-syms.txt" >/dev/null; then
    echo "UNDEFINED_SET=IDENTICAL ($(wc -l < "$OUT/r1-undefined-syms.txt") symbols)"
  else
    echo "UNDEFINED_SET=DIFFERS"
    diff "$OUT/r1-undefined-syms.txt" "$OUT/r2-undefined-syms.txt" | head -10
  fi
} > "$OUT/undefined-imports.txt"

cm1=$(sha256sum /home/hoo/q172rc/win/native/build/ffmpeg-xmake/manifest.json | awk '{print $1}')
cm2=$(sha256sum /home/hoo/q172rc-s2pkg/win/native/build/ffmpeg-xmake/manifest.json | awk '{print $1}')
{
  echo "closure manifest r1=$cm1"
  echo "closure manifest r2=$cm2"
  if [ "$cm1" = "$cm2" ]; then echo "CLOSURE_IDENTITY=BYTE_IDENTICAL"; else echo "CLOSURE_IDENTITY=DIFFERS"; fi
} > "$OUT/closure-identity.txt"

echo "---- summary ----"
cat "$OUT/identity.txt" "$OUT/member-order.verdict" "$OUT/member-diff.verdict" \
    "$OUT/archive-path-scan.txt" "$OUT/songcore-c-object.txt" "$OUT/closure-identity.txt"
echo "REPRO_DONE"
