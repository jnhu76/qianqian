#!/usr/bin/env bash
# Real-condition replay of ops/build-linux.sh: default HOME (DST is the
# campaign clone at the pinned branch/commit). Snapshot the FFmpeg closure
# identity before/after so the audit can show the closure was only reused.
set -u
export PATH="$HOME/.local/bin:$PATH"
DST=/home/hoo/qianqian-172-linux
SNAP=/home/hoo/q172-g4-audit/closure-snapshot-before.txt
snap() {
  {
    echo "head=$(git -C "$DST" rev-parse HEAD)"
    echo "branch=$(git -C "$DST" rev-parse --abbrev-ref HEAD)"
    echo "manifest_sha256=$(sha256sum "$DST/native/build/ffmpeg-xmake/manifest.json" | awk '{print $1}')"
    echo "verified_marker=$(cat "$DST/native/build/ffmpeg-src/.qianqian-verified")"
    echo "oracle_config_mak_sha256=$(sha256sum "$DST/native/build/ffmpeg-xmake/oracle/ffbuild/config.mak" | awk '{print $1}')"
    echo "artifact_a_before=$(sha256sum "$DST/native/build/artifacts/libsongcore.a" 2>/dev/null | awk '{print $1}')"
    echo "artifact_so_before=$(sha256sum "$DST/native/build/artifacts/shared/libsongcore.so" 2>/dev/null | awk '{print $1}')"
  } > "$1"
}
snap "$SNAP"
echo "=== BEFORE ==="
cat "$SNAP"
echo "=== RUN ==="
bash /mnt/c/Users/fred1/source/qianqian/ops/build-linux.sh
rc=$?
echo "BUILD_LINUX_EXIT=$rc"
echo "=== AFTER ==="
snap /home/hoo/q172-g4-audit/closure-snapshot-after.txt
cat /home/hoo/q172-g4-audit/closure-snapshot-after.txt
exit 0
