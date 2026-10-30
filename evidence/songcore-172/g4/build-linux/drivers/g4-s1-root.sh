#!/usr/bin/env bash
# g4-s1-root.sh — G4 linux-x86_64 lane, stage 1: independent root.
#
# Builds $HOME/g4-172/linux-root:
#   src/   fresh --no-local clone of the WSL repository at the pinned sha
#   src/native/build/ffmpeg-src + ffmpeg-xmake
#          copies of the already-imported linux FFmpeg closure (no download)
# then revalidates that closure against ffmpeg/pin.json and the
# linux-x86_64 recipe. Fail-closed: any mandatory identity failure exits 1.
set -Eeuo pipefail
export PATH="$HOME/.local/bin:$PATH"
export LC_ALL=C

ROOT=${G4_ROOT:-$HOME/g4-172/linux-root}
SRC=$ROOT/src
EV=$ROOT/receipts/g4-linux-build
PIN_SHA=69ba1ce05a2772f5aa077c6ece3365c40f6e0500
PIN_TREE=30d8c823b90845947341d978f77196dd8778a707
CLONE_FROM=file:///home/hoo/Source/qianqian
CLOSURE=/home/hoo/qianqian-172-linux/native/build
ZIP=/home/hoo/q172d2-verify/win/native/build/downloads/ffmpeg-n9.0.1.zip
DRV=$(cd "$(dirname "$0")" && pwd)
NA=$SRC/native

mkdir -p "$ROOT" "$EV"
INV=$EV/invocations.tsv
: > "$INV"
rec() { local st=$1 rc=$2; shift 2
  { printf '%s\t%s\t' "$st" "$rc"; printf '%q ' "$@"; printf '\n'; } >> "$INV"; }

# ---------------------------------------------------------------- lock
LOCK=$ROOT/.g4-lock
if ! mkdir "$LOCK" 2>/dev/null; then
  HOLDER=$(cat "$LOCK/pid" 2>/dev/null || true)
  if [ -n "$HOLDER" ] && ! kill -0 "$HOLDER" 2>/dev/null; then
    rm -rf "$LOCK"; mkdir "$LOCK"
  else
    echo "FATAL: another g4 driver holds $ROOT (pid ${HOLDER:-unknown})" >&2; exit 1
  fi
fi
echo $$ > "$LOCK/pid"
trap 'rm -rf "$LOCK"' EXIT

# ---------------------------------------------------------------- clone
if [ -d "$SRC/.git" ] && [ "$(git -C "$SRC" rev-parse HEAD 2>/dev/null)" = "$PIN_SHA" ]; then
  echo "stage1: reusing clone at $PIN_SHA"
  CLONE_MODE=reuse
else
  rm -rf "$SRC"
  set +e
  git clone --no-local "$CLONE_FROM" "$SRC" >"$EV/clone.log" 2>&1; RC=$?
  set -e
  rec clone "$RC" git clone --no-local "$CLONE_FROM" "$SRC"
  if [ $RC -ne 0 ]; then echo "FATAL: clone exit $RC" >&2; tail -20 "$EV/clone.log" >&2; exit 1; fi
  set +e
  git -C "$SRC" checkout --quiet --detach "$PIN_SHA" >>"$EV/clone.log" 2>&1; RC=$?
  set -e
  rec checkout "$RC" git -C "$SRC" checkout --quiet --detach "$PIN_SHA"
  if [ $RC -ne 0 ]; then echo "FATAL: checkout exit $RC" >&2; tail -20 "$EV/clone.log" >&2; exit 1; fi
  CLONE_MODE=fresh
fi

HEAD=$(git -C "$SRC" rev-parse HEAD)
TREE=$(git -C "$SRC" rev-parse 'HEAD^{tree}')
[ "$HEAD" = "$PIN_SHA" ] || { echo "FATAL: head $HEAD != $PIN_SHA" >&2; exit 1; }
[ "$TREE" = "$PIN_TREE" ] || { echo "FATAL: tree $TREE != $PIN_TREE" >&2; exit 1; }
if ! git -C "$SRC" diff-index --quiet HEAD --; then
  echo "FATAL: clone has tracked modifications" >&2; exit 1
fi
echo "stage1: head=$HEAD tree=$TREE tracked-tree=clean"

