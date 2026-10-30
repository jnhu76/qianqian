#!/usr/bin/env bash
# b329-build.sh — SongCore #329 two-root build driver (controller-owned, fail-closed).
#
# Usage: b329-build.sh <target:linux|windows> <root-dir> <fixmode> <mapto> <attempt>
#   fixmode: none | filemap | macromap | both
#   mapto:   "." | "src"   (canonical replacement; "src" is the causal counterfactual)
#
# Per root:
#   stage clone    fresh file:// clone of the seed at BASE b49b18c (reused when
#                  head+tracked-tree match; closure lives inside and survives).
#   stage closure  pinned FFmpeg zip (cache, sha-verified) + ffmpeg_import oracle
#                 derive, once per root; later attempts revalidate and reuse.
#   stage fix      restore pristine native/build/ffmpeg.lua, then (fixmode != none)
#                 insert the prefix-map flag into qianqian_av on_load so the flag
#                 reaches the actually replayed FFmpeg compile units.
#   stage build    xmake f <plat> + xmake -v build -r songcore (verbose log kept).
#   stage verify   format identity, member audit, ABI export set, leak census.
# Prints a compact JSON summary on stdout. Nothing is swallowed.
set -Eeuo pipefail
export PATH="$HOME/.local/bin:$PATH"
export LC_ALL=C

TARGET=${1:?target linux|windows}
ROOT=${2:?root dir}
FIXMODE=${3:?fixmode none|filemap|macromap|both}
MAPTO=${4:?mapto .|src}
ATTEMPT=${5:?attempt stamp}
BASE=${B329_BASE:-b49b18cd68ae1a353cd3396efb3977b8590cc4d1}
ORIGIN=git@github.com:jnhu76/qianqian.git
CACHE=/home/hoo/qq329/cache
PIN_SHA=d16837ddbb0963753aa2739971a09b07fe14fe855a6e84f253f851127f99c746
PIN_COMMIT=bf1b838f2ab88b4f8fd83443325c782ea0e0f7fa
ZIP_NAME=ffmpeg-n9.0.1.zip

case "$TARGET" in
  linux)  PLAT=linux; XPLAT="-p linux -a x86_64";  RECIPE=ffmpeg/targets/linux-x86_64.json;   RECIPE_ID=linux-x86_64;   IMPORT_ARGS="" ;;
  windows) PLAT=mingw; XPLAT="-p mingw -a x86_64"; RECIPE=ffmpeg/targets/windows-x86_64.json; RECIPE_ID=windows-x86_64; IMPORT_ARGS="--recipe windows-x86_64" ;;
  *) echo "FATAL: bad target $TARGET" >&2; exit 2 ;;
esac
case "$FIXMODE" in none|filemap|macromap|both) ;; *) echo "FATAL: bad fixmode" >&2; exit 2 ;; esac
case "$MAPTO" in .|src) ;; *) echo "FATAL: bad mapto" >&2; exit 2 ;; esac

# Per-root mutual exclusion: a superseded run's orphaned build must never fight a
# live one over the same root. Fail closed when a live holder exists.
mkdir -p "$ROOT"
LOCK="$ROOT/.b329-lock"
if ! mkdir "$LOCK" 2>/dev/null; then
  HOLDER=$(cat "$LOCK/pid" 2>/dev/null || true)
  if [ -n "$HOLDER" ] && ! kill -0 "$HOLDER" 2>/dev/null; then
    rm -rf "$LOCK"
    mkdir "$LOCK"
  else
    echo "FATAL: another b329-build holds $ROOT (pid ${HOLDER:-unknown})" >&2
    exit 1
  fi
fi
echo $$ > "$LOCK/pid"
trap 'rm -rf "$ROOT/.b329-lock"' EXIT

SRC=$ROOT/src
EVROOT=$ROOT/receipts
STAMP="${ATTEMPT}-${FIXMODE}-${MAPTO}"
EV=$EVROOT/$STAMP
FACTS=$EV/facts.env
STAGE=init
mkdir -p "$EV"
fact() { printf '%s=%s\n' "$1" "$2" >> "$FACTS"; }
log()  { printf '[b329] %s\n' "$*" >&2; }
on_err() { local rc=$?; printf '{"verdict":"FAIL","failed_stage":"%s","exit":%d}\n' "$STAGE" "$rc"; exit "$rc"; }
trap on_err ERR

