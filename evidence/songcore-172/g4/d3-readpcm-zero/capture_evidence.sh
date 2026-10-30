#!/usr/bin/env bash
# Capture the D3 probe evidence files into the evidence directory and record
# the exact toolchain identity used for the run.
set -u
OUT=/home/hoo/d3probe
EVID=/mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/g4/d3-readpcm-zero
mkdir -p "$EVID/logs"

for f in run-force.log run-clean.log build-force.log build-clean.log \
         configure-clean.log cc-force.log song_read_pcm-force.dis \
         song_read_pcm-clean.dis; do
    if [ -f "$OUT/$f" ]; then
        cp -f "$OUT/$f" "$EVID/logs/$f"
        echo "copied $f exit=$?"
    else
        echo "MISSING $f"
    fi
done

{
  echo "== toolchain =="
  gcc --version | head -n 1
  export PATH=$HOME/.local/bin:$PATH
  xmake --version | head -n 2
  uname -srmo
  echo "== artifact =="
  sha256sum /home/hoo/d3probe/lib/libsongcore.so
  ls -l --time-style=+%s /home/hoo/d3probe/lib/libsongcore.so
} > "$EVID/logs/toolchain.txt" 2>&1
cat "$EVID/logs/toolchain.txt"

echo "== evidence hashes =="
sha256sum "$EVID/logs/"* "$EVID/readpcm_zero_probe.c" "$EVID/build_and_probe.sh" \
          "$EVID/clean_rebuild_and_probe.sh" "$EVID/force_rebuild_and_probe.sh"
