#!/usr/bin/env bash
# Lane C (linux) - stage 02: build the external consumers in an isolated dir,
# against the UNPACKED PACKAGE ONLY (include/ + lib/).
set -u

PKG=/home/hoo/g4-172/laneB-stage/linux/songcore-v0.0.0-preflight-linux-x86_64
W=/home/hoo/g4-172/consume-linux
EVID=/mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/g4/consume-linux
LOG=$EVID/logs
mkdir -p "$W"/src "$W"/bin "$W"/fixtures "$W"/out "$LOG"

run() {  # run <logfile> <cmd...> ; appends argv + exit code
    local log=$1; shift
    printf '\n$ %s\n' "$*" >> "$log"
    "$@" >> "$log" 2>&1
    local rc=$?
    printf '[exit] %d\n' "$rc" >> "$log"
    echo "EXIT=$rc : $*"
    return $rc
}

: > "$LOG/stage02-build.log"
{
  echo "=== stage02 build $(date -u +%Y-%m-%dT%H:%M:%SZ) ==="
  echo "PKG=$PKG"
  echo "W=$W"
  echo "gcc: $(gcc --version | head -1)"
  echo "uname: $(uname -a)"
} >> "$LOG/stage02-build.log"

echo "--- copy package example + our corpus consumer + frozen corpus into isolated dir ---"
cp "$PKG/metadata/external-consumer-example.c" "$W/src/external-consumer-example.c"
cp "$EVID/src/consume_corpus.c"                "$W/src/consume_corpus.c"
cp /mnt/c/Users/fred1/source/qianqian/native/experiments/songcore-equivalence/fixtures/* "$W/fixtures/"
cp /mnt/c/Users/fred1/source/qianqian/native/experiments/songcore-equivalence/reference.json "$W/reference.json"

echo "--- source / header / lib identity (must be package bytes) ---"
{
  echo "--- sha256 of consumer sources ---"
  sha256sum "$W/src/external-consumer-example.c" "$W/src/consume_corpus.c"
  echo "--- sha256 of package example (must equal copied one) ---"
  sha256sum "$PKG/metadata/external-consumer-example.c"
  echo "--- sha256 of header used by -I ---"
  sha256sum "$PKG/include/songcore.h"
  echo "--- sha256 of archives used by the link lines ---"
  sha256sum "$PKG/lib/libsongcore.a" "$PKG/lib/shared/libsongcore.so"
  echo "--- sha256 of fixtures ---"
  sha256sum "$W"/fixtures/*
  echo "--- reference.json ---"
  sha256sum "$W/reference.json"
} >> "$LOG/stage02-build.log"
echo "sources copied"

echo
echo "=== STATIC BUILD: package example (contract section 9 link line) ==="
run "$LOG/stage02-build.log" gcc -O2 -I"$PKG/include" "$W/src/external-consumer-example.c" \
    -L"$PKG/lib" -lsongcore -lm -lpthread -o "$W/bin/consumer-example-static"
rc1=$?

echo
echo "=== STATIC BUILD: corpus consumer (archive path literal) ==="
run "$LOG/stage02-build.log" gcc -O2 -I"$PKG/include" "$W/src/consume_corpus.c" \
    "$PKG/lib/libsongcore.a" -lm -lpthread -o "$W/bin/consume-corpus-static"
rc2=$?

echo
echo "=== SHARED BUILD: package example (-L package/lib/shared, no rpath) ==="
run "$LOG/stage02-build.log" gcc -O2 -I"$PKG/include" "$W/src/external-consumer-example.c" \
    -L"$PKG/lib/shared" -lsongcore -lm -lpthread -o "$W/bin/consumer-example-shared"
rc3=$?

echo
echo "=== SHARED BUILD: corpus consumer (-L package/lib/shared, no rpath) ==="
run "$LOG/stage02-build.log" gcc -O2 -I"$PKG/include" "$W/src/consume_corpus.c" \
    -L"$PKG/lib/shared" -lsongcore -lm -lpthread -o "$W/bin/consume-corpus-shared"
rc4=$?

echo
echo "=== SHARED BUILD: corpus consumer with absolute RPATH to the package .so ==="
run "$LOG/stage02-build.log" gcc -O2 -I"$PKG/include" "$W/src/consume_corpus.c" \
    -L"$PKG/lib/shared" -lsongcore -lm -lpthread \
    -Wl,-rpath,"$PKG/lib/shared" -o "$W/bin/consume-corpus-shared-rpath"
rc5=$?

echo
echo "=== ARTIFACT SHAPE ==="
{
  echo "--- bin listing ---"
  ls -l "$W/bin"
  echo "--- sha256 of built consumers ---"
  sha256sum "$W"/bin/*
  echo "--- file() ---"
  file "$W"/bin/*
  echo "--- ldd static (expect: NO libsongcore line) ---"
  ldd "$W/bin/consumer-example-static" || true
  ldd "$W/bin/consume-corpus-static" || true
  echo "--- readelf -d static bin: NEEDED ---"
  readelf -d "$W/bin/consume-corpus-static" | egrep 'NEEDED|RPATH|RUNPATH'
  echo "--- readelf -d shared bin: NEEDED/RPATH/RUNPATH ---"
  readelf -d "$W/bin/consume-corpus-shared" | egrep 'NEEDED|RPATH|RUNPATH'
  echo "--- readelf -d shared-rpath bin: NEEDED/RPATH/RUNPATH ---"
  readelf -d "$W/bin/consume-corpus-shared-rpath" | egrep 'NEEDED|RPATH|RUNPATH'
  echo "--- undef song* symbols in static bin (must be none) ---"
  nm -u "$W/bin/consume-corpus-static" | egrep 'song' || echo "(none)"
  echo "--- any /home/hoo/Source (dev tree) baked into binaries? (must be empty) ---"
  for b in "$W"/bin/*; do strings -a "$b" | egrep '/home/hoo/Source|native/build|build/artifacts' && echo "DEV_TREE_REF_IN $b"; done
  echo "(dev-tree scan done)"
} >> "$LOG/stage02-build.log" 2>&1
sed -n '/=== ARTIFACT SHAPE ===/,$p' "$LOG/stage02-build.log"

echo
echo "BUILD_EXITS static_example=$rc1 static_corpus=$rc2 shared_example=$rc3 shared_corpus=$rc4 shared_rpath=$rc5"