fact target "$TARGET"; fact root "$ROOT"; fact fixmode "$FIXMODE"; fact mapto "$MAPTO"; fact attempt "$ATTEMPT"
fact base "$BASE"; fact uname "$(uname -sr)"; fact nproc "$(nproc)"
fact gcc "$(gcc --version | head -1)"

# ---------------------------------------------------------------- stage clone
STAGE=clone
reuse_src() {
  [ -d "$SRC/.git" ] || return 1
  ( cd "$SRC"
    [ "$(git rev-parse HEAD)" = "$BASE" ] || return 1
    # The fix stage's patch on the tracked build script is expected state between
    # attempts; restore it so clone+closure reuse survives across fix-mode swaps.
    git checkout -q -- native/build/ffmpeg.lua 2>/dev/null || true
    git diff-index --quiet HEAD -- ) || return 1
  return 0
}
if reuse_src; then
  log "stage1: reusing clone at $BASE"
  fact clone reuse
else
  log "stage1: fresh clone from origin into $SRC"
  rm -rf "$SRC" 2>/dev/null || true
  if [ -d "$SRC" ]; then sleep 5; rm -rf "$SRC" 2>/dev/null || true; fi
  if [ -d "$SRC" ]; then sleep 20; rm -rf "$SRC" || { echo "FATAL: $SRC undeletable (concurrent build process?)" >&2; exit 1; }; fi
  mkdir -p "$ROOT"
  git clone --quiet "$ORIGIN" "$SRC" >"$EV/clone.log" 2>&1
  git -C "$SRC" fetch --quiet origin "refs/heads/main" >>"$EV/clone.log" 2>&1
  git -C "$SRC" checkout --quiet --detach "$BASE" >>"$EV/clone.log" 2>&1
  [ "$(git -C "$SRC" rev-parse HEAD)" = "$BASE" ] || { echo "FATAL: clone head mismatch" >&2; exit 1; }
  git -C "$SRC" diff-index --quiet HEAD -- || { echo "FATAL: fresh clone dirty" >&2; exit 1; }
  fact clone fresh
  fact clone_origin "$ORIGIN"
fi
fact head "$(git -C "$SRC" rev-parse HEAD)"
fact tree "$(git -C "$SRC" rev-parse 'HEAD^{tree}')"
cd "$SRC/native"

# pristine ffmpeg.lua kept for fix-mode swapping
if [ ! -f "$ROOT/ffmpeg.lua.pristine" ]; then
  cp build/ffmpeg.lua "$ROOT/ffmpeg.lua.pristine"
fi

# ---------------------------------------------------------------- stage closure
STAGE=closure
PIN=ffmpeg/pin.json
MAN=build/ffmpeg-xmake/manifest.json
PROFILE=ffmpeg/profiles/codec-base.json
closure_reusable() {
  [ -f "$MAN" ] || return 1
  [ -f build/ffmpeg-src/.qianqian-verified ] || return 1
  grep -q "^${PIN_COMMIT}|${PIN_SHA}$" build/ffmpeg-src/.qianqian-verified || return 1
  [ -f build/ffmpeg-xmake/oracle/ffbuild/config.mak ] || return 1
  python3 - "$MAN" "$RECIPE" <<'PY' || return 1
import json, sys, hashlib, platform
man = json.load(open(sys.argv[1]))
rec = json.load(open(sys.argv[2]))
rec_sha = hashlib.sha256(open(sys.argv[2], 'rb').read()).hexdigest()
t, tt, tl = man.get("target", {}), rec.get("ffmpeg", {}), man.get("toolchain", {})
ok = (man.get("schema") == 2
      and t.get("id") == rec.get("id")
      and t.get("platform") == rec.get("platform")
      and t.get("arch") == rec.get("arch")
      and t.get("target_os") == tt.get("target_os")
      and t.get("cross_prefix") == tt.get("cross_prefix")
      and t.get("recipe_sha256") == rec_sha
      and tl.get("system") == platform.system().lower()
      and tl.get("machine") == platform.machine().lower())
sys.exit(0 if ok else 1)
PY
}
if closure_reusable; then
  log "stage2: reusing imported FFmpeg closure (identity revalidated)"
  fact closure reuse
