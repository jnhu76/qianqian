#!/usr/bin/env bash
# Lane E: corrected static link matrix (the -O2 abi-complete probe was elided by GCC)
# plus archive-level symbol-name scan and consumer GLIBC floors.
set -u
export LC_ALL=C

ART=/home/hoo/g4-172/linux-root/src/native/build/artifacts
INC=/home/hoo/g4-172/linux-root/src/native/include
EVD=/mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/g4/lane-e-runtime-deps
RAW="$EVD/raw"
W="$HOME/lane-e-runtime/work"
LOG="$RAW/static-matrix-exitcodes.txt"
: > "$LOG"

logrun() {
  local desc="$1"; local out="$2"; shift 2
  "$@" > "$out" 2>&1
  local rc=$?
  printf '%s | argv: %s | exit=%s | output=%s\n' "$desc" "$*" "$rc" "$out" >> "$LOG"
  return $rc
}

cd "$W"

# ---- 0. document why the -O2 probe is not closure-complete ----
{
  echo "== '-O2 abi-complete' binary: defined symbols (expect only main) =="
  nm static/abi-complete-libs | grep ' [Tt] ' || true
  echo
  echo "== '-O2 abi-complete' binary: undefined symbols =="
  nm -u static/abi-complete-libs
  echo
  echo "== sizes (bytes): -O2 probe vs minimal consumer =="
  stat -c '%s %n' static/abi-complete-libs static/minimal-libs static/external-consumer-static
} > "$RAW/probe-elision-proof.txt" 2>&1
printf 'probe elision proof | argv: nm/stat on built binaries | exit=%s | output=%s\n' "$?" "$RAW/probe-elision-proof.txt" >> "$LOG"

# ---- 1. real closure at -O0 ----
logrun "O0 abi-complete WITHOUT -lm -lpthread" "$RAW/link-O0-abi-nolibs.log" \
  gcc -O0 -I"$INC" src/abi-complete-consumer.c -L"$ART" -lsongcore -o static/abi-O0-nolibs
logrun "O0 abi-complete with -lpthread only" "$RAW/link-O0-abi-nomath.log" \
  gcc -O0 -I"$INC" src/abi-complete-consumer.c -L"$ART" -lsongcore -lpthread -o static/abi-O0-nomath
logrun "O0 abi-complete with -lm only" "$RAW/link-O0-abi-nopthread.log" \
  gcc -O0 -I"$INC" src/abi-complete-consumer.c -L"$ART" -lsongcore -lm -o static/abi-O0-nopthread
logrun "O0 abi-complete with -lm -lpthread" "$RAW/link-O0-abi-libs.log" \
  gcc -O0 -I"$INC" src/abi-complete-consumer.c -L"$ART" -lsongcore -lm -lpthread -o static/abi-O0-libs

# minimal consumer: isolate -lm vs -lpthread
logrun "minimal with -lm only" "$RAW/link-minimal-lm-only.log" \
  gcc -O2 -I"$INC" src/minimal-consumer.c -L"$ART" -lsongcore -lm -o static/minimal-lm-only
logrun "minimal with -lpthread only" "$RAW/link-minimal-pthread-only.log" \
  gcc -O2 -I"$INC" src/minimal-consumer.c -L"$ART" -lsongcore -lpthread -o static/minimal-pthread-only

# ---- 2. archive undefined symbol-name scan (static-form floor drivers by NAME) ----
nm -A --undefined-only "$ART/libsongcore.a" > "$RAW/archive-undefined-all.txt" 2>&1
printf 'archive undefined dump | argv: nm -A --undefined-only %s | exit=%s | output=%s\n' "$ART/libsongcore.a" "$?" "$RAW/archive-undefined-all.txt" >> "$LOG"
awk '{print $NF}' "$RAW/archive-undefined-all.txt" | sed 's/@.*//' | sort -u > "$RAW/archive-undefined-names.txt"
wc -l < "$RAW/archive-undefined-names.txt" > "$RAW/archive-undefined-unique-count.txt"
{
  echo "== unique undefined names in merged archive: $(cat "$RAW/archive-undefined-unique-count.txt") =="
  echo
  echo "== C23/glibc-redirect symbol names (__isoc*) =="
  grep -E '^__isoc' "$RAW/archive-undefined-names.txt" || echo "(none)"
  echo
  echo "== pthread_* / dl* / rt-class names =="
  grep -E '^(pthread_|dl(open|close|sym|error)|clock_gettime|shm_)' "$RAW/archive-undefined-names.txt" || echo "(none)"
  echo
  echo "== libm-class math names (sample) =="
  grep -E '^(cos|sin|exp|log|log2|atan|atan2|pow|sqrt|hypot|ldexp|frexp|floor|ceil|round|fmod|fabs)f?$' "$RAW/archive-undefined-names.txt" | sort -u
} > "$RAW/archive-symbol-classes.txt" 2>&1
printf 'archive symbol classes | argv: awk/grep over archive-undefined-names.txt | exit=%s | output=%s\n' "$?" "$RAW/archive-symbol-classes.txt" >> "$LOG"

# ---- 3. GLIBC floors of host-linked consumers ----
for b in static/minimal-libs static/external-consumer-static static/abi-O0-libs; do
  [ -e "$b" ] || continue
  n=$(basename "$b")
  readelf --version-info "$b" > "$RAW/readelf-version-info-$n.txt" 2>&1
  grep -o 'GLIBC_[0-9.]*' "$RAW/readelf-version-info-$n.txt" | sort -uV > "$RAW/glibc-versions-$n.txt"
  printf '%s highest GLIBC: %s\n' "$n" "$(tail -n1 "$RAW/glibc-versions-$n.txt")" >> "$RAW/consumer-glibc-floors.txt"
  printf 'readelf version-info %s | argv: readelf --version-info %s | exit=%s | output=%s\n' "$n" "$b" "$?" "$RAW/readelf-version-info-$n.txt" >> "$LOG"
done

# ---- 4. shared runs, re-executed with the true argv recorded ----
{
  echo "### shared abi-complete"
  echo "argv: (cd $W/shared && ./abi-complete-shared)"
  ( cd "$W/shared" && ./abi-complete-shared ); echo "exit=$?"
  echo
  echo "### shared external-consumer-example mp3-cbr-id3v23"
  echo "argv: (cd $W/shared && ./external-consumer-shared /home/hoo/Source/qianqian/native/experiments/songcore-equivalence/fixtures/mp3-cbr-id3v23.mp3)"
  ( cd "$W/shared" && ./external-consumer-shared /home/hoo/Source/qianqian/native/experiments/songcore-equivalence/fixtures/mp3-cbr-id3v23.mp3 ); echo "exit=$?"
  echo
  echo "### shared sizeof probe"
  echo "argv: (cd $W/shared && ./abi-sizeof-probe-shared)"
  ( cd "$W/shared" && ./abi-sizeof-probe-shared ); echo "exit=$?"
} > "$RAW/consumer-runs-shared.txt" 2>&1
printf 'shared runs corrected receipt | argv: three runs under %s/shared | exit=%s | output=%s\n' "$W" "$?" "$RAW/consumer-runs-shared.txt" >> "$LOG"

# ---- 5. run the O0-linked real-closure binary ----
"$W/static/abi-O0-libs" > "$RAW/run-static-abi-O0.txt" 2>&1
printf 'static abi-O0-libs | argv: %s | exit=%s | output=%s\n' "$W/static/abi-O0-libs" "$?" "$RAW/run-static-abi-O0.txt" >> "$LOG"

echo "done"
