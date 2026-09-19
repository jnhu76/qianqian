#!/usr/bin/env bash
# tools/package-windows.sh — build the portable Windows package
# (WINDOWS-TUI-LISTENING-RELEASE-1, Issue #166 remaining closure).
#
#   dist/qianqian-windows-x86_64/
#       qianqian.exe QUICKSTART.md LICENSE LICENSE-MIT LICENSE-APACHE
#       THIRD_PARTY_NOTICES.md BUILD-MANIFEST.txt
#   dist/qianqian-windows-x86_64.zip
#
# Behavior: cross-builds the release product binary against the mingw
# SongCore artifact, FAILS CLOSED on any non-system DLL import (the
# package is then incomplete until the runtime file is shipped and
# documented — no silent partial archives), stages exactly the product
# files, writes the manifest, zips, prints the archive SHA256.
#
# Usage: tools/package-windows.sh   (from anywhere; paths are resolved
# relative to the repository root, never embedded into the package).

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET="x86_64-pc-windows-gnu"
PROFILE="release"
FEATURES="playback"
PKG_NAME="qianqian-windows-x86_64"
DIST="$REPO_ROOT/dist"
STAGE="$DIST/$PKG_NAME"
ZIP="$DIST/$PKG_NAME.zip"

MINGW_ARCHIVE="native/build/artifacts-mingw/libsongcore.a"
SONGCORE_HEADER="native/include/songcore.h"

# Windows system DLLs qianqian.exe may import without shipping
# anything (case-insensitive; OS api-set names are allowed by prefix).
SYSTEM_DLLS=(
    kernel32.dll user32.dll userenv.dll ws2_32.dll advapi32.dll
    shell32.dll bcrypt.dll bcryptprimitives.dll combase.dll msvcrt.dll
    ntdll.dll ole32.dll oleaut32.dll propsys.dll rpcrt4.dll
    gdi32.dll imm32.dll uxtheme.dll dwmapi.dll shcore.dll secur32.dll
    crypt32.dll sspicli.dll version.dll winmm.dll msacm32.dll
)

fail() { echo "package-windows: FAIL: $*" >&2; exit 1; }
note() { echo "package-windows: $*"; }

sha256() { sha256sum "$1" | awk '{print $1}'; }

command -v cargo      >/dev/null 2>&1 || fail "cargo not found"
command -v objdump    >/dev/null 2>&1 || fail "objdump not found"
command -v zip        >/dev/null 2>&1 || fail "zip not found"
command -v x86_64-w64-mingw32-gcc >/dev/null 2>&1 \
    || fail "mingw cross linker x86_64-w64-mingw32-gcc not found (see .cargo/config.toml)"
[[ -f "$REPO_ROOT/$MINGW_ARCHIVE" ]] \
    || fail "mingw SongCore artifact missing: $MINGW_ARCHIVE (build it first — see native/README.md)"
[[ -f "$REPO_ROOT/$SONGCORE_HEADER" ]] || fail "SongCore header missing: $SONGCORE_HEADER"

# --- 1. stage the native tree the cross build links against ----------
NATIVE_STAGE="$(mktemp -d "${TMPDIR:-/tmp}/qn-package-native.XXXXXX")"
trap 'rm -rf "$NATIVE_STAGE"' EXIT
mkdir -p "$NATIVE_STAGE/build/artifacts"
cp -r "$REPO_ROOT/native/include" "$NATIVE_STAGE/include"
cp "$REPO_ROOT/$MINGW_ARCHIVE" "$NATIVE_STAGE/build/artifacts/libsongcore.a"
SONGCORE_SHA="$(sha256 "$REPO_ROOT/$MINGW_ARCHIVE")"
HEADER_SHA="$(sha256 "$REPO_ROOT/$SONGCORE_HEADER")"

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
       RUSTFLAGS="--remap-path-prefix=$HOME/=/qianqian-build/ -Clink-arg=-lbcrypt" \
       cargo build --release --target "$TARGET" --features "$FEATURES" \
            -p qianqian-headless )
EXE="$REPO_ROOT/target/$TARGET/$PROFILE/qianqian.exe"
[[ -f "$EXE" ]] || fail "build produced no qianqian.exe"
EXE_SHA="$(sha256 "$EXE")"
EXE_SIZE="$(stat -c%s "$EXE")"

# The remap must be complete: any surviving build-host path in the
# binary fails the package (fail-closed, like every other gate here).
if command -v strings >/dev/null 2>&1; then
    if strings "$EXE" | grep -qF "$HOME"; then
        fail "qianqian.exe embeds build-host paths under $HOME — \
the --remap-path-prefix above did not cover everything"
    fi
    note "  build-host path remap verified (no $HOME strings in the exe)"
fi

# --- 3. runtime self-containment audit (fail closed) -----------------
note "auditing qianqian.exe DLL imports"
IMPORTS="$(objdump -p "$EXE" | sed -n 's/.*DLL Name: //p' | tr -d '\r' | sort -fu)"
[[ -n "$IMPORTS" ]] || fail "objdump reported no DLL imports at all (suspicious)"
while IFS= read -r dll; do
    [[ -z "$dll" ]] && continue
    lower="$(printf '%s' "$dll" | tr '[:upper:]' '[:lower:]')"
    ok=0
    case "$lower" in
        api-ms-win-*|ext-ms-win-*) ok=1 ;;
    esac
    if [[ $ok -eq 0 ]]; then
        for sys in "${SYSTEM_DLLS[@]}"; do
            [[ "$lower" == "$sys" ]] && { ok=1; break; }
        done
    fi
    [[ $ok -eq 1 ]] || fail "qianqian.exe imports non-system DLL '$dll'. \
Ship that runtime file beside qianqian.exe, document it in \
THIRD_PARTY_NOTICES.md and BUILD-MANIFEST.txt, and re-run — the package \
must not silently omit a required runtime dependency."
done <<< "$IMPORTS"
note "  imports (all Windows system DLLs): $(echo "$IMPORTS" | tr '\n' ' ')"