else
  log "stage2: deriving FFmpeg closure (fetch + import oracle)"
  fact closure fresh
  mkdir -p build/downloads "$CACHE"
  if [ ! -f "$CACHE/$ZIP_NAME" ]; then
    for cand in "$HOME/qianqian-172-linux/native/build/downloads/$ZIP_NAME" \
                "$HOME/qianqian-172-linux-r2/native/build/downloads/$ZIP_NAME"; do
      if [ -f "$cand" ] && [ "$(sha256sum "$cand" | awk '{print $1}')" = "$PIN_SHA" ]; then
        cp "$cand" "$CACHE/$ZIP_NAME"; log "seeded zip cache from $cand"; break
      fi
    done
  fi
  if [ -f "$CACHE/$ZIP_NAME" ] && [ "$(sha256sum "$CACHE/$ZIP_NAME" | awk '{print $1}')" = "$PIN_SHA" ]; then
    cp "$CACHE/$ZIP_NAME" build/downloads/$ZIP_NAME
    fact zip_source "cache sha256-verified"
  else
    fact zip_source "network download via fetch-ffmpeg"
  fi
  bash scripts/fetch-ffmpeg >"$EV/fetch-ffmpeg.log" 2>&1 || {
    echo "FATAL: fetch-ffmpeg failed"; tail -5 "$EV/fetch-ffmpeg.log" >&2; exit 1; }
  python3 tools/ffmpeg_import.py $IMPORT_ARGS >"$EV/ffmpeg-import.log" 2>&1 || {
    echo "FATAL: ffmpeg_import failed"; tail -5 "$EV/ffmpeg-import.log" >&2; exit 1; }
fi
grep -E 'tag recheck|zip sha256' "$EV/fetch-ffmpeg.log" > "$EV/fetch-provenance-lines.txt" 2>/dev/null || true
fact manifest_sha256 "$(sha256sum "$MAN" | awk '{print $1}')"
fact recipe_sha256 "$(sha256sum "$RECIPE" | awk '{print $1}')"

# ---------------------------------------------------------------- stage fix
STAGE=fix
cp "$ROOT/ffmpeg.lua.pristine" build/ffmpeg.lua
if [ "$FIXMODE" != "none" ]; then
  python3 - "$FIXMODE" "$MAPTO" <<'PY'
import sys
mode, mapto = sys.argv[1], sys.argv[2]
path = "build/ffmpeg.lua"
text = open(path, encoding="utf-8").read()
anchor = '        target:add("includedirs", buildroot, srcroot, {public = true})\n'
assert text.count(anchor) == 1, f"anchor count {text.count(anchor)}"
ins = ""
if mode in ("filemap", "both"):
    ins += '        target:add("cflags", "-ffile-prefix-map=" .. srcroot .. "=%s", {force = true})\n' % mapto
if mode in ("macromap", "both"):
    ins += '        target:add("cflags", "-fmacro-prefix-map=" .. srcroot .. "=%s", {force = true})\n' % mapto
open(path, "w", encoding="utf-8").write(text.replace(anchor, anchor + ins))
print(f"patched {mode} mapto={mapto}", file=sys.stderr)
PY
  grep -n "prefix-map" build/ffmpeg.lua > "$EV/fix-patch-lines.txt"
  [ -s "$EV/fix-patch-lines.txt" ] || { echo "FATAL: fix patch missing" >&2; exit 1; }
fi
fact fix_applied "$FIXMODE"

# ---------------------------------------------------------------- stage build
STAGE=build
xmake f $XPLAT -m release -y >"$EV/xmake-config.log" 2>&1
xmake build -r songcore >"$EV/xmake-build.log" 2>&1
if [ "$TARGET" = linux ]; then
  xmake build songcore_static >"$EV/xmake-build-static-explicit.log" 2>&1
  xmake build songcore_shared >"$EV/xmake-build-shared-explicit.log" 2>&1
