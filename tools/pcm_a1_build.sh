#!/usr/bin/env bash
# E10-A1: build the five SRC runner binaries (bench-only).
#   bypass  - P0 BYPASS reference (same-rate only)
#   swr     - FFmpeg libswresample (pinned n9.0.1, c5 oracle build = code
#             already present in Qianqian's FFmpeg closure)
#   soxr    - soxr 0.1.3 (fetched, untracked build/e10-src)
#   r8b     - r8brain-free-src 6.5 (fetched, untracked)
#   lsr     - libsamplerate 0.2.2 (fetched, untracked)
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
E10="$ROOT/build/e10-src"
OUT="$ROOT/build/pcm-a1"
mkdir -p "$OUT"

WRAP="-Wl,--wrap=malloc,--wrap=calloc,--wrap=realloc,--wrap=free,--wrap=aligned_alloc"

# ---- soxr ----
if [ ! -f "$E10/soxr/build/libsoxr.a" ]; then
  cmake -S "$E10/soxr" -B "$E10/soxr/build" \
    -DCMAKE_POLICY_VERSION_MINIMUM=3.5 \
    -DBUILD_SHARED_LIBS=OFF -DBUILD_EXAMPLES=OFF -DBUILD_TESTS=OFF \
    -DSOXR_LSR_BINDINGS=OFF -DBUILD_LSR_BINDINGS=OFF \
    -DCMAKE_BUILD_TYPE=Release >/dev/null
  cmake --build "$E10/soxr/build" -j"$(nproc)" >/dev/null
fi
SOXR_LIB="$E10/soxr/build/src/libsoxr.a"
[ -f "$SOXR_LIB" ] || { echo "soxr lib missing"; exit 1; }

# ---- libsamplerate ----
if [ ! -f "$E10/libsamplerate/build/libsamplerate.a" ]; then
  mkdir -p "$E10/libsamplerate/build"
  ( cd "$E10/libsamplerate" && \
    for f in samplerate.c src_sinc.c src_linear.c src_zoh.c; do \
      cc -O2 -fPIC -DPACKAGE=\"libsamplerate\" -DVERSION=\"0.2.2\" \
         -DENABLE_SINC_BEST_CONVERTER -DENABLE_SINC_MEDIUM_CONVERTER \
         -DENABLE_SINC_FAST_CONVERTER \
         -c "src/$f" -Isrc -Iinclude -o "build/$(basename $f .c).o"; \
    done && \
    ar rcs build/libsamplerate.a build/samplerate.o build/src_sinc.o \
       build/src_linear.o build/src_zoh.o )
fi
LSR_LIB="$E10/libsamplerate/build/libsamplerate.a"
[ -f "$LSR_LIB" ] || { echo "lsr lib missing"; exit 1; }

# ---- r8brain objects ----
if [ ! -f "$E10/r8brain-free-src/build/r8bbase.o" ]; then
  mkdir -p "$E10/r8brain-free-src/build"
  ( cd "$E10/r8brain-free-src" && \
    c++ -O2 -std=c++14 -fPIC -c r8bbase.cpp -o build/r8bbase.o && \
    c++ -O2 -std=c++14 -fPIC -c pffft.cpp -o build/pffft.o )
fi
R8B_OBJS="$E10/r8brain-free-src/build/r8bbase.o $E10/r8brain-free-src/build/pffft.o"

# ---- swr (c5 oracle from pinned FFmpeg n9.0.1) ----
SWR_LIB="$ROOT/build/minimize/c5/oracle/libswresample/libswresample.a"
AVUTIL_LIB="$ROOT/build/minimize/c5/oracle/libavutil/libavutil.a"
if [ ! -f "$SWR_LIB" ] || [ ! -f "$AVUTIL_LIB" ]; then
  echo "swr oracle libs missing (run the repo's FFmpeg oracle build first)"
  exit 1
fi

FFINC="-I$ROOT/build/ffmpeg-src -I$ROOT/build/minimize/c5/oracle"
SRCINC="-I$ROOT/bench/pcm/src -I$ROOT/bench/pcm"

echo "building runners..."
cc -O2 -std=c11 $SRCINC -DCANDIDATE='"bypass"' \
   "$ROOT/bench/pcm/src/a1_harness.c" "$ROOT/bench/pcm/src/a1_bypass.c" \
   -o "$OUT/a1_run_bypass" $WRAP -lm

cc -O2 -std=c11 $SRCINC $FFINC -DCANDIDATE='"swr"' \
   "$ROOT/bench/pcm/src/a1_harness.c" "$ROOT/bench/pcm/src/a1_swr.c" \
   "$SWR_LIB" "$AVUTIL_LIB" -o "$OUT/a1_run_swr" $WRAP -lm

cc -O2 -std=c11 $SRCINC -I"$E10/soxr/src" -DCANDIDATE='"soxr"' \
   "$ROOT/bench/pcm/src/a1_harness.c" "$ROOT/bench/pcm/src/a1_soxr.c" \
   "$SOXR_LIB" -o "$OUT/a1_run_soxr" $WRAP -lm -fopenmp

cc -O2 -std=c11 $SRCINC -DCANDIDATE='"r8b"' -c \
   "$ROOT/bench/pcm/src/a1_harness.c" -o "$OUT/a1_harness_r8b.o"
g++ -O2 -std=c++14 $SRCINC -I"$E10/r8brain-free-src" \
   "$OUT/a1_harness_r8b.o" "$ROOT/bench/pcm/src/a1_r8b.cpp" \
   $R8B_OBJS -o "$OUT/a1_run_r8b" $WRAP -lm

cc -O2 -std=c11 $SRCINC -I"$E10/libsamplerate/include" -DCANDIDATE='"lsr"' \
   "$ROOT/bench/pcm/src/a1_harness.c" "$ROOT/bench/pcm/src/a1_lsr.c" \
   "$LSR_LIB" -o "$OUT/a1_run_lsr" $WRAP -lm

echo "runners built in $OUT:"
ls -la "$OUT" | grep a1_run
