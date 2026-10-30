#!/usr/bin/env bash
set -Eeuo pipefail
probe="$1"
on_err() { local rc=$?; printf '{"exit":%d}\n' "$rc"; exit "$rc"; }
trap on_err ERR

f() { cp /nope/x /tmp/y; }
g() { if cp /nope/x /tmp/y 2>/dev/null; then echo a; else cp /nope/x /tmp/z; fi; }

case "$probe" in
  plain_fn) f; echo AFTER ;;
  fn_if)    g; echo AFTER ;;
  fn_called_in_else)
    if false; then :; else g; echo AFTER2; fi ;;
esac
echo SCRIPT_END
