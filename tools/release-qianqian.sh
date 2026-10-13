#!/usr/bin/env bash
# One local/CI prepare path; no publication operation.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
case "${1:-}" in
  --help|-h|'')
    echo 'usage: release-qianqian.sh prepare|verify --tag TAG --source SHA --mode formal|dry-run [--zip ZIP]'
    echo 'prepare always bootstraps native from tracked source in a fresh temporary directory; never publishes.'
    exit 0 ;;
  verify) shift; exec python3 "$ROOT/tools/check_release.py" verify "$@" ;;
  prepare) shift ;;
  *) echo 'POLICY_RED: unsupported operation; local/dry-run publication forbidden' >&2; exit 2 ;;
esac
TAG='' SOURCE='' MODE=''
while [[ $# -gt 0 ]]; do
  [[ $# -ge 2 ]] || { echo 'TOOLING_FAIL: missing argument value' >&2; exit 3; }
  case "$1" in
    --tag) TAG="$2" ;;
    --source) SOURCE="$2" ;;
    --mode) MODE="$2" ;;
    *) echo "TOOLING_FAIL: unknown argument $1" >&2; exit 3 ;;
  esac
  shift 2
done
python3 "$ROOT/tools/check_release.py" source --tag "$TAG" --source "$SOURCE" --mode "$MODE"
for tool in git python3 curl unzip make nasm pkg-config xmake cargo rustc x86_64-w64-mingw32-gcc x86_64-w64-mingw32-ar objdump zip sha256sum; do
  command -v "$tool" >/dev/null || { echo "TOOLING_FAIL: missing $tool" >&2; exit 3; }
done
BUILD_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/qianqian-release-native.XXXXXX")"
trap 'rm -rf "$BUILD_ROOT"' EXIT
# No local generated tree, verified stamp or archive may enter this build.
git -C "$ROOT" archive "$SOURCE" native | tar -x -C "$BUILD_ROOT"
NATIVE="$BUILD_ROOT/native"
if ! (cd "$NATIVE" && python3 tools/ffmpeg_import.py --recipe windows-x86_64 &&
      xmake f -c -p mingw -a x86_64 -m release --mingw=/usr -y &&
      xmake build songcore_static); then
  echo 'BUILD_FAIL: fresh Windows native bootstrap' >&2
  exit 4
fi
python3 "$ROOT/tools/check_release.py" native --tag "$TAG" --source "$SOURCE" --mode "$MODE" --native "$NATIVE"
QIANQIAN_RELEASE_NATIVE_DIR="$NATIVE" bash "$ROOT/tools/package-windows.sh" "$TAG" "$SOURCE" "$MODE"
python3 "$ROOT/tools/check_release.py" source --tag "$TAG" --source "$SOURCE" --mode "$MODE"
