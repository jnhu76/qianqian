#!/usr/bin/env bash
# g4-s2-build.sh — G4 linux-x86_64 lane, stage 2: documented xmake build.
#
# Runs the documented route (native/README.md "Build (Linux x86_64)" /
# ops/build-linux.sh stage3) inside the independent root:
#   xmake f -p linux -a x86_64 -m release -y
#   xmake build songcore
#   xmake build songcore_static
#   xmake build songcore_shared
# Every invocation is appended to receipts/g4-linux-build/invocations.tsv with
# its real exit code. Fail-closed on any nonzero exit.
set -Eeuo pipefail
export PATH="$HOME/.local/bin:$PATH"
export LC_ALL=C

ROOT=${G4_ROOT:-$HOME/g4-172/linux-root}
SRC=$ROOT/src
EV=$ROOT/receipts/g4-linux-build
NA=$SRC/native
INV=$EV/invocations.tsv
touch "$INV"
rec() { local st=$1 rc=$2; shift 2
  { printf '%s\t%s\t' "$st" "$rc"; printf '%q ' "$@"; printf '\n'; } >> "$INV"; }

cd "$NA"
[ -f build/ffmpeg-xmake/manifest.json ] || { echo "FATAL: closure missing; run g4-s1-root.sh" >&2; exit 1; }
[ -f ffmpeg/targets/linux-x86_64.json ] || { echo "FATAL: recipe missing" >&2; exit 1; }

XMAKE_LOCAL_CACHE=absent; [ -d build/xmake ] && XMAKE_LOCAL_CACHE=present
{
  echo "xmake_local_cache_at_build_start=$XMAKE_LOCAL_CACHE"
  echo "build_start_utc=$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "uname=$(uname -sr)"
  echo "gcc=$(gcc --version | head -1)"
  echo "xmake=$(xmake --version 2>/dev/null | sed 's/\x1b\[[0-9;]*m//g' | head -1)"
  echo "nproc=$(nproc)"
  echo "cwd=$NA"
} >> "$EV/facts.env"

run_xmake() { # run_xmake <stage> <log> <argv...>
  local st=$1 log=$2; shift 2
  local start rc=0
  start=$(date -u +%Y-%m-%dT%H:%M:%SZ)
  echo "===== $st : $* : start $start =====" >>"$log"
  "$@" >>"$log" 2>&1 || rc=$?
  echo "===== $st : exit $rc : end $(date -u +%Y-%m-%dT%H:%M:%SZ) =====" >>"$log"
  rec "$st" "$rc" "$@"
  if [ $rc -ne 0 ]; then
    echo "FATAL: $st exit $rc; tail of $log:" >&2
    tail -25 "$log" >&2
    exit "$rc"
  fi
  echo "stage2: $st exit 0"
}

# fresh-build precondition: no artifact from an earlier run may be present
if [ -f build/artifacts/libsongcore.a ] || [ -f build/artifacts/shared/libsongcore.so ]; then
  echo "FATAL: artifacts already exist before this build; root is not fresh" >&2
  ls -l build/artifacts/libsongcore.a build/artifacts/shared/libsongcore.so 2>/dev/null >&2 || true
  exit 1
fi

: > "$EV/xmake-config.log"
run_xmake configure "$EV/xmake-config.log" xmake f -p linux -a x86_64 -m release -y

: > "$EV/xmake-build.log"
run_xmake build-songcore "$EV/xmake-build.log" xmake build songcore

: > "$EV/xmake-build-static-explicit.log"
run_xmake build-static-explicit "$EV/xmake-build-static-explicit.log" xmake build songcore_static

: > "$EV/xmake-build-shared-explicit.log"
run_xmake build-shared-explicit "$EV/xmake-build-shared-explicit.log" xmake build songcore_shared

# flag-delivery witness: compile_commands.json records the exact compile line
# of every replayed FFmpeg unit (written outside the source tree).
mkdir -p "$EV/compile-commands"
run_xmake compile-commands "$EV/xmake-compile-commands.log" \
  xmake project -k compile_commands -o "$EV/compile-commands"

{
  echo "build_end_utc=$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "build_ok=1"
} >> "$EV/facts.env"
echo "stage2: PASS"
