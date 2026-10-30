#!/usr/bin/env bash
# Lane C (linux) - stage 08: final inventory.
#   * package self-checksum verification (metadata/checksums.txt)
#   * copy the latest run outputs into the Windows-visible evidence lane
#   * sha256 inventory of every evidence file
set -u
PKG=/home/hoo/g4-172/laneB-stage/linux/songcore-v0.0.0-preflight-linux-x86_64
W=/home/hoo/g4-172/consume-linux
EVID=/mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/g4/consume-linux
LOG=$EVID/logs/stage08-inventory.log
mkdir -p "$EVID/out/static" "$EVID/out/shared" "$EVID/fixtures-bad"
: > "$LOG"

run() {
    local log=$1; shift
    printf '\n$ %s\n' "$*" >> "$log"
    "$@" >> "$log" 2>&1
    local rc=$?
    printf '[exit] %d\n' "$rc" >> "$log"
    echo "EXIT=$rc : $*"
    return $rc
}

echo "=== package self-checksum (manifest/checksums.txt over the unpacked tree) ==="
printf '\n$ (cd %s && sha256sum -c metadata/checksums.txt)\n' "$PKG" >> "$LOG"
( cd "$PKG" && sha256sum -c metadata/checksums.txt ) >> "$LOG" 2>&1
printf '[exit] %d\n' $? >> "$LOG"
( cd "$PKG" && sha256sum -c metadata/checksums.txt ) 2>&1 | sed 's/^/  /'

echo
echo "=== manifest-vs-tree file set (manifest files{} count vs on-disk) ==="
{
  echo "on-disk file count: $(find "$PKG" -type f | wc -l)"
  echo "manifest files{} entries: $(python3 -c 'import json,sys;print(len(json.load(open(sys.argv[1]))["files"]))' "$PKG/metadata/manifest.json")"
  echo "checksums.txt lines: $(wc -l < "$PKG/metadata/checksums.txt")"
} >> "$LOG"

echo
echo "=== copy latest outputs + bad fixtures into the evidence lane ==="
cp "$W"/out/static/*.jsonl "$W"/out/static/*.out "$EVID/out/static/" 2>/dev/null
cp "$W"/out/shared/*.jsonl "$W"/out/shared/*.out "$W"/out/shared/*.err "$EVID/out/shared/" 2>/dev/null
cp "$W"/fixtures/bad/* "$EVID/fixtures-bad/" 2>/dev/null
echo copied

echo
echo "=== evidence sha256 inventory ==="
{
  find "$EVID" -type f ! -name evidence-sha256.txt -print0 | sort -z | xargs -0 sha256sum
} > "$EVID/evidence-sha256.txt"
wc -l "$EVID/evidence-sha256.txt"
echo
echo "--- key identities ---"
{
  echo "package include/songcore.h : $(sha256sum "$PKG/include/songcore.h" | cut -d' ' -f1)"
  echo "package lib/libsongcore.a : $(sha256sum "$PKG/lib/libsongcore.a" | cut -d' ' -f1)"
  echo "package lib/shared/libsongcore.so : $(sha256sum "$PKG/lib/shared/libsongcore.so" | cut -d' ' -f1)"
  echo "workdir reference.json    : $(sha256sum "$W/reference.json" | cut -d' ' -f1)"
} | tee -a "$LOG"
