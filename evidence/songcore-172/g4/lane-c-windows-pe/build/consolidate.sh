#!/usr/bin/env bash
# Lane C (windows) — consolidate build + run evidence into the lane directory
# and emit a sha256 manifest over every evidence file.
set -u
LANE=/mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/g4/lane-c-windows-pe
RUN=/mnt/c/Users/fred1/lane-c-win-pe
WLOGS=/home/hoo/lane-c-pe/logs
mkdir -p "$LANE/build/logs" "$LANE/run"
exec > >(tee "$LANE/build/consolidate.log") 2>&1

echo "=== build logs from the WSL build host ==="
for f in build.log stage.log link-wo-bcrypt.log link-with-bcrypt.log; do
  cp -f "$WLOGS/$f" "$LANE/build/logs/$f"; echo "EXIT[cp-$f]=$?"
done

echo "=== the exact PE that was executed ==="
cp -f "$RUN/pe-consumer.exe" "$LANE/build/pe-consumer.exe"; echo "EXIT[cp-exe]=$?"
sha256sum "$LANE/build/pe-consumer.exe" "$RUN/pe-consumer.exe"

echo "=== run artifacts from the Windows host ==="
for f in "$RUN"/run/*; do
  cp -f "$f" "$LANE/run/$(basename "$f")"; echo "EXIT[cp-$(basename "$f")]=$?"
done

echo "=== byte-level identity between the isolated Windows run dir and the lane copy ==="
for f in "$RUN"/run/*; do
  cmp "$f" "$LANE/run/$(basename "$f")" || echo "MISMATCH: $(basename "$f")"
done
echo "EXIT[cmp-all]=$?"

echo "=== sha256 manifest over the lane evidence ==="
( cd "$LANE" && find . -type f ! -name 'sha256-manifest.txt' ! -name 'stage.json' \
    -exec sha256sum {} \; | sed 's#\./##' | sort -k2 ) > "$LANE/build/sha256-manifest.txt"
wc -l "$LANE/build/sha256-manifest.txt"
cat "$LANE/build/sha256-manifest.txt"
echo "CONSOLIDATE_DONE"
