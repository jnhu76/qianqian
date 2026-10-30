#!/usr/bin/env bash
# D3 probe, forced-rebuild pass: `xmake build -r songcore` recompiles the target
# from the pinned sources, so the probed .so cannot be a stale artifact.
set -u
REPO=/home/hoo/Source/qianqian
OUT=/home/hoo/d3probe
MDIR=/mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/g4/d3-readpcm-zero
FIXTURE=/mnt/c/Users/fred1/source/qianqian/native/experiments/songcore-equivalence/fixtures/flac-16-44-stereo.flac
export PATH=$HOME/.local/bin:$PATH

cd "$REPO" || exit 90
git rev-parse HEAD
git diff --exit-code -- native/src native/include; echo "diff_exit=$?"

cd "$REPO/native" || exit 91
( time xmake build -r songcore < /dev/null ) > "$OUT/build-force.log" 2>&1
echo "force_build_exit=$?"
grep -E "build ok|error:" "$OUT/build-force.log" | tail -n 3
grep -c "songcore_ffmpeg.c" "$OUT/build-force.log" || true
tail -n 3 "$OUT/build-force.log"

LIB="$REPO/native/build/artifacts/shared/libsongcore.so"
sha256sum "$LIB"
cp -f "$LIB" "$OUT/lib/"; echo "cp_exit=$?"

gcc -std=c11 -O1 -Wall -Wextra -I "$REPO/native/include" \
    "$MDIR/readpcm_zero_probe.c" -L "$OUT/lib" -lsongcore \
    -Wl,-rpath,"$OUT/lib" -o "$OUT/readpcm_zero_probe" > "$OUT/cc-force.log" 2>&1
echo "cc_exit=$?"

"$OUT/readpcm_zero_probe" "$FIXTURE" > "$OUT/run-force.log" 2>&1
echo "run_exit=$?"
cat "$OUT/run-force.log"

objdump -d --no-show-raw-insn --disassemble=song_read_pcm "$OUT/lib/libsongcore.so" > "$OUT/song_read_pcm-force.dis" 2>&1
echo "objdump_exit=$?"
sha256sum "$OUT/song_read_pcm-force.dis"
