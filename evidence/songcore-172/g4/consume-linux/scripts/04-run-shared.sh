#!/usr/bin/env bash
# Lane C (linux) - stage 04: run the SHARED consumers (package lib/shared/libsongcore.so)
# and prove which .so the process actually loads.
set -u

PKG=/home/hoo/g4-172/laneB-stage/linux/songcore-v0.0.0-preflight-linux-x86_64
W=/home/hoo/g4-172/consume-linux
EVID=/mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/g4/consume-linux
LOG=$EVID/logs/stage04-shared-run.log
mkdir -p "$W/out/shared"
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

{
  echo "=== stage04 shared run $(date -u +%Y-%m-%dT%H:%M:%SZ) ==="
  echo "PKG=$PKG"
  echo "the only libsongcore.so under PKG:"
  find "$PKG" -name 'libsongcore.so*' -exec sha256sum {} \;
  echo "any system libsongcore.so? (ldconfig -p | grep songcore):"
  ldconfig -p | grep songcore || echo "(none in the dynamic-linker cache)"
  echo "any OTHER libsongcore.so on the host outside PKG (excluding our isolated dir):"
  find / -xdev -name 'libsongcore.so*' 2>/dev/null | grep -v "^$PKG" | grep -v '^/proc' || true
} >> "$LOG"

echo "=== 1. ldd WITHOUT LD_LIBRARY_PATH (expect libsongcore.so => not found) ==="
printf '\n$ env -u LD_LIBRARY_PATH ldd %s\n' "$W/bin/consume-corpus-shared" >> "$LOG"
env -u LD_LIBRARY_PATH ldd "$W/bin/consume-corpus-shared" >> "$LOG" 2>&1
printf '[exit] %d\n' $? >> "$LOG"

echo "=== 2. ldd WITH LD_LIBRARY_PATH=$PKG/lib/shared (expect the parked absolute path) ==="
printf '\n$ LD_LIBRARY_PATH=%s ldd %s\n' "$PKG/lib/shared" "$W/bin/consume-corpus-shared" >> "$LOG"
LD_LIBRARY_PATH="$PKG/lib/shared" ldd "$W/bin/consume-corpus-shared" >> "$LOG" 2>&1
printf '[exit] %d\n' $? >> "$LOG"

echo "=== 3. ldd of the RPATH build WITHOUT LD_LIBRARY_PATH (expect RUNPATH-resolved package path) ==="
printf '\n$ env -u LD_LIBRARY_PATH ldd %s\n' "$W/bin/consume-corpus-shared-rpath" >> "$LOG"
env -u LD_LIBRARY_PATH ldd "$W/bin/consume-corpus-shared-rpath" >> "$LOG" 2>&1
printf '[exit] %d\n' $? >> "$LOG"

echo "=== 4. runtime loader trace (LD_DEBUG=libs) of one real run ==="
printf '\n$ LD_DEBUG=libs LD_LIBRARY_PATH=%s %s <mp3>\n' "$PKG/lib/shared" "$W/bin/consume-corpus-shared" >> "$LOG"
LD_DEBUG=libs LD_LIBRARY_PATH="$PKG/lib/shared" "$W/bin/consume-corpus-shared" \
  "$W/fixtures/mp3-cbr-id3v23.mp3" > "$W/out/shared/lddebug-mp3.out" 2> "$W/out/shared/lddebug-mp3.err"
printf '[exit] %d\n' $? >> "$LOG"
grep -E 'calling init: .*songcore|trying file=.*songcore' "$W/out/shared/lddebug-mp3.err" >> "$LOG"

OVERALL=0
declare -A FIX=(
  [mp3-cbr-id3v23]=mp3-cbr-id3v23.mp3
  [flac-16-44-stereo]=flac-16-44-stereo.flac
  [alac-16-44-stereo]=alac-16-44-stereo.m4a
  [alac-long]=alac-long.m4a
)
ORDER="mp3-cbr-id3v23 flac-16-44-stereo alac-16-44-stereo alac-long"
for id in $ORDER; do
  f=$W/fixtures/${FIX[$id]}
  printf '\n=== [shared] consumer-example-shared %s ===\n' "$f" >> "$LOG"
  printf '$ LD_LIBRARY_PATH=%s %s %s\n' "$PKG/lib/shared" "$W/bin/consumer-example-shared" "$f" >> "$LOG"
  LD_LIBRARY_PATH="$PKG/lib/shared" "$W/bin/consumer-example-shared" "$f" > "$W/out/shared/example-$id.out" 2>&1
  rc=$?
  printf '[exit] %d\n' "$rc" >> "$LOG"; cat "$W/out/shared/example-$id.out" >> "$LOG"
  echo "RUN shared/example $id EXIT=$rc"; [ $rc -ne 0 ] && OVERALL=1

  printf '\n=== [shared] consume-corpus-shared %s ===\n' "$f" >> "$LOG"
  printf '$ LD_LIBRARY_PATH=%s %s %s\n' "$PKG/lib/shared" "$W/bin/consume-corpus-shared" "$f" >> "$LOG"
  LD_LIBRARY_PATH="$PKG/lib/shared" "$W/bin/consume-corpus-shared" "$f" > "$W/out/shared/corpus-$id.jsonl" 2> "$W/out/shared/corpus-$id.err"
  rc=$?
  printf '[exit] %d\n' "$rc" >> "$LOG"; cat "$W/out/shared/corpus-$id.err" >> "$LOG"
  echo "RUN shared/corpus $id EXIT=$rc"; [ $rc -ne 0 ] && OVERALL=1
done

printf '\n=== [shared-rpath] consume-corpus-shared-rpath (no LD_LIBRARY_PATH) flac ===\n' >> "$LOG"
printf '$ env -u LD_LIBRARY_PATH %s %s\n' "$W/bin/consume-corpus-shared-rpath" "$W/fixtures/flac-16-44-stereo.flac" >> "$LOG"
env -u LD_LIBRARY_PATH "$W/bin/consume-corpus-shared-rpath" "$W/fixtures/flac-16-44-stereo.flac" \
  > "$W/out/shared/rpath-flac.jsonl" 2> "$W/out/shared/rpath-flac.err"
rc=$?
printf '[exit] %d\n' "$rc" >> "$LOG"
echo "RUN shared-rpath/flac EXIT=$rc"; [ $rc -ne 0 ] && OVERALL=1

printf '\n=== [shared] negative control: run WITHOUT LD_LIBRARY_PATH (expect loader failure 127) ===\n' >> "$LOG"
printf '$ env -u LD_LIBRARY_PATH %s <mp3>\n' "$W/bin/consume-corpus-shared" >> "$LOG"
env -u LD_LIBRARY_PATH "$W/bin/consume-corpus-shared" "$W/fixtures/mp3-cbr-id3v23.mp3" > "$W/out/shared/nolibpath.out" 2>&1
rc=$?
printf '[exit] %d\n' "$rc" >> "$LOG"; cat "$W/out/shared/nolibpath.out" >> "$LOG"
echo "NEGATIVE_CONTROL no_LD_LIBRARY_PATH exit=$rc (expected non-zero)"

echo "SHARED_RUNS_EXIT=$OVERALL"
echo
echo "----- ldd / loader evidence -----"
sed -n '/=== 1\./,/=== 4\./p' "$LOG"
