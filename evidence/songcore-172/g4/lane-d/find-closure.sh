#!/usr/bin/env bash
set -u
echo "== native/build listing =="
ls -la /home/hoo/Source/qianqian/native/build/
echo "== candidate closure dirs =="
for d in /home/hoo/qianqian-172-linux/native/build/ffmpeg-xmake \
         /home/hoo/q172d2-verify/closure \
         /home/hoo/q172rc/closure; do
  echo "--- $d ---"
  if [ -e "$d" ]; then
    ls -la "$d" | head -12
    find "$d" -maxdepth 2 -name 'manifest.json' -printf '%p %s\n' 2>/dev/null | head -5
  else
    echo "MISSING"
  fi
done
