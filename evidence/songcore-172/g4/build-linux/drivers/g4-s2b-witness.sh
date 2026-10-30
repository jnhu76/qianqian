#!/usr/bin/env bash
# g4-s2b-witness.sh — G4 linux-x86_64 lane, stage 2b: corrected compile
# database export (the `-o` spelling used first is invalid for xmake 2.9.7;
# `xmake project -k compile_commands <outdir>` takes the directory positionally).
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
mkdir -p "$EV/compile-commands"
set +e
xmake project -k compile_commands "$EV/compile-commands" >"$EV/xmake-compile-commands-retry.log" 2>&1
RC=$?
set -e
rec compile-commands-retry "$RC" xmake project -k compile_commands "$EV/compile-commands"
[ $RC -eq 0 ] || { echo "FATAL: compile_commands retry exit $RC" >&2; tail -15 "$EV/xmake-compile-commands-retry.log" >&2; exit 1; }

ls -l "$NA/build/artifacts/libsongcore.a" "$NA/build/artifacts/shared/libsongcore.so"
{
  echo "build_ok=1"
  echo "build_end_utc=$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "artifacts_present_at_end=$(ls "$NA/build/artifacts/libsongcore.a" "$NA/build/artifacts/shared/libsongcore.so" | wc -l)"
} >> "$EV/facts.env"
echo "stage2b: PASS"