# ---------------------------------------------------------------- closure copy
CLOSURE_MODE=reuse
if [ ! -f "$NA/build/ffmpeg-xmake/manifest.json" ] || [ ! -d "$NA/build/ffmpeg-src" ]; then
  CLOSURE_MODE=copy
  mkdir -p "$NA/build"
  rm -rf "$NA/build/ffmpeg-src" "$NA/build/ffmpeg-xmake"
  set +e
  cp -a "$CLOSURE/ffmpeg-src" "$NA/build/" >"$EV/cp-ffmpeg-src.log" 2>&1; RC=$?
  set -e
  rec closure-copy-ffmpeg-src "$RC" cp -a "$CLOSURE/ffmpeg-src" "$NA/build/"
  if [ $RC -ne 0 ]; then echo "FATAL: cp ffmpeg-src exit $RC" >&2; exit 1; fi
  set +e
  cp -a "$CLOSURE/ffmpeg-xmake" "$NA/build/" >"$EV/cp-ffmpeg-xmake.log" 2>&1; RC=$?
  set -e
  rec closure-copy-ffmpeg-xmake "$RC" cp -a "$CLOSURE/ffmpeg-xmake" "$NA/build/"
  if [ $RC -ne 0 ]; then echo "FATAL: cp ffmpeg-xmake exit $RC" >&2; exit 1; fi
  echo "stage1: imported closure copied from $CLOSURE (no download)"
else
  echo "stage1: reusing closure copy already inside $NA/build"
fi

# ---------------------------------------------------------------- revalidate
set +e
python3 "$DRV/g4-closure-validate.py" "$SRC" "$ZIP" \
  >"$EV/closure-validation.json" 2>"$EV/closure-validation.stderr"; RC=$?
set -e
rec closure-validate "$RC" python3 "$DRV/g4-closure-validate.py" "$SRC" "$ZIP"
if [ $RC -ne 0 ]; then
  echo "FATAL: closure revalidation exit $RC" >&2
  tail -40 "$EV/closure-validation.stderr" >&2
  python3 -c "import json,sys; d=json.load(open('$EV/closure-validation.json')); print('failed:', d.get('mandatory_failures'))" >&2 || true
  exit 1
fi
echo "stage1: closure revalidated against pin.json + linux-x86_64 recipe"

# ---------------------------------------------------------------- facts
XMAKE_LOCAL_CACHE=absent; [ -d "$NA/build/xmake" ] && XMAKE_LOCAL_CACHE=present
{
  echo "root=$ROOT"
  echo "src=$SRC"
  echo "source_pin_sha=$PIN_SHA"
  echo "source_pin_tree=$PIN_TREE"
  echo "head=$HEAD"
  echo "tree=$TREE"
  echo "clone_mode=$CLONE_MODE"
  echo "clone_from=$CLONE_FROM"
  echo "closure_mode=$CLOSURE_MODE"
  echo "closure_source=$CLOSURE"
  echo "uname=$(uname -sr)"
  echo "nproc=$(nproc)"
  echo "gcc=$(gcc --version | head -1)"
  echo "xmake=$(xmake --version 2>/dev/null | sed 's/\x1b\[[0-9;]*m//g' | head -1)"
  echo "python3=$(python3 --version)"
  echo "objdump=$(objdump --version | head -1)"
  echo "readelf=$(readelf --version | head -1)"
  echo "ar=$(ar --version | head -1)"
  echo "nm=$(nm --version | head -1)"
  echo "closure_manifest_sha256=$(sha256sum "$NA/build/ffmpeg-xmake/manifest.json" | awk '{print $1}')"
  echo "pin_json_sha256=$(sha256sum "$NA/ffmpeg/pin.json" | awk '{print $1}')"
  echo "recipe_sha256=$(sha256sum "$NA/ffmpeg/targets/linux-x86_64.json" | awk '{print $1}')"
  echo "profile_sha256=$(sha256sum "$NA/ffmpeg/profiles/codec-base.json" | awk '{print $1}')"
  echo "zip_path=$ZIP"
  echo "zip_sha256=$(sha256sum "$ZIP" | awk '{print $1}')"
  echo "xmake_local_cache_prebuild=$XMAKE_LOCAL_CACHE"
} > "$EV/facts.env"

echo "stage1: PASS"
sed -n '1,80p' "$EV/facts.env"
