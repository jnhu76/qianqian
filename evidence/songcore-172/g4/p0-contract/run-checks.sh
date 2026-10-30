#!/usr/bin/env bash
# p0-contract lane: read-only contract/source consistency measurement.
# SongCore #172 G4 pre-check. Writes only under evidence/songcore-172/g4/p0-contract/
# and $HOME/q172-g4-p0/ scratch. Does not modify any tracked file.
set -u
SRC=/home/hoo/Source/qianqian
WIN=/mnt/c/Users/fred1/source/qianqian
OUT=$WIN/evidence/songcore-172/g4/p0-contract
SCRATCH=/home/hoo/q172-g4-p0
TARBALL=/home/hoo/s2pkg-v010/songcore-v0.1.0-linux-x86_64.tar.gz
mkdir -p "$OUT" "$SCRATCH"
cd "$SRC" || { echo "FATAL cannot cd $SRC"; exit 99; }

CONTRACT_HEADER_SHA=cf509a5c044735e4813642ac6683691242146e642342a8ac356e98fbd8e78536
CONTRACT_PROFILE_SHA=57fb70ed2c29a8464d939c611b3c5ed1bc99879438eb7f159530dfa52de554fa
PIN_SHA_PREFIX=224ad3d5
PIN_SHA_SUFFIX=ff85
WIN_RECIPE_SHA_PREFIX=3ba0a9e6
WIN_RECIPE_SHA_SUFFIX=8492d
PINNED_ZIP_SHA=d16837ddbb0963753aa2739971a09b07fe14fe855a6e84f253f851127f99c746

mark(){ echo; echo "########## $* ##########"; }

mark "A0 git identity of the measured tree"
git rev-parse HEAD; echo "exit=$?"
git status --porcelain | head -5; echo "exit_git_status=${PIPESTATUS[0]}"

mark "A1 sha256sum all contract inputs (WSL source tree)"
sha256sum native/include/songcore.h \
          native/ffmpeg/pin.json \
          native/ffmpeg/profiles/codec-base.json \
          native/ffmpeg/targets/linux-x86_64.json \
          native/ffmpeg/targets/windows-x86_64.json \
          native/xmake.lua \
          native/build/songcore.lua \
          native/build/ffmpeg.lua \
          docs/release/songcore-release-contract.md \
          docs/architecture/songcore-binding-architecture.md \
          > "$OUT/sha256-inputs.txt" 2>&1
echo "exit=$?"; cat "$OUT/sha256-inputs.txt"

mark "A2 same header through the Windows worktree mount"
sha256sum "$WIN/native/include/songcore.h" > "$OUT/sha256-win-worktree.txt" 2>&1
echo "exit=$?"; cat "$OUT/sha256-win-worktree.txt"

mark "C1 contract 3 header sha256 equality"
HDR_SHA=$(sha256sum "$SRC/native/include/songcore.h" | awk '{print $1}')
echo "measured=$HDR_SHA"
echo "contract=$CONTRACT_HEADER_SHA"
if [ "$HDR_SHA" = "$CONTRACT_HEADER_SHA" ]; then echo "C1=MATCH"; else echo "C1=MISMATCH"; fi

mark "C2a SONGCORE_ABI_VERSION define lines"
grep -n 'SONGCORE_ABI_VERSION' native/include/songcore.h > "$OUT/abi-version-lines.txt" 2>&1
echo "exit=$?"; cat "$OUT/abi-version-lines.txt"

mark "C2b SONGCORE_API top-level declarations (grep, line-numbered)"
grep -nE '^SONGCORE_API[[:space:]]' native/include/songcore.h > "$OUT/export-declarations.txt" 2>&1
echo "exit=$?"; cat "$OUT/export-declarations.txt"

mark "C2c grep declaration count"
grep -cE '^SONGCORE_API[[:space:]]' native/include/songcore.h > "$OUT/export-count-grep.txt" 2>&1
echo "exit=$?"; cat "$OUT/export-count-grep.txt"

mark "C2d python parse of the export set (names + non-API top-level decls)"
python3 - native/include/songcore.h > "$OUT/export-parse.txt" 2>&1 <<'PY'
import re, sys
p = sys.argv[1]
src = open(p, encoding='utf-8').read()
names = re.findall(r'^SONGCORE_API\s+[A-Za-z_][A-Za-z0-9_]*\s+([A-Za-z_][A-Za-z0-9_]*)\s*\(', src, re.M)
print("decl_count=%d" % len(names))
for i, n in enumerate(names, 1):
    print("%02d %s" % (i, n))
