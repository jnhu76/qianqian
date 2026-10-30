#!/usr/bin/env bash
# Lane C (linux) - stage 07: full re-run after the consumer finalization.
#   * rebuild every consumer against the package only
#   * S-BIND 7.1 step 10 (views unchanged across seek) and step 13 (reopen)
#   * S-BIND 7.1 step 12 (malformed / truncated sources)
#   * static + shared corpus runs, comparison
set -u
PKG=/home/hoo/g4-172/laneB-stage/linux/songcore-v0.0.0-preflight-linux-x86_64
W=/home/hoo/g4-172/consume-linux
EVID=/mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/g4/consume-linux
LOG=$EVID/logs/stage07-full-rerun.log
mkdir -p "$EVID/logs" "$W/out/static" "$W/out/shared" "$W/fixtures/bad"
: > "$LOG"

run() {
    local log=$1; shift
    printf '\n$ %s\n' "$*" >> "$log"
    "$@" >> "$log" 2>&1
    local rc=$?
    printf '[exit] %d\n' "$rc" >> "$log"
    echo "EXIT=$rc : $*"
    return $rc
}

echo "=== copy finalized sources into the isolated workdir ==="
cp "$EVID/src/consume_corpus.c"     "$W/src/consume_corpus.c"
cp "$EVID/src/external-consumer-example.c" "$W/src/external-consumer-example.c"
cp "$EVID/src/op_probe.c"           "$W/src/op_probe.c"
cp "$EVID/src/seek_probe.c"         "$W/src/seek_probe.c"
cp "$EVID/src/robustness_probe.c"   "$W/src/robustness_probe.c"
{
  echo "=== stage07 $(date -u +%Y-%m-%dT%H:%M:%SZ) ==="
  echo "PKG=$PKG"
  echo "gcc: $(gcc --version | head -1)"
  echo "host: $(uname -srm), glibc: $(ldd --version | head -1)"
  sha256sum "$W"/src/*.c
  sha256sum "$PKG"/include/songcore.h "$PKG"/lib/libsongcore.a "$PKG"/lib/shared/libsongcore.so
} >> "$LOG"

echo
echo "=== rebuild (package include + package lib only) ==="
run "$LOG" gcc -O2 -I"$PKG/include" "$W/src/external-consumer-example.c" -L"$PKG/lib" -lsongcore -lm -lpthread -o "$W/bin/consumer-example-static"
run "$LOG" gcc -O2 -I"$PKG/include" "$W/src/external-consumer-example.c" -L"$PKG/lib/shared" -lsongcore -lm -lpthread -o "$W/bin/consumer-example-shared"
run "$LOG" gcc -O2 -I"$PKG/include" "$W/src/consume_corpus.c" "$PKG/lib/libsongcore.a" -lm -lpthread -o "$W/bin/consume-corpus-static"
run "$LOG" gcc -O2 -I"$PKG/include" "$W/src/consume_corpus.c" -L"$PKG/lib/shared" -lsongcore -lm -lpthread -o "$W/bin/consume-corpus-shared"
run "$LOG" gcc -O2 -I"$PKG/include" "$W/src/consume_corpus.c" -L"$PKG/lib/shared" -lsongcore -lm -lpthread -Wl,-rpath,"$PKG/lib/shared" -o "$W/bin/consume-corpus-shared-rpath"
run "$LOG" gcc -O2 -I"$PKG/include" "$W/src/op_probe.c" "$PKG/lib/libsongcore.a" -lm -lpthread -o "$W/bin/op-probe-static"
run "$LOG" gcc -O2 -I"$PKG/include" "$W/src/robustness_probe.c" "$PKG/lib/libsongcore.a" -lm -lpthread -o "$W/bin/robustness-probe-static"
run "$LOG" gcc -O2 -I"$PKG/include" "$W/src/robustness_probe.c" -L"$PKG/lib/shared" -lsongcore -lm -lpthread -o "$W/bin/robustness-probe-shared"
{
  echo "--- artifact sha256 ---"
  sha256sum "$W"/bin/*
} >> "$LOG"

echo
echo "=== bad-input fixtures for S-BIND 7.1 step 12 ==="
: > "$W/fixtures/bad/empty.bin"
python3 - "$W/fixtures/bad/garbage.mp3" <<'PY'
import sys
# deterministic non-media bytes: 4096 bytes of a repeating text pattern
open(sys.argv[1], "wb").write(b"this is not an audio container\n" * 148)
PY
head -c 87742 "$W/fixtures/flac-16-44-stereo.flac" > "$W/fixtures/bad/flac-truncated.flac"
head -c 40000 "$W/fixtures/mp3-cbr-id3v23.mp3"  > "$W/fixtures/bad/mp3-truncated.mp3"
{
  echo "--- bad fixtures ---"
  ls -l "$W/fixtures/bad"
  sha256sum "$W"/fixtures/bad/*
} >> "$LOG"

echo
echo "=== step 12: malformed / truncated sources (static) ==="
run "$LOG" "$W/bin/robustness-probe-static" \
    empty       "$W/fixtures/bad/empty.bin" \
    garbage     "$W/fixtures/bad/garbage.mp3" \
    flac-trunc  "$W/fixtures/bad/flac-truncated.flac" \
    mp3-trunc   "$W/fixtures/bad/mp3-truncated.mp3"
echo "=== step 12: malformed / truncated sources (shared) ==="
printf '\n=== [shared] robustness-probe (LD_LIBRARY_PATH=%s) ===\n' "$PKG/lib/shared" >> "$LOG"
LD_LIBRARY_PATH="$PKG/lib/shared" "$W/bin/robustness-probe-shared" \
    empty       "$W/fixtures/bad/empty.bin" \
    garbage     "$W/fixtures/bad/garbage.mp3" \
    flac-trunc  "$W/fixtures/bad/flac-truncated.flac" \
    mp3-trunc   "$W/fixtures/bad/mp3-truncated.mp3" >> "$LOG" 2>&1
printf '[exit] %d\n' $? >> "$LOG"

echo
echo "=== step 10: metadata / artwork / format views across seek (op probe) ==="
run "$LOG" "$W/bin/op-probe-static" "$W/fixtures/flac-16-44-stereo.flac" m s:1000000 r:4096 m s:2000000 r:4096 m s:3000000 r:4096 m d m

echo
echo "=== STATIC corpus runs ==="
declare -A FIX=(
  [mp3-cbr-id3v23]=mp3-cbr-id3v23.mp3
  [flac-16-44-stereo]=flac-16-44-stereo.flac
  [alac-16-44-stereo]=alac-16-44-stereo.m4a
  [alac-long]=alac-long.m4a
)
ORDER="mp3-cbr-id3v23 flac-16-44-stereo alac-16-44-stereo alac-long"
for id in $ORDER; do
  f=$W/fixtures/${FIX[$id]}
  "$W/bin/consumer-example-static" "$f" > "$W/out/static/example-$id.out" 2>&1; rc=$?
  printf '\n$ %s %s  [exit] %d\n' "$W/bin/consumer-example-static" "$f" "$rc" >> "$LOG"
  cat "$W/out/static/example-$id.out" >> "$LOG"
  echo "static/example $id EXIT=$rc"
  "$W/bin/consume-corpus-static" "$f" > "$W/out/static/corpus-$id.jsonl" 2> "$W/out/static/corpus-$id.err"; rc=$?
  printf '\n$ %s %s  [exit] %d\n' "$W/bin/consume-corpus-static" "$f" "$rc" >> "$LOG"
  cat "$W/out/static/corpus-$id.err" >> "$LOG"
  echo "static/corpus  $id EXIT=$rc"
done

echo
echo "=== SHARED corpus runs (LD_LIBRARY_PATH=<package>/lib/shared) ==="
for id in $ORDER; do
  f=$W/fixtures/${FIX[$id]}
  LD_LIBRARY_PATH="$PKG/lib/shared" "$W/bin/consumer-example-shared" "$f" > "$W/out/shared/example-$id.out" 2>&1; rc=$?
  printf '\n$ LD_LIBRARY_PATH=%s %s %s  [exit] %d\n' "$PKG/lib/shared" "$W/bin/consumer-example-shared" "$f" "$rc" >> "$LOG"
  cat "$W/out/shared/example-$id.out" >> "$LOG"
  echo "shared/example $id EXIT=$rc"
  LD_LIBRARY_PATH="$PKG/lib/shared" "$W/bin/consume-corpus-shared" "$f" > "$W/out/shared/corpus-$id.jsonl" 2> "$W/out/shared/corpus-$id.err"; rc=$?
  printf '\n$ LD_LIBRARY_PATH=%s %s %s  [exit] %d\n' "$PKG/lib/shared" "$W/bin/consume-corpus-shared" "$f" "$rc" >> "$LOG"
  cat "$W/out/shared/corpus-$id.err" >> "$LOG"
  echo "shared/corpus  $id EXIT=$rc"
done

echo
echo "=== RPATH build, no LD_LIBRARY_PATH ==="
env -u LD_LIBRARY_PATH "$W/bin/consume-corpus-shared-rpath" "$W/fixtures/flac-16-44-stereo.flac" > "$W/out/shared/rpath-flac.jsonl" 2> "$W/out/shared/rpath-flac.err"; rc=$?
printf '\n$ env -u LD_LIBRARY_PATH %s <flac>  [exit] %d\n' "$W/bin/consume-corpus-shared-rpath" "$rc" >> "$LOG"
echo "shared-rpath/flac EXIT=$rc"

echo
echo "=== comparison against the frozen reference ==="
run "$LOG" python3 "$EVID/scripts/05-compare.py" "$W" "$EVID/out/comparison.json"

echo
echo "----- step 12 / step 10 evidence -----"
sed -n '/robustness-probe-static/,/step 10/p' "$LOG"
grep -E '"op":"(metadata|seek)"' "$LOG" | tail -12
