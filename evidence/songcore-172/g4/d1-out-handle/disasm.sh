#!/usr/bin/env bash
# Persist the song_open disassembly for both release targets as evidence.
set -u
EV=/mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/g4/d1-out-handle
REPO=/home/hoo/Source/qianqian
WIN=/home/hoo/d1-win-src/native/build/artifacts/libsongcore.a
{
  echo "# linux-x86_64 libsongcore.a  sha256 $(sha256sum $REPO/native/build/artifacts/libsongcore.a | cut -d' ' -f1)"
  echo "# source SHA 69ba1ce05a2772f5aa077c6ece3365c40f6e0500"
  echo "# argv: objdump -d <lib> | awk '/<song_open>:/{f=1} f{print} f&&/^$/{exit}'"
  objdump -d "$REPO/native/build/artifacts/libsongcore.a" 2>/dev/null \
    | awk '/<song_open>:/{f=1} f{print} f&&/^$/{exit}'
} > "$EV/disasm-linux.txt"
{
  echo "# windows-x86_64 (mingw) libsongcore.a  sha256 $(sha256sum $WIN | cut -d' ' -f1)"
  echo "# built from: git archive 69ba1ce05a2772f5aa077c6ece3365c40f6e0500 | tar -x"
  echo "# argv: x86_64-w64-mingw32-objdump -d <lib> | awk '/<song_open>:/{f=1} f{print} f&&/^$/{exit}'"
  x86_64-w64-mingw32-objdump -d "$WIN" 2>/dev/null \
    | awk '/<song_open>:/{f=1} f{print} f&&/^$/{exit}'
} > "$EV/disasm-windows.txt"
grep -n "movq   \$0x0" "$EV/disasm-linux.txt" "$EV/disasm-windows.txt"
