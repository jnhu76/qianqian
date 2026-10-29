set -euo pipefail
export PATH="$HOME/.local/bin:$PATH"
WT="$(wslpath -u "$Q172_WT")"
EV="$(wslpath -u "$Q172_EV")"
ROOT="$Q172_ROOT"
W="$ROOT/win"
cd "$W/native"

xmake f -p mingw -a x86_64 -m release -y > "$EV/config.log" 2>&1
xmake build -r songcore > "$EV/build.log" 2>&1

test -f build/artifacts/libsongcore.a || { echo "FAIL: libsongcore.a missing after green build"; exit 12; }
test ! -e build/artifacts/shared || { echo "FAIL: shared/ artifact dir exists (recipe says shared=false)"; exit 13; }

# grep -c exits 1 on a zero count, which is a valid count here; suspend -e
# around the captures and assert the values explicitly below.
set +e
PE=$(x86_64-w64-mingw32-objdump -f build/artifacts/libsongcore.a 2>/dev/null | grep -c 'file format pe-x86-64')
ELF=$(x86_64-w64-mingw32-objdump -f build/artifacts/libsongcore.a 2>/dev/null | grep -c 'file format elf')
SONG=$(x86_64-w64-mingw32-nm build/artifacts/libsongcore.a 2>/dev/null | grep -c ' T song')
set -e
if [ "$PE" -le 0 ] || [ "$ELF" -ne 0 ]; then
    echo "FAIL: member formats pe=$PE elf=$ELF (expected pe>0, elf==0)"
    exit 14
fi
if [ "$SONG" -ne 15 ]; then
    echo "FAIL: song* ABI symbols = $SONG (expected 15)"
    exit 15
fi

# Negative control: the recipe does not enable the shared form, so an
# explicit build MUST fail closed. Exiting 0 here means the policy regressed.
if xmake build songcore_shared > "$EV/negative-shared.log" 2>&1; then
    echo "FAIL: explicit songcore_shared exited 0 (recipe policy broken)"
    exit 16
fi

# Copy the artifacts + closure manifest back into the worktree (the #324
# consumption step: downstream consumers find native/build/artifacts).
mkdir -p "$WT/native/build/artifacts" "$WT/native/build/ffmpeg-xmake"
cp build/artifacts/libsongcore.a "$WT/native/build/artifacts/libsongcore.a"
cp build/artifacts/libqianqian_av.a "$WT/native/build/artifacts/libqianqian_av.a"
cp build/ffmpeg-xmake/manifest.json "$WT/native/build/ffmpeg-xmake/manifest.json"

SHA=$(sha256sum "$WT/native/build/artifacts/libsongcore.a" | awk '{print $1}')
SIZE=$(stat -c '%s' "$WT/native/build/artifacts/libsongcore.a")
echo "PE_MEMBERS=$PE"
echo "SONG_ABI=$SONG"
echo "ARTIFACT_SHA256=$SHA"
echo "ARTIFACT_SIZE=$SIZE"
echo "BUILD_OK"