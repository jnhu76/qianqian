set -euo pipefail
export PATH="$HOME/.local/bin:$PATH"
WT="$(wslpath -u "$QR_WT")"
ROOT="$QR_ROOT"
W="$ROOT/win"

# Defense in depth for the rm/cp operations below: the work root must be a
# plain absolute WSL-local path.
case "$ROOT" in
  /home/*|/root/*|/tmp/*) : ;;
  *) echo "refusing suspicious work root: '$ROOT'"; exit 11 ;;
esac

mkdir -p "$ROOT" "$W"
# Refresh only git-controlled source subtrees; build/ (xmake cache +
# closure) persists across runs -> interrupted runs resume incrementally.
rm -rf "$W/native/src" "$W/native/include" "$W/native/ffmpeg" \
       "$W/native/tools" "$W/native/scripts" "$W/native/experiments" \
       "$W/native/xmake.lua" "$W/native/README.md" \
       "$W/native/build/ffmpeg.lua" "$W/native/build/songcore.lua"
mkdir -p "$W/native/build"
cd "$WT"
tar cf - \
    --exclude='native/build/artifacts' \
    --exclude='native/build/xmake' \
    --exclude='native/build/downloads' \
    --exclude='native/build/ffmpeg-xmake' \
    --exclude='native/build/ffmpeg-src' \
    native | tar xf - -C "$W"

cd "$W/native"
EXPECT=$(python3 -c "import json;p=json.load(open('ffmpeg/pin.json'));print(p['ffmpeg_commit_sha']+'|'+p['source_sha256'])")

check_marker() {
    local ACTUAL
    ACTUAL=$(cat build/ffmpeg-src/.qianqian-verified)
    if [ "$ACTUAL" != "$EXPECT" ]; then
        echo "closure pin mismatch: '$ACTUAL' != '$EXPECT'"
        exit 11
    fi
}
complete_closure() {
    test -f build/ffmpeg-xmake/manifest.json &&
    test -f build/ffmpeg-src/.qianqian-verified &&
    test -f build/ffmpeg-xmake/oracle/config_components.h
}
cache_valid() {
    test -f "$ROOT/closure/ffmpeg-xmake/manifest.json" &&
    test -f "$ROOT/closure/ffmpeg-src/.qianqian-verified" &&
    test -f "$ROOT/closure/ffmpeg-xmake/oracle/config_components.h"
}
source_closure_valid() {
    test -f "$1/ffmpeg-xmake/manifest.json" &&
    test -f "$1/ffmpeg-src/.qianqian-verified" &&
    test -f "$1/ffmpeg-xmake/oracle/config_components.h"
}
place_closure() {
    rm -rf build/ffmpeg-xmake build/ffmpeg-src
    cp -a "$1/ffmpeg-xmake" build/ffmpeg-xmake
    cp -a "$1/ffmpeg-src" build/ffmpeg-src
    check_marker
}

if complete_closure; then
    check_marker
    echo "closure: persistent build root (reused, pin-validated)"
elif cache_valid; then
    place_closure "$ROOT/closure"
    echo "closure: work-root cache"
elif [ -n "$QR_CLOSURE_SRC" ] && source_closure_valid "$QR_CLOSURE_SRC"; then
    place_closure "$QR_CLOSURE_SRC"
    echo "closure: copied from -ClosureSource (no re-import)"
else
    echo "FAIL: no complete imported FFmpeg closure available."
    echo "  The RC lane never downloads or re-imports FFmpeg."
    echo "  Pass -ClosureSource <wsl-path> pointing at a complete imported"
    echo "  closure (ffmpeg-src/ + ffmpeg-xmake/{manifest.json,oracle}/), or"
    echo "  run the D2 import once (ops/verify-build.ps1) first."
    exit 11
fi

# Keep the work-root cache in sync with the placed closure so later runs and
# the -BuildDir second root can reuse it without any import.
if complete_closure; then
    mkdir -p "$ROOT/closure"
    rm -rf "$ROOT/closure/ffmpeg-src" "$ROOT/closure/ffmpeg-xmake"
    cp -a build/ffmpeg-src "$ROOT/closure/ffmpeg-src"
    cp -a build/ffmpeg-xmake "$ROOT/closure/ffmpeg-xmake"
fi

test -f build/ffmpeg-xmake/manifest.json || { echo "no manifest after closure step"; exit 11; }
echo "PREPARE_OK"