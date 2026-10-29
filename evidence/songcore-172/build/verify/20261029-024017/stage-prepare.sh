set -euo pipefail
export PATH="$HOME/.local/bin:$PATH"
WT="$(wslpath -u "$Q172_WT")"
EV="$(wslpath -u "$Q172_EV")"
ROOT="$Q172_ROOT"
W="$ROOT/win"

rm -rf "$W"
mkdir -p "$W/native/build"
cd "$WT"
tar cf - \
    --exclude='native/build/artifacts' \
    --exclude='native/build/xmake' \
    native | tar xf - -C "$W"

cd "$W/native"
EXPECT=$(python3 -c "import json;p=json.load(open('ffmpeg/pin.json'));print(p['ffmpeg_commit_sha']+'|'+p['source_sha256'])")

check_marker() {
    ACTUAL=$(cat build/ffmpeg-src/.qianqian-verified)
    if [ "$ACTUAL" != "$EXPECT" ]; then
        echo "closure pin mismatch: '$ACTUAL' != '$EXPECT'"
        exit 11
    fi
}

# The replay needs the complete closure: the pinned source tree (source_root)
# AND the imported oracle dir with the generated config headers
# (config_root = build/ffmpeg-xmake/oracle 鈥?config.h, config_components.h;
# the manifest's generated units live there). A cache holding only the
# source + manifest fails exactly like a missing import
# (`config_components.h: No such file or directory`), so the cache is valid
# only with the oracle present.
cache_valid() {
    test -f "$ROOT/closure/manifest.json" &&
    test -f "$ROOT/closure/ffmpeg-src/.qianqian-verified" &&
    test -f "$ROOT/closure/ffmpeg-xmake/oracle/config_components.h"
}

if [ -f build/ffmpeg-xmake/manifest.json ] && [ -f build/ffmpeg-src/.qianqian-verified ] &&
   [ -f build/ffmpeg-xmake/oracle/config_components.h ]; then
    check_marker
    echo "closure: worktree in-tree"
elif cache_valid; then
    rm -rf build/ffmpeg-xmake
    cp -a "$ROOT/closure/ffmpeg-xmake" build/ffmpeg-xmake
    cp -a "$ROOT/closure/ffmpeg-src" build/ffmpeg-src
    check_marker
    echo "closure: verify-run cache"
else
    echo "closure: importing (one-time oracle cost)"
    bash scripts/fetch-ffmpeg > "$EV/closure-fetch.log" 2>&1
    python3 tools/ffmpeg_import.py --recipe windows-x86_64 > "$EV/closure-import.log" 2>&1
    tail -3 "$EV/closure-import.log"
fi

# Persist the verified closure for later verify runs (WSL-local, fast).
# Only a COMPLETE closure (source pin marker + manifest + oracle headers)
# is worth caching.
if [ -f build/ffmpeg-src/.qianqian-verified ] &&
   [ -f build/ffmpeg-xmake/manifest.json ] &&
   [ -f build/ffmpeg-xmake/oracle/config_components.h ]; then
    rm -rf "$ROOT/closure"
    mkdir -p "$ROOT/closure"
    cp -a build/ffmpeg-src "$ROOT/closure/ffmpeg-src"
    cp -a build/ffmpeg-xmake "$ROOT/closure/ffmpeg-xmake"
fi

test -f build/ffmpeg-xmake/manifest.json || { echo "no manifest after closure step"; exit 11; }
echo "PREPARE_OK"