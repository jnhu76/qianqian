#!/usr/bin/env bash
# Lane E: measured link-time requirements and real consumer runs (linux-x86_64).
set -u
export LC_ALL=C

ART=/home/hoo/g4-172/linux-root/src/native/build/artifacts
INC=/home/hoo/g4-172/linux-root/src/native/include
EVD=/mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/g4/lane-e-runtime-deps
RAW="$EVD/raw"
SRC_ABI=/mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/audit/abi-complete-consumer.c
SRC_EXT=/mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/linux/verify/package/external-consumer-example.c
MEDIA=/home/hoo/Source/qianqian/native/experiments/songcore-equivalence/fixtures/mp3-cbr-id3v23.mp3
MEDIA2=/home/hoo/Source/qianqian/apps/headless/tests/fixtures/mp3-cbr-cover.mp3
W="$HOME/lane-e-runtime/work"
LOG="$RAW/build-run-exitcodes.txt"
: > "$LOG"

logrun() {
  local desc="$1"; local out="$2"; shift 2
  "$@" > "$out" 2>&1
  local rc=$?
  printf '%s | argv: %s | exit=%s | output=%s\n' "$desc" "$*" "$rc" "$out" >> "$LOG"
  return $rc
}

rm -rf "$W"
mkdir -p "$W/static" "$W/shared" "$W/src"

cp "$SRC_ABI" "$W/src/abi-complete-consumer.c"
cp "$SRC_EXT" "$W/src/external-consumer-example.c"
cp "$INC/songcore.h" "$W/src/songcore.h"

# ---------------- input identity ----------------
{
  echo "== consumer sources and header sha256 =="
  sha256sum "$W/src/abi-complete-consumer.c" "$W/src/external-consumer-example.c" "$W/src/songcore.h"
  echo
  echo "== media fixtures =="
  ls -l "$MEDIA" "$MEDIA2"
  sha256sum "$MEDIA" "$MEDIA2"
} > "$RAW/inputs-sha256.txt" 2>&1
printf 'inputs identity | argv: sha256sum sources/header/media | exit=%s | output=%s\n' "$?" "$RAW/inputs-sha256.txt" >> "$LOG"

# ---------------- ABI sizeof probe source ----------------
cat > "$W/src/abi-sizeof-probe.c" <<'EOF'
#include "songcore.h"
#include <stdio.h>
#include <stddef.h>
int main(void) {
    printf("abi_version=%u\n", (unsigned)songcore_abi_version());
    printf("sizeof(song_status)=%zu\n", sizeof(song_status));
    printf("sizeof(song_io)=%zu\n", sizeof(song_io));
    printf("sizeof(song_info)=%zu\n", sizeof(song_info));
    printf("sizeof(song_stream_info)=%zu\n", sizeof(song_stream_info));
    printf("sizeof(song_metadata)=%zu\n", sizeof(song_metadata));
    printf("sizeof(song_handle*)=%zu\n", sizeof(song_handle *));
    printf("off(song_info.codec)=%zu\n", offsetof(song_info, codec));
    printf("off(song_info.container)=%zu\n", offsetof(song_info, container));
    printf("off(song_info.sample_rate)=%zu\n", offsetof(song_info, sample_rate));
    printf("off(song_info.channels)=%zu\n", offsetof(song_info, channels));
    printf("off(song_info.duration_us)=%zu\n", offsetof(song_info, duration_us));
    printf("off(song_info.audio_stream_count)=%zu\n", offsetof(song_info, audio_stream_count));
    printf("off(song_info.selected_audio_index)=%zu\n", offsetof(song_info, selected_audio_index));
    return songcore_abi_version() == 1u ? 0 : 1;
}
EOF

# ---------------- static link requirement matrix ----------------
(
  cd "$W"
  logrun "static abi-complete WITH -lm -lpthread" "$RAW/link-static-libs.log" \
    gcc -O2 -I"$INC" src/abi-complete-consumer.c -L"$ART" -lsongcore -lm -lpthread -o static/abi-complete-libs
  logrun "static abi-complete WITHOUT -lm -lpthread" "$RAW/link-static-nolibs.log" \
    gcc -O2 -I"$INC" src/abi-complete-consumer.c -L"$ART" -lsongcore -o static/abi-complete-nolibs
  logrun "static abi-complete with ONLY -lpthread (no -lm)" "$RAW/link-static-nomath.log" \
    gcc -O2 -I"$INC" src/abi-complete-consumer.c -L"$ART" -lsongcore -lpthread -o static/abi-complete-nomath
  logrun "static abi-complete with ONLY -lm (no -lpthread)" "$RAW/link-static-nopthread.log" \
    gcc -O2 -I"$INC" src/abi-complete-consumer.c -L"$ART" -lsongcore -lm -o static/abi-complete-nopthread
  printf '#include "songcore.h"\n#include <stdio.h>\nint main(void){printf("%%u\\n",(unsigned)songcore_abi_version());return songcore_abi_version()==1u?0:1;}\n' > src/minimal-consumer.c
  logrun "static MINIMAL consumer, -lsongcore only" "$RAW/link-static-minimal-nolibs.log" \
    gcc -O2 -I"$INC" src/minimal-consumer.c -L"$ART" -lsongcore -o static/minimal-nolibs
  logrun "static MINIMAL consumer, -lsongcore -lm -lpthread" "$RAW/link-static-minimal-libs.log" \
    gcc -O2 -I"$INC" src/minimal-consumer.c -L"$ART" -lsongcore -lm -lpthread -o static/minimal-libs
  logrun "static external-consumer-example with -lm -lpthread" "$RAW/link-static-external.log" \
    gcc -O2 -I"$INC" src/external-consumer-example.c -L"$ART" -lsongcore -lm -lpthread -o static/external-consumer-static
  logrun "static sizeof probe" "$RAW/link-static-sizeof.log" \
    gcc -O2 -I"$INC" src/abi-sizeof-probe.c -L"$ART" -lsongcore -lm -lpthread -o static/abi-sizeof-probe-static
)