# --- 4. stage the package --------------------------------------------
rm -rf "$STAGE"
mkdir -p "$STAGE"
for f in QUICKSTART.md LICENSE LICENSE-MIT LICENSE-APACHE THIRD_PARTY_NOTICES.md; do
    [[ -f "$REPO_ROOT/$f" ]] || fail "required package file missing: $f"
    cp "$REPO_ROOT/$f" "$STAGE/$f"
done
cp "$EXE" "$STAGE/qianqian.exe"

# --- 5. manifest ------------------------------------------------------
COMMIT="$(git -C "$REPO_ROOT" rev-parse HEAD 2>/dev/null || echo "unknown")"
if [[ -n "$(git -C "$REPO_ROOT" status --porcelain 2>/dev/null)" ]]; then
    SOURCE_STATUS="dirty worktree — the binary may not match the commit"
else
    SOURCE_STATUS="clean"
fi
RUSTC_VERSION="$(rustc --version)"
BUILD_UTC="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
MANIFEST="$STAGE/BUILD-MANIFEST.txt"
{
    echo "qianqian-windows-x86_64 — build manifest"
    echo "========================================"
    echo "package:            $PKG_NAME"
    echo "built_utc:          $BUILD_UTC"
    echo "product:            qianqian.exe (the canonical product binary;"
    echo "                    qianqian-headless.exe is a development/"
    echo "                    regression target and is NOT shipped)"
    echo
    echo "source"
    echo "  commit:           $COMMIT"
    echo "  status:           $SOURCE_STATUS"
    echo "  build command:    QIANQIAN_NATIVE_DIR=<staged native tree> \\"
    echo "                    cargo build --release \\"
    echo "                    --target x86_64-pc-windows-gnu \\"
    echo "                    --features playback -p qianqian-headless"
    echo
    echo "toolchain"
    echo "  rustc:            $RUSTC_VERSION"
    echo "  target triple:    $TARGET (GNU/mingw)"
    echo "  profile:          $PROFILE"
    echo "  features:         $FEATURES"
    echo
    echo "native decode artifact"
    echo "  songcore archive: libsongcore.a (mingw) sha256 $SONGCORE_SHA"
    echo "  songcore header:  songcore.h sha256 $HEADER_SHA"
    echo "  substrate:        FFmpeg n9.0.1 (bf1b838f), LGPL-2.1-or-later,"
    echo "                    trimmed closure — see THIRD_PARTY_NOTICES.md"
    echo
    echo "shipped files"
    for f in qianqian.exe QUICKSTART.md LICENSE LICENSE-MIT \
             LICENSE-APACHE THIRD_PARTY_NOTICES.md BUILD-MANIFEST.txt; do
        if [[ -f "$STAGE/$f" ]]; then
            printf '  %-22s %s bytes  sha256 %s\n' \
                "$f" "$(stat -c%s "$STAGE/$f")" "$(sha256 "$STAGE/$f")"
        fi
    done
    echo
    echo "runtime dependency audit"
    echo "  DLL imports:      $(echo "$IMPORTS" | tr '\n' ' ')"
    echo "  verdict:          all imports are Windows system DLLs;"
    echo "                    no runtime files beyond qianqian.exe are"
    echo "                    required (decode stack statically linked)"
    echo
    echo "reproducibility"
    echo "  note:             rebuilds of this toolchain are not byte-"
    echo "                    stable; identity of the delivered artifact"
    echo "                    is the sha256 above, recorded per build"
} > "$MANIFEST"

# --- 6. zip ------------------------------------------------------------
rm -f "$ZIP"
( cd "$DIST" && zip -r -X -q "$PKG_NAME.zip" "$PKG_NAME" )
note "archive: $ZIP"
note "sha256:  $(sha256 "$ZIP")"

# --- 7. archive content gate (fail closed) -----------------------------
FORBIDDEN="$(unzip -Z1 "$ZIP" | grep -Ei \
    'qianqian-headless|/target/|^\.git/|Cargo\.(toml|lock)|\.rs$|\.pdb$|fixtures|evidence' \
    || true)"
[[ -z "$FORBIDDEN" ]] || fail "archive contains forbidden entries: $FORBIDDEN"
REQUIRED_OK=1
for f in "$PKG_NAME/qianqian.exe" "$PKG_NAME/QUICKSTART.md" "$PKG_NAME/LICENSE" \
         "$PKG_NAME/THIRD_PARTY_NOTICES.md" "$PKG_NAME/BUILD-MANIFEST.txt"; do
    unzip -Z1 "$ZIP" | grep -qx "$f" || { note "  missing in archive: $f"; REQUIRED_OK=0; }
done
[[ $REQUIRED_OK -eq 1 ]] || fail "archive is missing required entries"

note "package contents:"
unzip -Z1 "$ZIP" | sed 's/^/  /'
note "OK"
