#!/usr/bin/env bash
# Lane E: run the built consumers on older-glibc distros (real container runs).
set -u
export LC_ALL=C
STAGE=/mnt/c/Users/fred1/lane-e-container
MEDIA=/home/hoo/Source/qianqian/native/experiments/songcore-equivalence/fixtures/mp3-cbr-id3v23.mp3
EVD=/mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/g4/lane-e-runtime-deps
RAW="$EVD/raw"
LOG="$RAW/container-runs-exitcodes.txt"
: > "$LOG"

logrun() {
  local desc="$1"; local out="$2"; shift 2
  "$@" > "$out" 2>&1
  local rc=$?
  printf '%s | argv: %s | exit=%s | output=%s\n' "$desc" "$*" "$rc" "$out" >> "$LOG"
  return $rc
}

cp "$MEDIA" "$STAGE/media.mp3"
sha256sum "$STAGE/media.mp3" > "$RAW/container-media-sha256.txt" 2>&1

# attempt to obtain an older LTS (glibc 2.31) if network allows
logrun "docker pull ubuntu:20.04" "$RAW/docker-pull-ubuntu2004.txt" timeout 240 docker pull ubuntu:20.04

run_image() {
  local img="$1"; local tag="$2"
  logrun "os-release $tag" "$RAW/container-$tag-os-release.txt" \
    docker run --rm --entrypoint /bin/cat "$img" /etc/os-release
  logrun "ldd --version $tag" "$RAW/container-$tag-ldd-version.txt" \
    docker run --rm --entrypoint ldd "$img" --version
  logrun "shared consumer $tag (mp3 open->decode)" "$RAW/container-$tag-external-shared.txt" \
    docker run --rm -v "$STAGE:/work" -w /work "$img" ./external-consumer-shared /work/media.mp3
  logrun "static-form consumer $tag (minimal-libs)" "$RAW/container-$tag-minimal-static.txt" \
    docker run --rm -v "$STAGE:/work" -w /work "$img" ./minimal-libs
  logrun "sizeof probe $tag" "$RAW/container-$tag-sizeof-shared.txt" \
    docker run --rm -v "$STAGE:/work" -w /work "$img" ./abi-sizeof-probe-shared
}

run_image postgres:18.4-bookworm bookworm
run_image mcr.microsoft.com/playwright:v1.61.0-noble noble
if grep -q 'Status: Downloaded\|Image is up to date\|already exists' "$RAW/docker-pull-ubuntu2004.txt" 2>/dev/null || docker image inspect ubuntu:20.04 >/dev/null 2>&1; then
  run_image ubuntu:20.04 focal
else
  printf 'ubuntu:20.04 NOT AVAILABLE | argv: docker pull ubuntu:20.04 | exit=%s | output=%s\n' "$?" "$RAW/docker-pull-ubuntu2004.txt" >> "$LOG"
fi

# alpine is musl: informative only, not a glibc data point
logrun "alpine (musl) shared consumer" "$RAW/container-alpine-external-shared.txt" \
  docker run --rm -v "$STAGE:/work" -w /work alpine:latest ./external-consumer-shared /work/media.mp3

echo "done"
