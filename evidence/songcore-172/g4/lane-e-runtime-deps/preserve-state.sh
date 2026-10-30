#!/usr/bin/env bash
# Lane E: pin identity of preserved copies after the build root disappeared.
set -u
export LC_ALL=C
EVD=/mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/g4/lane-e-runtime-deps
RAW="$EVD/raw"
W="$HOME/lane-e-runtime/work"
STAGE=/mnt/c/Users/fred1/lane-e-container
{
  echo "== preserved copies (made during measurement, before build root removal) =="
  sha256sum "$W/shared/libsongcore.so" "$STAGE/libsongcore.so" "$W/src/songcore.h"
  echo
  echo "expected from task input:"
  echo "085425e590b0d6d810192404162cef6f1f1f880f769e4afd12e4af5306078e98  libsongcore.so"
  echo "0d38f4b5a8bd812ba262ca2dc26c32f2d1a596511222d930e685db25d7361308  libsongcore.a (not copied; measured in place)"
  echo
  echo "== build root state at end of lane =="
  ls -la /home/hoo/g4-172/ 2>&1
  echo
  ls -la /home/hoo/g4-172/linux-root 2>&1
  echo
  echo "== preserved work dir listing =="
  ls -lR "$W" | head -60
} > "$RAW/post-measurement-state.txt" 2>&1
cat "$RAW/post-measurement-state.txt"
