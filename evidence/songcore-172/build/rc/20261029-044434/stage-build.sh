set -euo pipefail
export PATH="$HOME/.local/bin:$PATH"
WT="$(wslpath -u "$QR_WT")"
EV="$(wslpath -u "$QR_EV")"
ROOT="$QR_ROOT"
W="$ROOT/win"
cd "$W/native"

xmake f -p mingw -a x86_64 -m release -y > "$EV/config.log" 2>&1
xmake build songcore > "$EV/build.log" 2>&1

test -f build/artifacts/libsongcore.a || { echo "FAIL: libsongcore.a missing after build"; exit 12; }
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

# Provenance crosscheck: the closure manifest actually used must agree with
# pin.json, the profile file digest, and the recipe file digest (V3-style
# carried-duplicate byte-equality, contract section 8).
python3 -c "
import json, hashlib, sys
m = json.load(open('build/ffmpeg-xmake/manifest.json'))
p = json.load(open('ffmpeg/pin.json'))
errs = []
def sha(p_): return hashlib.sha256(open(p_, 'rb').read()).hexdigest()
if m['ffmpeg_tag'] != p['ffmpeg_tag']: errs.append('ffmpeg_tag')
if m['ffmpeg_commit_sha'] != p['ffmpeg_commit_sha']: errs.append('ffmpeg_commit_sha')
if m['ffmpeg_source_sha256'] != p['source_sha256']: errs.append('ffmpeg_source_sha256')
if m.get('profile_sha256') != sha('ffmpeg/profiles/' + m.get('profile', '') + '.json'): errs.append('profile_sha256')
mrs = (m.get('target') or {}).get('recipe_sha256', '')
if mrs != sha('ffmpeg/targets/windows-x86_64.json'): errs.append('recipe_sha256')
if errs:
    print('CLOSURE_CROSSCHECK=FAIL: ' + ','.join(errs)); sys.exit(17)
print('CLOSURE_CROSSCHECK=OK')
print('CLOSURE_UNITS=%d' % len(m['units']))
print('FFMPEG_TAG=' + m['ffmpeg_tag'])
print('FFMPEG_COMMIT=' + m['ffmpeg_commit_sha'])
print('FFMPEG_SOURCE_SHA256=' + m['ffmpeg_source_sha256'])
print('PROFILE=' + m.get('profile', ''))
print('PROFILE_SHA256=' + m.get('profile_sha256', ''))
print('RECIPE_SHA256_IN_MANIFEST=' + mrs)
print('CLOSURE_MANIFEST_SHA256=' + sha('build/ffmpeg-xmake/manifest.json'))
"
RSHA_LOCAL=$(sha256sum ffmpeg/targets/windows-x86_64.json | awk '{print $1}')
echo "RECIPE_SHA256_LOCAL=$RSHA_LOCAL"
HSUM=$(sha256sum "$WT/native/include/songcore.h" | awk '{print $1}')
echo "HEADER_SHA256=$HSUM"

# Copy the artifacts + closure manifest back into the worktree (the D2
# consumption step: downstream consumers find native/build/artifacts).
mkdir -p "$WT/native/build/artifacts" "$WT/native/build/ffmpeg-xmake"
cp build/artifacts/libsongcore.a "$WT/native/build/artifacts/libsongcore.a"
cp build/artifacts/libqianqian_av.a "$WT/native/build/artifacts/libqianqian_av.a"
cp build/ffmpeg-xmake/manifest.json "$WT/native/build/ffmpeg-xmake/manifest.json"

SHA=$(sha256sum "$WT/native/build/artifacts/libsongcore.a" | awk '{print $1}')
SIZE=$(stat -c '%s' "$WT/native/build/artifacts/libsongcore.a")
echo "PE_MEMBERS=$PE"
echo "ELF_MEMBERS=$ELF"
echo "SONG_ABI=$SONG"
echo "ARTIFACT_SHA256=$SHA"
echo "ARTIFACT_SIZE=$SIZE"
echo "BUILD_OK"