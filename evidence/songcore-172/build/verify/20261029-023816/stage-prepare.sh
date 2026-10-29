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

if [ -f build/ffmpeg-xmake/manifest.json ] && [ -f build/ffmpeg-src/.qianqian-verified ]; then
    check_marker
    echo "closure: worktree in-tree"
elif [ -f "$ROOT/closure/manifest.json" ] && [ -f "$ROOT/closure/ffmpeg-src/.qianqian-verified" ]; then
    mkdir -p build/ffmpeg-xmake
    cp "$ROOT/closure/manifest.json" build/ffmpeg-xmake/manifest.json
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
if [ -f build/ffmpeg-src/.qianqian-verified ]; then
    rm -rf "$ROOT/closure"
    mkdir -p "$ROOT/closure"
    cp -a build/ffmpeg-src "$ROOT/closure/ffmpeg-src"
    cp build/ffmpeg-xmake/manifest.json "$ROOT/closure/manifest.json"
fi

test -f build/ffmpeg-xmake/manifest.json || { echo "no manifest after closure step"; exit 11; }
echo "PREPARE_OK"