else
  xmake build songcore_static >"$EV/xmake-build-static-explicit.log" 2>&1
fi
log "stage4: xmake build OK"
fact build ok
ART_A=build/artifacts/libsongcore.a
ART_SO=build/artifacts/shared/libsongcore.so
[ -f "$ART_A" ] || { echo "FATAL: $ART_A missing" >&2; exit 1; }

# witness: did the flag reach the replayed FFmpeg compile units?
# compile_commands.json records the exact compile command of every unit.
xmake project -k compile_commands >/dev/null 2>&1
WIT=$EV/fix-witness.txt
{
  echo "compile_commands_entries=$(python3 -c "import json;print(len(json.load(open('compile_commands.json'))))" 2>/dev/null || echo 0)"
  echo "ffile_prefix_map_entries=$(grep -c -- '-ffile-prefix-map' compile_commands.json 2>/dev/null || true)"
  echo "fmacro_prefix_map_entries=$(grep -c -- '-fmacro-prefix-map' compile_commands.json 2>/dev/null || true)"
  echo "ffmpeg_src_entries=$(grep -c 'ffmpeg-src' compile_commands.json 2>/dev/null || true)"
  echo "sample_flag_entries:"
  grep -m 2 -- 'prefix-map' compile_commands.json 2>/dev/null | cut -c1-400 || true
} > "$WIT"

# ---------------------------------------------------------------- stage verify
STAGE=verify
sha256sum "$ART_A" $([ "$TARGET" = linux ] && echo "$ART_SO" || true) \
  "$MAN" "$PIN" "$PROFILE" "$RECIPE" > "$EV/artifacts-sha256.txt"

if [ "$TARGET" = linux ]; then
  [ -f "$ART_SO" ] || { echo "FATAL: $ART_SO missing" >&2; exit 1; }
  file "$ART_A" "$ART_SO" > "$EV/elf-identity-file.txt"
  grep -q "current ar archive" "$EV/elf-identity-file.txt" || { echo "FATAL: .a not ar" >&2; exit 1; }
  grep -q "ELF 64-bit LSB shared object, x86-64" "$EV/elf-identity-file.txt" || { echo "FATAL: .so not ELF64" >&2; exit 1; }
  MEMBERS=$(ar t "$ART_A" | wc -l)
  ELF64_MEMBERS=$(objdump -f "$ART_A" | grep -c "file format elf64-x86-64")
  [ "$MEMBERS" -gt 0 ] && [ "$MEMBERS" = "$ELF64_MEMBERS" ] || { echo "FATAL: member formats $ELF64_MEMBERS/$MEMBERS" >&2; exit 1; }
  objdump -f "$ART_A" > "$EV/objdump-f-a.txt"
  readelf -d "$ART_SO" | grep NEEDED | awk '{print $5}' | tr -d '[]' > "$EV/needed-libs.txt"
  if grep -qE '^libav' "$EV/needed-libs.txt"; then echo "FATAL: shared NEEDs libav" >&2; exit 1; fi
  grep 'SONGCORE_API' include/songcore.h | grep -oE '\bsong[a-z0-9_]*\(' | tr -d '(' | sort -u > "$EV/abi-expected.txt"
  nm -D --defined-only "$ART_SO" | awk '$2=="T"||$2=="W"{print $3}' | grep '^song' | sort -u > "$EV/abi-exported-so.txt"
  nm "$ART_A" | awk '$2=="T"||$2=="A"{print $3}' | grep '^song' | sort -u > "$EV/abi-exported-a.txt"
  diff "$EV/abi-expected.txt" "$EV/abi-exported-so.txt" > "$EV/abi-diff-so.txt" || { echo "FATAL: .so ABI set" >&2; exit 1; }
  diff "$EV/abi-expected.txt" "$EV/abi-exported-a.txt" > "$EV/abi-diff-a.txt" || { echo "FATAL: .a ABI set" >&2; exit 1; }
  ABI_N=$(wc -l < "$EV/abi-expected.txt")
