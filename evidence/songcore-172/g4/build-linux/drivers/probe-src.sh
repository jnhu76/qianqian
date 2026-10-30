#!/usr/bin/env bash
set -uo pipefail
S=/home/hoo/qianqian-172-linux/native/build/ffmpeg-src
echo "symlinks:"
find "$S" -type l | head
echo "symlink_count=$(find "$S" -type l | wc -l)"
echo "file_count=$(find "$S" -type f | wc -l)"
echo "dir_count=$(find "$S" -type d | wc -l)"
echo "top_level:"
ls -a "$S" | head -8
