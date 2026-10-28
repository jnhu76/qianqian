#!/usr/bin/env bash
# run_smoke.sh - #324 Windows native smoke suite (S1 / S2 / S3).
#
# Run from the worktree root with the Windows mingw toolchain on PATH:
#   export PATH="/c/Users/fred1/tools/mingw64/bin:/c/Users/fred1/tools/bin:$PATH"
#   export CARGO_TARGET_X86_64_PC_WINDOWS_GNU_AR=x86_64-w64-mingw32-gcc-ar
#   bash evidence/windows-native-tier2-324/harness/run_smoke.sh
#
# Writes receipts under evidence/windows-native-tier2-324/raw/smoke/.
set -euo pipefail

WT="$(cd "$(dirname "$0")/../../.." && pwd)"
PKG="$WT/evidence/windows-native-tier2-324"
OUT="$PKG/raw/smoke"
MANIFEST="$WT/crates/qianqian-decode-songcore/Cargo.toml"
TARGET="x86_64-pc-windows-gnu"
FIXTURE="$WT/native/experiments/songcore-equivalence/fixtures/flac-16-44-stereo.flac"

mkdir -p "$OUT"
cd "$WT"
export QIANQIAN_NATIVE_DIR="$WT/native"

echo "== fixture identity"
sha256sum "$FIXTURE" | tee "$OUT/fixture-identity.txt"

echo "== S1: native Windows C consumer (build + link + run)"
gcc -O2 -Wall -Wextra -I native/include \
  "$PKG/harness/songcore_consumer_smoke.c" \
  native/build/artifacts/libsongcore.a -lbcrypt \
  -o "$OUT/songcore_consumer_smoke.exe"
{
  echo "# build: gcc 16.2.0 (WinLibs) -I native/include harness/songcore_consumer_smoke.c native/build/artifacts/libsongcore.a -lbcrypt"
  sha256sum "$OUT/songcore_consumer_smoke.exe"
  "$OUT/songcore_consumer_smoke.exe" "$FIXTURE"
  echo "EXIT=$?"
} > "$OUT/consumer-smoke.txt" 2>&1
cat "$OUT/consumer-smoke.txt"

echo "== S2: Rust adapter smoke"
cargo test --manifest-path "$MANIFEST" --target "$TARGET" --test windows_native_smoke --no-run
S2=$(ls "$WT"/crates/qianqian-decode-songcore/target/$TARGET/debug/deps/windows_native_smoke-*.exe | head -1)
{
  sha256sum "$S2"
  "$S2" --nocapture --test-threads=1
} > "$OUT/s2-rust-adapter.txt" 2>&1
cat "$OUT/s2-rust-adapter.txt"

echo "== S3: FFmpeg callback ownership (three FRESH processes)"
cargo test --manifest-path "$MANIFEST" --target "$TARGET" --test windows_ffmpeg_callback_ownership --no-run
S3=$(ls "$WT"/crates/qianqian-decode-songcore/target/$TARGET/debug/deps/windows_ffmpeg_callback_ownership-*.exe | head -1)
{
  sha256sum "$S3"
  echo "--- case unset ---"
  env -u QIANQIAN_FFMPEG_LOG Q324_LOG_CASE=unset "$S3" --nocapture --test-threads=1
  echo "--- case zero ---"
  QIANQIAN_FFMPEG_LOG=0 Q324_LOG_CASE=zero "$S3" --nocapture --test-threads=1
  echo "--- case one ---"
  QIANQIAN_FFMPEG_LOG=1 Q324_LOG_CASE=one "$S3" --nocapture --test-threads=1
} > "$OUT/s3-ffmpeg-callback.txt" 2>&1
cat "$OUT/s3-ffmpeg-callback.txt"

echo "SMOKE SUITE COMPLETE"