else
  file "$ART_A" > "$EV/coff-identity-file.txt"
  grep -q "current ar archive" "$EV/coff-identity-file.txt" || { echo "FATAL: .a not ar" >&2; exit 1; }
  MEMBERS=$(x86_64-w64-mingw32-ar t "$ART_A" | wc -l)
  PE_MEMBERS=$(x86_64-w64-mingw32-objdump -f "$ART_A" | grep -c "file format pe-x86-64")
  [ "$MEMBERS" -gt 0 ] && [ "$MEMBERS" = "$PE_MEMBERS" ] || { echo "FATAL: member formats $PE_MEMBERS/$MEMBERS" >&2; exit 1; }
  x86_64-w64-mingw32-objdump -f "$ART_A" > "$EV/objdump-f-a.txt"
  grep 'SONGCORE_API' include/songcore.h | grep -oE '\bsong[a-z0-9_]*\(' | tr -d '(' | sort -u > "$EV/abi-expected.txt"
  x86_64-w64-mingw32-nm "$ART_A" | awk '$2=="T"||$2=="A"{print $3}' | grep '^song' | sort -u > "$EV/abi-exported-a.txt"
  diff "$EV/abi-expected.txt" "$EV/abi-exported-a.txt" > "$EV/abi-diff-a.txt" || { echo "FATAL: .a ABI set" >&2; exit 1; }
  ABI_N=$(wc -l < "$EV/abi-expected.txt")
fi
fact static_members "$MEMBERS"
fact abi_symbols "$ABI_N"

# leak census over the shipped artifacts
python3 - "$TARGET" "$ROOT" > "$EV/leak-census.txt" <<'PY'
import sys
target, root = sys.argv[1], sys.argv[2]
paths = ["build/artifacts/libsongcore.a"]
if target == "linux":
    paths.append("build/artifacts/shared/libsongcore.so")
pats = [(root + "/src").encode(), b"/home/hoo", b"C:\\", b"/mnt/"]
for p in paths:
    body = open(p, "rb").read()
    counts = {pat.decode("latin1"): body.count(pat) for pat in pats}
    print(p, counts)
PY
# ---------------------------------------------------------------- summary
STAGE=summary
python3 - "$TARGET" "$ROOT" "$STAMP" > "$EV/summary.json" <<'PY'
import json, os, sys
target, root, stamp = sys.argv[1], sys.argv[2], sys.argv[3]
ev = os.path.join(root, "receipts", stamp)
facts = {}
for line in open(os.path.join(ev, "facts.env")):
    k, _, v = line.strip().partition("=")
    facts[k] = v
sha = {}
for line in open(os.path.join(ev, "artifacts-sha256.txt")):
    h, p = line.split()
    sha[p] = h
def art(rel):
    full = os.path.join(root, "src", "native", rel)
    return {"path": full, "bytes": os.path.getsize(full), "sha256": sha.get(rel)}
out = {
    "entry": "b329-build.sh", "campaign": "songcore-172 #329 Q2",
    "target": target, "root": root, "attempt_stamp": stamp,
    "fixmode": facts["fixmode"], "mapto": facts["mapto"],
    "clone": facts["clone"], "closure": facts["closure"],
    "head": facts["head"], "tree": facts["tree"],
    "toolchain": {"uname": facts["uname"], "gcc": facts["gcc"]},
    "artifacts": {"static": art("build/artifacts/libsongcore.a"),
                  **({"shared": art("build/artifacts/shared/libsongcore.so")} if target == "linux" else {})},
    "static_members": int(facts["static_members"]),
    "abi_symbols": int(facts["abi_symbols"]),
    "witness": open(os.path.join(ev, "fix-witness.txt")).read() if os.path.exists(os.path.join(ev, "fix-witness.txt")) else "",
    "checks": {"build": "PASS", "member_formats": "PASS", "abi_export_set": "PASS"},
    "verdict": "PASS",
}
print(json.dumps(out, indent=2))
PY
cp "$EV/summary.json" /tmp/b329-last-summary.json
log "stage6: PASS — receipts in $EV"
fact verdict PASS
cat "$EV/summary.json"
