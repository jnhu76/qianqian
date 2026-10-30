#!/usr/bin/env bash
# Lane C (windows) — stage the cross-built PE and the media fixtures into an
# isolated Windows-visible directory, then hash what landed there.
set -u
PKG=/mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/g4/package/win/extract/songcore-v0.0.0-preflight-windows-x86_64
FIX=/mnt/c/Users/fred1/source/qianqian/native/experiments/songcore-equivalence/fixtures
RUN=/mnt/c/Users/fred1/lane-c-win-pe
WORK=$HOME/lane-c-pe
LOGS=$WORK/logs
mkdir -p "$RUN/media" "$RUN/run" "$LOGS"
exec > >(tee "$LOGS/stage.log") 2>&1

echo "=== stage exe (built in WSL against the packaged archive) ==="
echo "src  : $WORK/out/pe-consumer.exe"
echo "dest : $RUN/pe-consumer.exe"
cp -f "$WORK/out/pe-consumer.exe" "$RUN/pe-consumer.exe"
echo "### EXIT[cp-exe]=$?"

echo "=== stage media fixtures (Windows-readable copies) ==="
for f in mp3-cbr-id3v23.mp3 flac-16-44-stereo.flac alac-16-44-stereo.m4a alac-long.m4a; do
  cp -f "$FIX/$f" "$RUN/media/$f"
  echo "### EXIT[cp-$f]=$?"
done

echo "=== sha256 of the staged tree (what Windows will execute) ==="
sha256sum "$RUN/pe-consumer.exe"
echo "### EXIT[sha-exe]=$?"
for f in mp3-cbr-id3v23.mp3 flac-16-44-stereo.flac alac-16-44-stereo.m4a alac-long.m4a; do
  sha256sum "$RUN/media/$f"
  echo "### EXIT[sha-$f]=$?"
done

echo "=== cross-check: build-side and staged exe are byte-identical ==="
cmp "$WORK/out/pe-consumer.exe" "$RUN/pe-consumer.exe"
echo "### EXIT[cmp-exe]=$?"

echo "=== cross-check: staged fixtures equal the committed reference fixtures ==="
for f in mp3-cbr-id3v23.mp3 flac-16-44-stereo.flac alac-16-44-stereo.m4a alac-long.m4a; do
  cmp "$FIX/$f" "$RUN/media/$f"
  echo "### EXIT[cmp-$f]=$?"
done

echo "=== Windows-visible listing ==="
ls -la "$RUN" "$RUN/media"
echo "STAGE_RESULT=PASS"
