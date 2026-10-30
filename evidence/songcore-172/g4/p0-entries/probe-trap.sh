#!/usr/bin/env bash
# Probe: does an ERR trap's exit status propagate? (bash semantics for
# ops/build-linux.sh's on_err handler.)
set -Eeuo pipefail
mode="$1"
case "$mode" in
  fn)
    on_err(){ local rc=$?; printf '{"exit":%d}\n' "$rc" >&2; exit "$rc"; }
    trap on_err ERR
    cp /nope/x /tmp/y
    ;;
  inline)
    trap 'printf "inline_rc=%d\n" $? >&2; exit $?' ERR
    cp /nope/x /tmp/y
    ;;
  fn_noexit)
    on_err(){ local rc=$?; printf '{"exit":%d}\n' "$rc" >&2; }
    trap on_err ERR
    cp /nope/x /tmp/y
    ;;
esac
echo NOT_REACHED
