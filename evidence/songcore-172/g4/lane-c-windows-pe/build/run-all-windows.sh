#!/usr/bin/env bash
# Lane C (windows) — execute the cross-built PE consumer ON THE REAL WINDOWS
# HOST (this script runs under Git Bash, i.e. a native Windows process), capture
# argv, exit code, stdout and stderr for every case.
set -u
RUN=/c/Users/fred1/lane-c-win-pe
LOGDIR=$RUN/run
mkdir -p "$LOGDIR"
cd "$RUN" || exit 90

{
  echo "runner=Git Bash (native Windows process)"
  echo "host_hostname=$(hostname)"
  echo "host_uname=$(uname -a)"
  echo "host_windows=$(MSYS_NO_PATHCONV=1 cmd.exe /c ver 2>/dev/null | tr -d '\r' | grep -E '[0-9]' | tail -1)"
  echo "host_exe_path_windows=C:\\Users\\fred1\\lane-c-win-pe\\pe-consumer.exe"
  echo "host_exe_path_unix=$RUN/pe-consumer.exe"
  echo "host_build=$(sha256sum "$RUN/pe-consumer.exe" | cut -d' ' -f1)"
  echo "host_cwd=$PWD"
  echo "host_date_utc=$(date -u +%Y-%m-%dT%H:%M:%SZ)"
} > "$LOGDIR/host.txt" 2>&1
echo "### EXIT[host.txt]=$?"
cat "$LOGDIR/host.txt"

run_case() {
  local id="$1"; shift
  "$@" > "$LOGDIR/$id.stdout.txt" 2> "$LOGDIR/$id.stderr.txt"
  local rc=$?
  echo "$rc" > "$LOGDIR/$id.exit.txt"
  echo "### CASE[$id] exit=$rc cmd=[$*]"
  return 0
}

echo
echo "=== A. sha256 selftest (validates the consumer's own hasher on this host) ==="
run_case selftest ./pe-consumer.exe --selftest
grep -E "SHA256_SELFTEST|sha256_selftest" "$LOGDIR/selftest.stdout.txt"

echo
echo "=== B. full walk, all four reference fixtures (direct PE execution) ==="
for f in mp3-cbr-id3v23.mp3 flac-16-44-stereo.flac alac-16-44-stereo.m4a alac-long.m4a; do
  id="${f%%.*}"
  run_case "$id" ./pe-consumer.exe "media/$f"
done

echo
echo "=== C. same mp3 case driven by cmd.exe (second Windows launcher) ==="
MSYS_NO_PATHCONV=1 cmd.exe /c "C:\Users\fred1\lane-c-win-pe\pe-consumer.exe C:\Users\fred1\lane-c-win-pe\media\mp3-cbr-id3v23.mp3" \
  > "$LOGDIR/cmd-mp3-cbr-id3v23.stdout.txt" 2> "$LOGDIR/cmd-mp3-cbr-id3v23.stderr.txt"
echo "$?" > "$LOGDIR/cmd-mp3-cbr-id3v23.exit.txt"
echo "### CASE[cmd-mp3] exit=$(cat "$LOGDIR/cmd-mp3-cbr-id3v23.exit.txt")"

echo
echo "=== D. negative-control: menu path with a missing file (usage/IO exit class) ==="
run_case usage-noargs ./pe-consumer.exe

echo
echo "=== summary ==="
for f in "$LOGDIR"/*.exit.txt; do
  printf '%-34s exit=%s\n' "$(basename "$f" .exit.txt)" "$(cat "$f")"
done

echo
echo "=== key lines from every case ==="
for f in "$LOGDIR"/*.stdout.txt; do
  echo "--- $(basename "$f") ---"
  grep -E "^(host_os|abi_version|bcrypt_runtime|bcrypt_runtime_probe_rc|bcrypt_iat_counter|bcrypt_iat_positive_control|media_path|stage=probe|stage=decode|bcrypt_genrandom|stage=seek|stage=post_seek_decode|stage=close|RESULT=|SHA256_SELFTEST)" "$f"
done

echo
echo "=== stderr byte counts (0 expected everywhere) ==="
wc -c "$LOGDIR"/*.stderr.txt
echo "RUNALL_DONE"
