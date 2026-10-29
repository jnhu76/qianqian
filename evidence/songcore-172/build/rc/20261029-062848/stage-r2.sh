set -euo pipefail
export PATH="$HOME/.local/bin:$PATH"
WT="$(wslpath -u "$QR_WT")"
EV="$(wslpath -u "$QR_EV")"
ROOT="$QR_ROOT"
B="$QR_BUILDDIR"
CS="$QR_CLOSURE_SRC"
W2="$B/win"

# Defense in depth: the second root must be a plain absolute WSL-local path
# and must be set at all before any rm/cp below runs.
if [ -z "$B" ]; then
    echo "QR_BUILDDIR is empty; second-root stage must not run"
    exit 11
fi
case "$B" in
  /home/*|/root/*|/tmp/*) : ;;
  *) echo "refusing suspicious build dir: '$B'"; exit 11 ;;
esac
if [ "$B" = "$ROOT" ]; then
    echo "second root equals the primary work root; refusing"
    exit 11
fi

# The second root is per-run fresh: an independent absolute root for the
# contract section 12 T1 comparison (same source SHA / pin / profile / recipe).
rm -rf "$W2"
mkdir -p "$W2/native/build"
cd "$WT"
tar cf - \
    --exclude='native/build/artifacts' \
    --exclude='native/build/xmake' \
    --exclude='native/build/downloads' \
    --exclude='native/build/ffmpeg-xmake' \
    --exclude='native/build/ffmpeg-src' \
    native | tar xf - -C "$W2"

cd "$W2/native"
EXPECT=$(python3 -c "import json;p=json.load(open('ffmpeg/pin.json'));print(p['ffmpeg_commit_sha']+'|'+p['source_sha256'])")

check_marker() {
    local ACTUAL
    ACTUAL=$(cat build/ffmpeg-src/.qianqian-verified)
    if [ "$ACTUAL" != "$EXPECT" ]; then
        echo "closure pin mismatch: '$ACTUAL' != '$EXPECT'"
        exit 11
    fi
}
source_closure_valid() {
    test -f "$1/ffmpeg-xmake/manifest.json" &&
    test -f "$1/ffmpeg-src/.qianqian-verified" &&
    test -f "$1/ffmpeg-xmake/oracle/config_components.h"
}
place_closure() {
    cp -a "$1/ffmpeg-xmake" build/ffmpeg-xmake
    cp -a "$1/ffmpeg-src" build/ffmpeg-src
    check_marker
}

# Reuse an already-imported closure only; never download/re-import here.
if [ -n "$CS" ] && source_closure_valid "$CS"; then
    place_closure "$CS"
    echo "closure: copied from -ClosureSource"
elif source_closure_valid "$ROOT/closure"; then
    place_closure "$ROOT/closure"
    echo "closure: copied from primary work-root cache"
else
    echo "FAIL: no complete imported FFmpeg closure available for the second root."
    exit 11
fi

xmake f -p mingw -a x86_64 -m release -y > "$EV/r2-config.log" 2>&1
xmake build songcore > "$EV/r2-build.log" 2>&1

test -f build/artifacts/libsongcore.a || { echo "FAIL: R2 libsongcore.a missing"; exit 12; }
test ! -e build/artifacts/shared || { echo "FAIL: R2 shared/ dir exists"; exit 13; }

set +e
PE2=$(x86_64-w64-mingw32-objdump -f build/artifacts/libsongcore.a 2>/dev/null | grep -c 'file format pe-x86-64')
ELF2=$(x86_64-w64-mingw32-objdump -f build/artifacts/libsongcore.a 2>/dev/null | grep -c 'file format elf')
SONG2=$(x86_64-w64-mingw32-nm build/artifacts/libsongcore.a 2>/dev/null | grep -c ' T song')
set -e
if [ "$PE2" -le 0 ] || [ "$ELF2" -ne 0 ]; then
    echo "FAIL: R2 member formats pe=$PE2 elf=$ELF2"
    exit 14
fi
if [ "$SONG2" -ne 15 ]; then
    echo "FAIL: R2 song* ABI symbols = $SONG2 (expected 15)"
    exit 15
fi
if xmake build songcore_shared > "$EV/r2-negative-shared.log" 2>&1; then
    echo "FAIL: R2 explicit songcore_shared exited 0 (recipe policy broken)"
    exit 16
fi

# The second root's artifact lands in the run's evidence dir only -- the
# worktree consumption path keeps the primary root's artifact.
mkdir -p "$EV/r2-artifacts"
cp build/artifacts/libsongcore.a "$EV/r2-artifacts/libsongcore.a"

SHA2=$(sha256sum "$EV/r2-artifacts/libsongcore.a" | awk '{print $1}')
SIZE2=$(stat -c '%s' "$EV/r2-artifacts/libsongcore.a")
echo "R2_PE_MEMBERS=$PE2"
echo "R2_ELF_MEMBERS=$ELF2"
echo "R2_SONG_ABI=$SONG2"
echo "R2_ARTIFACT_SHA256=$SHA2"
echo "R2_ARTIFACT_SIZE=$SIZE2"
echo "R2_BUILD_OK"