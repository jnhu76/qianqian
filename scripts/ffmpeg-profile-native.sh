#!/usr/bin/env bash
set -euo pipefail

# Draft profile for Stage A only.
# Do not treat this as authoritative until corpus-driven reduction is complete.

FFMPEG_SRC="${FFMPEG_SRC:-./third_party/ffmpeg}"
PREFIX="${PREFIX:-./build/ffmpeg-native}"

cd "$FFMPEG_SRC"

./configure \
  --prefix="$PREFIX" \
  --disable-everything \
  --disable-programs \
  --disable-doc \
  --disable-network \
  --disable-autodetect \
  --enable-avformat \
  --enable-avcodec \
  --enable-avutil \
  --enable-swresample \
  --enable-demuxer=mp3 \
  --enable-demuxer=flac \
  --enable-decoder=mp3 \
  --enable-decoder=flac \
  --enable-parser=mpegaudio

echo
echo "IMPORTANT:"
echo "This is only a Stage A starting point."
echo "Every enabled component must later be justified by corpus evidence."
