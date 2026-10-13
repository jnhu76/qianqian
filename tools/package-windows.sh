#!/usr/bin/env bash
# tools/package-windows.sh — build the portable Windows package
# (WINDOWS-TUI-LISTENING-RELEASE-1, Issue #166 remaining closure).
#
#   dist/qianqian-vVERSION-windows-x86_64/
#       qianqian.exe QUICKSTART.md LICENSE LICENSE-MIT LICENSE-APACHE
#       THIRD_PARTY_NOTICES.md BUILD-MANIFEST.txt
#   dist/qianqian-vVERSION-windows-x86_64.zip
#
# Behavior: cross-builds the release product binary against the mingw
# SongCore artifact, FAILS CLOSED on any non-system DLL import (the
# package is then incomplete until the runtime file is shipped and
# documented — no silent partial archives), stages exactly the product
# files, writes the manifest, zips, prints the archive SHA256.
#
# Usage: invoked by release-qianqian.sh prepare; paths are resolved
# relative to the repository root, never embedded into the package).

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET="x86_64-pc-windows-gnu"
PROFILE="release"
FEATURES="playback"
[[ $# -eq 3 ]] || { echo 'usage: package-windows.sh TAG SOURCE_SHA formal|dry-run' >&2; exit 3; }
TAG="$1"
COMMIT="$2"
MODE="$3"
python3 "$REPO_ROOT/tools/check_release.py" source --tag "$TAG" --source "$COMMIT" --mode "$MODE"
PKG_NAME="$TAG-windows-x86_64"
DIST="$REPO_ROOT/dist"
STAGE="$DIST/$PKG_NAME"
ZIP="$DIST/$PKG_NAME.zip"

[[ -n "${QIANQIAN_RELEASE_NATIVE_DIR:-}" ]] || { echo 'EVIDENCE_MISSING: use release-qianqian.sh prepare to bootstrap native' >&2; exit 5; }
MINGW_ARCHIVE="$QIANQIAN_RELEASE_NATIVE_DIR/build/artifacts/libsongcore.a"
SONGCORE_HEADER="native/include/songcore.h"

fail() { echo "package-windows: FAIL: $*" >&2; exit 3; }
note() { echo "package-windows: $*"; }

sha256() { sha256sum "$1" | awk '{print $1}'; }

command -v cargo      >/dev/null 2>&1 || fail "cargo not found"
command -v objdump    >/dev/null 2>&1 || fail "objdump not found"
command -v zip        >/dev/null 2>&1 || fail "zip not found"
command -v x86_64-w64-mingw32-gcc >/dev/null 2>&1 \
    || fail "mingw cross linker x86_64-w64-mingw32-gcc not found (see .cargo/config.toml)"
[[ -f "$MINGW_ARCHIVE" ]] \
    || fail "mingw SongCore artifact missing: $MINGW_ARCHIVE (build it first — see native/README.md)"
[[ -f "$REPO_ROOT/$SONGCORE_HEADER" ]] || fail "SongCore header missing: $SONGCORE_HEADER"

# --- 1. stage the native tree the cross build links against ----------
NATIVE_STAGE="$(mktemp -d "${TMPDIR:-/tmp}/qn-package-native.XXXXXX")"
trap 'rm -rf "$NATIVE_STAGE"' EXIT
mkdir -p "$NATIVE_STAGE/build/artifacts"
cp -r "$QIANQIAN_RELEASE_NATIVE_DIR/include" "$NATIVE_STAGE/include"
cp "$MINGW_ARCHIVE" "$NATIVE_STAGE/build/artifacts/libsongcore.a"
cmp "$MINGW_ARCHIVE" "$NATIVE_STAGE/build/artifacts/libsongcore.a"

# --- 2. build the release product binaries ---------------------------
note "building $TARGET/$PROFILE (features: $FEATURES)"
# Remap every build-host path (rustup/cargo/repo all live under $HOME)
# out of the binary's panic-location metadata: a shipped player must
# not carry the developer's home layout inside it. The env RUSTFLAGS
# REPLACES the target rustflags in .cargo/config.toml, so the mingw
# rustc bcrypt link arg is repeated here (rustc's windows-gnu runtime
# references BCrypt*; the cross linker does not pick it up by default).
( cd "$REPO_ROOT" \
    && QIANQIAN_NATIVE_DIR="$NATIVE_STAGE" \
       CARGO_TARGET_DIR="$REPO_ROOT/target" \
       RUSTFLAGS="--remap-path-prefix=$HOME/=/qianqian-build/ --remap-path-prefix=$REPO_ROOT/=/qianqian-source/ -Clink-arg=-lbcrypt" \
       cargo build --locked --release --target "$TARGET" --features "$FEATURES" \
            -p qianqian-headless --bin qianqian ) || { echo "BUILD_FAIL: Rust Windows product" >&2; exit 4; }
EXE="$REPO_ROOT/target/$TARGET/$PROFILE/qianqian.exe"
[[ -f "$EXE" ]] || fail "build produced no qianqian.exe"

# The remap must be complete: any surviving build-host path in the
# binary fails the package (fail-closed, like every other gate here).
command -v strings >/dev/null 2>&1 || fail "strings not found"
if command -v strings >/dev/null 2>&1; then
    if strings "$EXE" | grep -F "$HOME" > /dev/null; then
        fail "qianqian.exe embeds build-host paths under $HOME — \
the --remap-path-prefix above did not cover everything"
    fi
    note "  build-host path remap verified (no $HOME strings in the exe)"
fi

# --- 3. DLL audit is performed by the shared manifest checker below.
# Verify freshly built native evidence before packaging.
python3 "$REPO_ROOT/tools/check_release.py" native --tag "$TAG" --source "$COMMIT" \
    --mode "$MODE" --native "$QIANQIAN_RELEASE_NATIVE_DIR"

# --- 4. stage the package --------------------------------------------
rm -rf "$STAGE"
mkdir -p "$STAGE"
for f in QUICKSTART.md LICENSE LICENSE-MIT LICENSE-APACHE THIRD_PARTY_NOTICES.md; do
    [[ -f "$REPO_ROOT/$f" ]] || fail "required package file missing: $f"
    cp "$REPO_ROOT/$f" "$STAGE/$f"
done
cp "$EXE" "$STAGE/qianqian.exe"

# --- 5. Complete payload hashes; manifest never hashes its own partial content.
python3 "$REPO_ROOT/tools/check_release.py" manifest --tag "$TAG" --source "$COMMIT" \
    --mode "$MODE" --native "$QIANQIAN_RELEASE_NATIVE_DIR" --stage "$STAGE"
cp "$QIANQIAN_RELEASE_NATIVE_DIR/build/release-provenance.json" "$ZIP.provenance.json"

# --- 6. External checksum protects the completed manifest too.
rm -f "$ZIP"
( cd "$DIST" && zip -r -X -q "$PKG_NAME.zip" "$PKG_NAME" )
( cd "$DIST" && sha256sum "$PKG_NAME.zip" > "$PKG_NAME.zip.sha256" )
python3 "$REPO_ROOT/tools/check_release.py" verify --tag "$TAG" --source "$COMMIT" \
    --mode "$MODE" --zip "$ZIP"
note "archive: $ZIP"
note "sha256: $(sha256 "$ZIP")"
note "OK"
