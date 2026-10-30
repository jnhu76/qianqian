#!/usr/bin/env bash
# D3 probe, clean-rebuild pass: proves the shared library used for the probe is
# produced by a build of THIS tree at the pinned SHA (artifacts removed first).
set -u
REPO=/home/hoo/Source/qianqian
OUT=/home/hoo/d3probe
MDIR=/mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/g4/d3-readpcm-zero
FIXTURE=/mnt/c/Users/fred1/source/qianqian/native/experiments/songcore-equivalence/fixtures/flac-16-44-stereo.flac
export PATH=$HOME/.local/bin:$PATH

echo "=== STEP 0 identity (before) ==="
cd "$REPO" || exit 90
git rev-parse HEAD; echo "rev_exit=$?"
sha256sum native/src/songcore_ffmpeg.c native/include/songcore.h
find "$REPO/native/build/artifacts" -name 'libsongcore*' -printf '%Ts %s %p\n' 2>/dev/null

echo "=== STEP 1 remove only SongCore artifacts (closure untouched) ==="
rm -rf "$REPO/native/build/artifacts"
echo "rm_exit=$?"
ls "$REPO/native/build/artifacts" 2>/dev/null || echo "artifacts_absent"

echo "=== STEP 2 clean build from native/ ==="
cd "$REPO/native" || exit 91
xmake f -p linux -a x86_64 -m release -y < /dev/null > "$OUT/configure-clean.log" 2>&1
echo "configure_exit=$?"
( time xmake build songcore < /dev/null ) > "$OUT/build-clean.log" 2>&1
echo "build_exit=$?"
grep -E "build ok|error" "$OUT/build-clean.log" | tail -n 3
tail -n 3 "$OUT/build-clean.log"

echo "=== STEP 3 artifact identity (after) ==="
LIB="$REPO/native/build/artifacts/shared/libsongcore.so"
ls -l --time-style=+%s "$LIB"; echo "ls_lib_exit=$?"
sha256sum "$LIB"
mkdir -p "$OUT/lib"; cp -f "$LIB" "$OUT/lib/"; echo "cp_exit=$?"

echo "=== STEP 4 recompile consumer against the fresh artifact ==="
gcc -std=c11 -O1 -Wall -Wextra -I "$REPO/native/include" \
    "$MDIR/readpcm_zero_probe.c" \
    -L "$OUT/lib" -lsongcore -Wl,-rpath,"$OUT/lib" \
    -o "$OUT/readpcm_zero_probe" > "$OUT/cc-clean.log" 2>&1
echo "cc_exit=$?"
cat "$OUT/cc-clean.log"

echo "=== STEP 5 run ==="
"$OUT/readpcm_zero_probe" "$FIXTURE" > "$OUT/run-clean.log" 2>&1
echo "run_exit=$?"
cat "$OUT/run-clean.log"
sha256sum "$MDIR/readpcm_zero_probe.c"

echo "=== STEP 6 guard order in the emitted binary ==="
objdump -d --no-show-raw-insn --disassemble=song_read_pcm "$OUT/lib/libsongcore.so" > "$OUT/song_read_pcm-clean.dis" 2>&1
echo "objdump_exit=$?"
awk '/<song_read_pcm/{f=1} f' "$OUT/song_read_pcm-clean.dis" | sed -n '1,30p'
echo "--- early-return blocks ---"
awk '/<song_read_pcm/{f=1} f' "$OUT/song_read_pcm-clean.dis" | grep -E "^\s+(35a0[0-9a-f]|35a[1-3][0-9a-f]|35b3[0-9a-f]|35b6[0-9a-f]|35b7[0-9a-f]):" | head -30
sha256sum "$LIB"
