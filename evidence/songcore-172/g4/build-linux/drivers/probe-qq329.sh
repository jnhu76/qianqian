#!/usr/bin/env bash
# probe-qq329.sh — locate the #329 root artifacts and hash them (read-only).
set -uo pipefail
export LC_ALL=C
for r in /home/hoo/qq329/lin-r1 /home/hoo/qq329/linux-root-two /home/hoo/qq329/lin-l1-route; do
  echo "== $r =="
  ls -l "$r/src/native/build/artifacts/libsongcore.a" 2>&1
  ls -l "$r/src/native/build/artifacts/shared/libsongcore.so" 2>&1
  for f in "$r/src/native/build/artifacts/libsongcore.a" "$r/src/native/build/artifacts/shared/libsongcore.so"; do
    [ -f "$f" ] && sha256sum "$f"
  done
done
