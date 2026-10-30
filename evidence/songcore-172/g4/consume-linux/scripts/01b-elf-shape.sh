#!/usr/bin/env bash
# Lane C (linux) - stage 01b: ELF shape of the packaged artifacts (what we link against).
set -u
T=/home/hoo/g4-172/laneB-stage/linux/songcore-v0.0.0-preflight-linux-x86_64
cd "$T" || exit 1

echo "=== file(1) ==="
file lib/libsongcore.a lib/shared/libsongcore.so
echo
echo "=== readelf -h lib/shared/libsongcore.so (condensed) ==="
readelf -h lib/shared/libsongcore.so | egrep 'Class|Machine|Type|Entry'
echo
echo "=== .so dynamic: SONAME / RPATH / RUNPATH / NEEDED ==="
readelf -d lib/shared/libsongcore.so | egrep 'NEEDED|SONAME|RPATH|RUNPATH'
echo
echo "=== .so exported dynamic symbols in .dynsym (defined, global) ==="
readelf --dyn-syms -W lib/shared/libsongcore.so | awk '$7!="UND" && $5=="GLOBAL" {print $8}' | grep -v '^$' | sort
echo "--- count ---"
readelf --dyn-syms -W lib/shared/libsongcore.so | awk '$7!="UND" && $5=="GLOBAL" {print $8}' | grep -v '^$' | wc -l
echo
echo "=== .so: any av_/ff_/swr_/sws_ exported? (must be 0) ==="
readelf --dyn-syms -W lib/shared/libsongcore.so | awk '$7!="UND" && $5=="GLOBAL" {print $8}' | egrep '^(av|ff|swr|sws)_' | wc -l
echo
echo "=== archive member count + any av_/ff_ exported-ish global T ==="
ar t lib/libsongcore.a | wc -l
echo "--- package symbols.txt ---"
cat metadata/symbols.txt
echo "--- checksums.txt ---"
cat metadata/checksums.txt