print("unique=%d" % len(set(names)))
lines = src.splitlines()
others = []
for i, l in enumerate(lines, 1):
    if l.startswith('SONGCORE_API') or l.startswith('typedef'):
        continue
    if re.match(r'^[A-Za-z_][A-Za-z0-9_]*[ \t]+\*?[A-Za-z_][A-Za-z0-9_]*[ \t]*\(', l):
        others.append((i, l))
print("non_api_top_level_fn_decls=%d" % len(others))
for ln, l in others:
    print("  line %d: %s" % (ln, l))
PY
echo "exit=$?"; cat "$OUT/export-parse.txt"

mark "C2e extracted names vs contract 3 list"
printf '%s\n' songcore_abi_version song_open song_probe song_audio_stream_count \
  song_audio_stream_info song_select_stream song_get_metadata \
  song_get_metadata_count song_get_metadata_entry song_get_artwork_count \
  song_get_artwork_item song_last_error song_read_pcm song_seek song_close \
  | sort > "$OUT/contract-3-list-sorted.txt"
echo "exit=$?"
awk '/^[0-9][0-9] /{print $2}' "$OUT/export-parse.txt" | sort > "$OUT/header-names-sorted.txt"
echo "exit=$?"
echo "contract_list_count=$(wc -l < "$OUT/contract-3-list-sorted.txt")"
echo "header_list_count=$(wc -l < "$OUT/header-names-sorted.txt")"
if diff -u "$OUT/contract-3-list-sorted.txt" "$OUT/header-names-sorted.txt" > "$OUT/name-diff.txt" 2>&1; then
  echo "C2e=IDENTICAL_SETS"
else
  echo "C2e=DIFF"; cat "$OUT/name-diff.txt"
fi

mark "C3 pin.json fields"
python3 - native/ffmpeg/pin.json > "$OUT/pin-fields.txt" 2>&1 <<'PY'
import json, sys
d = json.load(open(sys.argv[1]))
for k in ('ffmpeg_tag', 'ffmpeg_commit_sha', 'ffmpeg_version', 'source_url', 'source_sha256'):
    print("%s=%s" % (k, d.get(k)))
print("has_verification_block=%s" % ('verification' in d))
print("pin_keys=%s" % ",".join(sorted(d.keys())))
PY
echo "exit=$?"; cat "$OUT/pin-fields.txt"

mark "C4 profile codec-base.json fields"
python3 - native/ffmpeg/profiles/codec-base.json > "$OUT/profile-fields.txt" 2>&1 <<'PY'
import json, sys
d = json.load(open(sys.argv[1]))
print("profile=%s" % d.get('profile'))
print("component_base=%s" % d.get('component_base'))
libs = d.get('libraries', {})
print("libraries.enable=%s" % ",".join(libs.get('enable', [])))
print("libraries.disable=%s" % ",".join(libs.get('disable', [])))
print("disable=%s" % ",".join(d.get('disable', [])))
c = d.get('components', {})
for k in ('demuxer', 'decoder', 'parser'):
    print("components.%s=%s" % (k, ",".join(c.get(k, []))))
print("provenance_keys=%s" % ",".join(sorted(d.get('provenance', {}).keys())))
PY
echo "exit=$?"; cat "$OUT/profile-fields.txt"

mark "C5 profile sha256 vs contract 5"
PROF_SHA=$(sha256sum "$SRC/native/ffmpeg/profiles/codec-base.json" | awk '{print $1}')
echo "measured=$PROF_SHA"
echo "contract=$CONTRACT_PROFILE_SHA"
if [ "$PROF_SHA" = "$CONTRACT_PROFILE_SHA" ]; then echo "C5=MATCH"; else echo "C5=MISMATCH"; fi

mark "C6 pin.json file sha256 vs contract 15 abbreviated record"
PIN_SHA=$(sha256sum "$SRC/native/ffmpeg/pin.json" | awk '{print $1}')
echo "measured=$PIN_SHA"
echo "contract_abbrev=${PIN_SHA_PREFIX}...${PIN_SHA_SUFFIX}"
case "$PIN_SHA" in
  ${PIN_SHA_PREFIX}*${PIN_SHA_SUFFIX}) echo "C6=ABBREV_CONSISTENT";;
  *) echo "C6=ABBREV_CONFLICT";;
esac
echo "pin_source_sha256_measured=$(awk -F= '/^source_sha256=/{print $2}' "$OUT/pin-fields.txt")"
echo "pinned_zip_expected=$PINNED_ZIP_SHA"

