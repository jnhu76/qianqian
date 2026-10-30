#!/usr/bin/env bash
# Exact structural replica of ops/build-linux.sh lines 36-112 (trap +
# do_clone() called from the else-branch of an if), with all real work
# replaced by failing placeholders. Probes the observed exit status.
set -Eeuo pipefail
STAGE=clone
on_err() {
  local rc=$?
  printf '{"verdict":"FAIL","failed_stage":"%s","exit":%d}\n' "$STAGE" "$rc"
  exit "$rc"
}
trap on_err ERR

DST=/home/hoo/q172-trap-probe/qianqian-172-linux
EV=$DST/ops/build-evidence/stamp
mkdir -p "$EV"

reuse_clone() { return 1; }

do_clone() {
  echo "stage1: cloning fresh"
  rm -rf "$DST"
  if cp -f /nope/src "$DST/x.log" 2>/dev/null; then
    echo made
  else
    echo "fallback"
    cp -f "$DST/x.log" "$DST/y.log"
  fi
  echo "do_clone_end"
}

if reuse_clone; then
  echo reuse
else
  do_clone
  echo "after_do_clone"
fi
echo "SCRIPT_END"
