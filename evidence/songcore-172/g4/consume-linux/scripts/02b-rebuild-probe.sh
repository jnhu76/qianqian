#!/usr/bin/env bash
# Lane C (linux) - stage 02b: rebuild static consumers after the consumer-source fix,
# build the seek diagnostic probe, then characterize the near-end seek.
set -u
PKG=/home/hoo/g4-172/laneB-stage/linux/songcore-v0.0.0-preflight-linux-x86_64
W=/home/hoo/g4-172/consume-linux
EVID=/mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/g4/consume-linux
LOG=$EVID/logs/stage02b-rebuild.log
mkdir -p "$EVID/logs"
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

cp "$EVID/src/consume_corpus.c" "$W/src/consume_corpus.c"
cp "$EVID/src/seek_probe.c"     "$W/src/seek_probe.c"

echo "=== source sha256 (post-fix) ==="
sha256sum "$W"/src/*.c | tee -a "$LOG"

run "$LOG" gcc -O2 -I"$PKG/include" "$W/src/consume_corpus.c" \
    "$PKG/lib/libsongcore.a" -lm -lpthread -o "$W/bin/consume-corpus-static"
run "$LOG" gcc -O2 -I"$PKG/include" "$W/src/consume_corpus.c" \
    -L"$PKG/lib/shared" -lsongcore -lm -lpthread -o "$W/bin/consume-corpus-shared"
run "$LOG" gcc -O2 -I"$PKG/include" "$W/src/consume_corpus.c" \
    -L"$PKG/lib/shared" -lsongcore -lm -lpthread \
    -Wl,-rpath,"$PKG/lib/shared" -o "$W/bin/consume-corpus-shared-rpath"
run "$LOG" gcc -O2 -I"$PKG/include" "$W/src/seek_probe.c" \
    "$PKG/lib/libsongcore.a" -lm -lpthread -o "$W/bin/seek-probe-static"

echo
echo "=== artifact sha256 ==="
sha256sum "$W"/bin/* | tee -a "$LOG"

echo
echo "=== seek characterization: FLAC, fresh handle per target ==="
run "$LOG" "$W/bin/seek-probe-static" fresh "$W/fixtures/flac-16-44-stereo.flac" \
    1000000 2000000 3000000 3500000 3700000 3800000 3900000 3950000 3990000 4000000
echo
echo "=== seek characterization: FLAC, one reused handle (as the corpus consumer does) ==="
run "$LOG" "$W/bin/seek-probe-static" same "$W/fixtures/flac-16-44-stereo.flac" \
    1000000 2000000 3000000 3800000
echo
echo "=== seek characterization: MP3 fresh, near end ==="
run "$LOG" "$W/bin/seek-probe-static" fresh "$W/fixtures/mp3-cbr-id3v23.mp3" \
    1000000 2000000 3000000 3800000
echo
echo "=== seek characterization: ALAC fresh, near end ==="
run "$LOG" "$W/bin/seek-probe-static" fresh "$W/fixtures/alac-16-44-stereo.m4a" \
    1000000 2000000 3000000 3800000

echo
echo "----- captured seek probe output -----"
sed -n '/seek-probe-static/,$p' "$LOG"
