#!/usr/bin/env bash
# D3 probe build+run. Read-only w.r.t. sources: builds the shared library from
# the pinned tree (native/build is gitignored) and runs a C consumer that poisons
# *out_frames_produced before each call.
# NOTE: xmake must run from native/ (that is where xmake.lua lives).
set -u
REPO=/home/hoo/Source/qianqian
OUT=/home/hoo/d3probe
MDIR=/mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/g4/d3-readpcm-zero
FIXTURE=/mnt/c/Users/fred1/source/qianqian/native/experiments/songcore-equivalence/fixtures/flac-16-44-stereo.flac
mkdir -p "$OUT"

echo "=== STEP 1 source identity ==="
cd "$REPO" || exit 90
git rev-parse HEAD; echo "rev_exit=$?"
sha256sum native/src/songcore_ffmpeg.c native/include/songcore.h
echo "git_status:"; git status --porcelain | head

echo "=== STEP 2 build (from native/) ==="
export PATH=$HOME/.local/bin:$PATH
cd "$REPO/native" || exit 91
command -v xmake; echo "xmake_present_exit=$?"
xmake f -p linux -a x86_64 -m release -y < /dev/null > "$OUT/configure.log" 2>&1
echo "configure_exit=$?"
xmake build songcore < /dev/null > "$OUT/build.log" 2>&1
echo "build_exit=$?"
tail -n 4 "$OUT/build.log"
tail -n 6 "$OUT/configure.log"

echo "=== STEP 3 artifact ==="
LIB=$(find "$REPO/native/build/artifacts" -name 'libsongcore.so' | head -n1)
echo "LIB=$LIB"
ls -l "$LIB" 2>/dev/null; echo "ls_lib_exit=$?"
sha256sum "$LIB" 2>/dev/null; echo "sha_lib_exit=$?"
mkdir -p "$OUT/lib"; cp -f "$LIB" "$OUT/lib/" 2>/dev/null; echo "cp_lib_exit=$?"

echo "=== STEP 4 compile consumer ==="
gcc -std=c11 -O1 -Wall -Wextra -I "$REPO/native/include" \
    "$MDIR/readpcm_zero_probe.c" \
    -L "$OUT/lib" -lsongcore -Wl,-rpath,"$OUT/lib" \
    -o "$OUT/readpcm_zero_probe" > "$OUT/cc.log" 2>&1
echo "cc_exit=$?"
cat "$OUT/cc.log"

echo "=== STEP 5 run consumer ==="
"$OUT/readpcm_zero_probe" "$FIXTURE" > "$OUT/run.log" 2>&1
echo "run_exit=$?"
cat "$OUT/run.log"

echo "=== STEP 6 guard order in the emitted binary ==="
objdump -d --no-show-raw-insn --disassemble=song_read_pcm "$OUT/lib/libsongcore.so" > "$OUT/song_read_pcm.dis" 2>&1
echo "objdump_exit=$?"
wc -l "$OUT/song_read_pcm.dis"
head -n 45 "$OUT/song_read_pcm.dis"