mark "C7 target recipe artifacts/status (contract 4 and 6)"
python3 - native/ffmpeg/targets/linux-x86_64.json native/ffmpeg/targets/windows-x86_64.json > "$OUT/targets-fields.txt" 2>&1 <<'PY'
import json, sys
for p in sys.argv[1:]:
    d = json.load(open(p))
    print("file=%s" % p)
    print("  id=%s platform=%s arch=%s" % (d.get('id'), d.get('platform'), d.get('arch')))
    print("  artifacts=%s" % json.dumps(d.get('artifacts'), sort_keys=True))
    print("  status=%s" % d.get('status'))
    print("  toolchain.family=%s" % (d.get('toolchain') or {}).get('family'))
    print("  ffmpeg.target_os=%s cross=%s cross_prefix=%s" % (
        (d.get('ffmpeg') or {}).get('target_os'),
        (d.get('ffmpeg') or {}).get('cross'),
        (d.get('ffmpeg') or {}).get('cross_prefix')))
PY
echo "exit=$?"; cat "$OUT/targets-fields.txt"

mark "C8 windows recipe sha256 vs contract 15 abbreviated record"
WINSHA=$(sha256sum "$SRC/native/ffmpeg/targets/windows-x86_64.json" | awk '{print $1}')
echo "measured=$WINSHA"
echo "contract_abbrev=${WIN_RECIPE_SHA_PREFIX}...${WIN_RECIPE_SHA_SUFFIX}"
case "$WINSHA" in
  ${WIN_RECIPE_SHA_PREFIX}*${WIN_RECIPE_SHA_SUFFIX}) echo "C8=ABBREV_CONSISTENT";;
  *) echo "C8=ABBREV_CONFLICT";;
esac

mark "C9 xmake project/version lines"
grep -n 'set_project\|set_version' native/xmake.lua > "$OUT/xmake-version-lines.txt" 2>&1
echo "exit=$?"; cat "$OUT/xmake-version-lines.txt"

mark "C10 repo-wide songcore version strings (what 2 equality would bind)"
grep -rn --include='*.py' --include='*.sh' --include='*.lua' --include='*.yml' --include='*.json' \
  -e 'songcore_version' -e 'songcore-v0' -e 'songcore-vX' \
  tools native ops .github docs 2>/dev/null > "$OUT/version-strings.txt"
echo "exit=$?"
wc -l < "$OUT/version-strings.txt"
head -40 "$OUT/version-strings.txt"

mark "C11 v0.1.0 historical tarball cross-check (3 and 5 'byte-identical since v0.1.0')"
sha256sum "$TARBALL" > "$OUT/v010-tarball-sha.txt" 2>&1
echo "exit=$?"; cat "$OUT/v010-tarball-sha.txt"
rm -rf "$SCRATCH/v010"; mkdir -p "$SCRATCH/v010"
tar -xzf "$TARBALL" -C "$SCRATCH/v010" > "$OUT/v010-extract.txt" 2>&1
echo "tar_exit=$?"
ls "$SCRATCH/v010" >> "$OUT/v010-extract.txt" 2>&1
cat "$OUT/v010-extract.txt"
python3 - "$SCRATCH/v010" > "$OUT/v010-manifest-fields.txt" 2>&1 <<'PY'
import json, os, sys
root = sys.argv[1]
tops = os.listdir(root)
print("top_entries=%s" % tops)
mp = os.path.join(root, tops[0], 'metadata', 'manifest.json')
d = json.load(open(mp))
print("manifest_path=%s" % mp)
for k in ('schema_version', 'songcore_version', 'songcore_abi', 'abi_header_sha256',
          'target', 'platform', 'arch', 'toolchain', 'ffmpeg_version', 'ffmpeg_commit',
          'ffmpeg_source_sha256', 'ffmpeg_profile', 'ffmpeg_profile_sha256',
          'archive_members'):
    print("%s=%s" % (k, d.get(k)))
PY
echo "exit=$?"; cat "$OUT/v010-manifest-fields.txt"

mark "C12 v0.1.0 shipped header bytes vs source header"
V010_HDR=$(find "$SCRATCH/v010" -name songcore.h | head -1)
echo "shipped_header=$V010_HDR"
sha256sum "$V010_HDR" "$SRC/native/include/songcore.h" > "$OUT/v010-header-vs-source.txt" 2>&1
echo "exit=$?"; cat "$OUT/v010-header-vs-source.txt"

echo
echo "########## DONE ##########"