# ---------------- shared link and staging ----------------
(
  cd "$W"
  cp "$ART/shared/libsongcore.so" shared/libsongcore.so
  logrun "shared external-consumer-example rpath ORIGIN" "$RAW/link-shared-external.log" \
    gcc -O2 -I"$INC" src/external-consumer-example.c -L"$ART/shared" -lsongcore -lm -lpthread -Wl,-rpath,'$ORIGIN' -o shared/external-consumer-shared
  logrun "shared abi-complete rpath ORIGIN" "$RAW/link-shared-abi.log" \
    gcc -O2 -I"$INC" src/abi-complete-consumer.c -L"$ART/shared" -lsongcore -lm -lpthread -Wl,-rpath,'$ORIGIN' -o shared/abi-complete-shared
  logrun "shared sizeof probe rpath ORIGIN" "$RAW/link-shared-sizeof.log" \
    gcc -O2 -I"$INC" src/abi-sizeof-probe.c -L"$ART/shared" -lsongcore -lm -lpthread -Wl,-rpath,'$ORIGIN' -o shared/abi-sizeof-probe-shared
)

# ---------------- binary linkage facts ----------------
for b in "$W/static/abi-complete-libs" "$W/static/minimal-libs" "$W/static/external-consumer-static" "$W/static/abi-sizeof-probe-static" "$W/shared/external-consumer-shared" "$W/shared/abi-complete-shared" "$W/shared/abi-sizeof-probe-shared"; do
  [ -e "$b" ] || continue
  n=$(basename "$b")
  file "$b" > "$RAW/file-$n.txt" 2>&1
  printf 'file %s | argv: file %s | exit=%s | output=%s\n' "$n" "$b" "$?" "$RAW/file-$n.txt" >> "$LOG"
  readelf -d "$b" > "$RAW/readelf-d-$n.txt" 2>&1
  printf 'readelf -d %s | argv: readelf -d %s | exit=%s | output=%s\n' "$n" "$b" "$?" "$RAW/readelf-d-$n.txt" >> "$LOG"
  ldd "$b" > "$RAW/ldd-$n.txt" 2>&1
  printf 'ldd %s | argv: ldd %s | exit=%s | output=%s\n' "$n" "$b" "$?" "$RAW/ldd-$n.txt" >> "$LOG"
done

# ---------------- real runs (current host, glibc 2.43) ----------------
: > "$RAW/consumer-runs.txt"
runone() {
  local desc="$1"; local out="$2"; shift 2
  "$@" > "$out" 2>&1
  local rc=$?
  {
    echo "### $desc"
    echo "argv: $*"
    echo "exit: $rc"
    echo "--- stdout/stderr ---"
    cat "$out"
    echo
  } >> "$RAW/consumer-runs.txt"
  printf '%s | argv: %s | exit=%s | output=%s\n' "$desc" "$*" "$rc" "$out" >> "$LOG"
}

runone "static abi-complete-libs" "$RAW/run-static-abi-complete.txt" "$W/static/abi-complete-libs"
runone "static minimal-libs" "$RAW/run-static-minimal.txt" "$W/static/minimal-libs"
runone "static external-consumer (mp3-cbr-id3v23)" "$RAW/run-static-external.txt" "$W/static/external-consumer-static" "$MEDIA"
runone "static sizeof probe" "$RAW/run-static-sizeof.txt" "$W/static/abi-sizeof-probe-static"
( cd "$W/shared" && "$W/shared/abi-complete-shared" > "$RAW/run-shared-abi-complete.txt" 2>&1; echo "exit=$?" >> "$RAW/run-shared-abi-complete.txt" )
( cd "$W/shared" && "$W/shared/external-consumer-shared" "$MEDIA" > "$RAW/run-shared-external.txt" 2>&1; echo "exit=$?" >> "$RAW/run-shared-external.txt" )
( cd "$W/shared" && "$W/shared/abi-sizeof-probe-shared" > "$RAW/run-shared-sizeof.txt" 2>&1; echo "exit=$?" >> "$RAW/run-shared-sizeof.txt" )
for f in run-shared-abi-complete run-shared-external run-shared-sizeof; do
  printf '%s | argv: (cd %s/shared && ./%s) | exit=%s | output=%s\n' "$f" "$W" "$f" "$(tail -n1 "$RAW/$f.txt" | sed 's/exit=//')" "$RAW/$f.txt" >> "$LOG"
done

echo "done"
