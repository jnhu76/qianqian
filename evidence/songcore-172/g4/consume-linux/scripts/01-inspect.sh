#!/usr/bin/env bash
# Lane C (linux) consumer verification - stage 01: inspect the unpacked package tree.
set -u
T=/home/hoo/g4-172/laneB-stage/linux/songcore-v0.0.0-preflight-linux-x86_64
W=/mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/g4/package/linux/extract/songcore-v0.0.0-preflight-linux-x86_64

echo "=== WSL-NATIVE TREE: $T ==="
echo "--- file list + sha256 (sorted) ---"
find "$T" -type f -print0 | sort -z | xargs -0 sha256sum
echo "--- file count ---"
find "$T" -type f | wc -l
echo
echo "--- tree -F ---"
find "$T" -print | sed "s|$T|.|" | sort
echo
echo "=== WINDOWS-VISIBLE MIRROR: $W ==="
if [ -d "$W" ]; then
  find "$W" -type f -print0 | sort -z | xargs -0 sha256sum
  echo "--- diff -r (empty == identical) ---"
  diff -r "$T" "$W" && echo "DIFF_EMPTY_EXIT0"
  echo "diff_exit=$?"
else
  echo "MIRROR_MISSING"
fi
