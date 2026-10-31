#!/usr/bin/env bash
# D1 counterfactual harness — SongCore build.rs provenance digest defect.
# Campaign #172; defect confirmed by #324 (ENVIRONMENT.md "Out-of-scope build
# observations" #1) and fixed on branch songcore-172/pr-a-build-fixes.
#
# Defect (pre-fix): build.rs sha256_hex() shells out to GNU coreutils sha256sum
# and takes the FIRST whitespace token of its output. On Windows the absolute
# path always contains backslashes; coreutils then prints the digest line in
# escaped format ("\ <digest>  <escaped-name>"), so
# QN_SONGCORE_HEADER_SHA256 / QN_SONGCORE_ARTIFACT_SHA256 carry a leading "\"
# (65 chars, not 64 hex). Without sha256sum on PATH the whole build panics.
#
# Phases:
#   run.sh red    — exercises the PRE-FIX implementation
#                   (build_rs_prefix_snapshot.rs: byte-identical snapshot of
#                   build.rs at base 05f5fef, sha256 3cb898c6d25c2efd...) across
#                   cases a-e and asserts the RED behavior is real.
#   run.sh green  — exercises the CURRENT (fixed) build.rs the same way, plus a
#                   real cargo build of the real crate, and asserts the digest
#                   is exactly 64 lowercase hex and equal to two independent
#                   oracles (coreutils sha256sum on a clean relative path, and
#                   Python hashlib).
#
# Cases:
#   a) absolute path as the build script passes it to sha256sum
#      (on Windows this is always backslash form — the RED trigger;
#       on Linux/WSL it is POSIX form — recorded as the correct-behavior
#       contrast)
#   b) path containing spaces
#   c) path containing non-ASCII characters
#   d) no sha256sum on PATH
#   e) cargo rerun-if-changed behavior across file content changes
#
# Usage: bash run.sh red|green     (Git Bash on Windows, or bash inside WSL)
# Results: results/<os>-<phase>-*.txt (local, git-ignored). Scratch is recreated per run
# and removed on success.
set -euo pipefail

PHASE="${1:?usage: run.sh red|green}"
case "$PHASE" in red|green) ;; *) echo "phase must be red or green" >&2; exit 2 ;; esac

HARNESS="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HARNESS/../../.." && pwd)"
if [ "$(basename "$ROOT")" != "qianqian-172" ]; then
  echo "refusing to run outside the qianqian-172 worktree (ROOT=$ROOT)" >&2
  exit 2
fi

OS_RAW="$(uname -s)"
case "$OS_RAW" in
  MINGW*|MSYS*|CYGWIN*) OS=windows ;;
  Linux)                OS=linux ;;
  *) echo "unsupported host: $OS_RAW" >&2; exit 2 ;;
esac

RESULTS="$HARNESS/results"
# HARNESS_WORK_ROOT: where per-run scratch (probe build, case trees, cargo
# targets) lives. Defaults to the harness dir. On WSL set it to a native-fs
# dir (e.g. $HOME/...): /mnt/c mtime readings can drift between stats, which
# makes cargo's own no-change fingerprint check spuriously Dirty (the same
# /mnt/c caveat that pushes native builds into the WSL local fs in #324).
WORK_ROOT="${HARNESS_WORK_ROOT:-$HARNESS}"
mkdir -p "$WORK_ROOT"
SCRATCH="$WORK_ROOT/scratch-$PHASE"
SNAPSHOT="$HARNESS/build_rs_prefix_snapshot.rs"
HEADER_SRC="$ROOT/native/include/songcore.h"
CRATE="$ROOT/crates/qianqian-songcore-sys"

# On Windows the default PATH shadows python3 with the Store stub, which
# exists on PATH (so `command -v` finds it) but exits 49 without output.
# Probe usability, not existence, and fall back to the real interpreter
# (WORKBRIEF §1: /c/Users/fred1/tools/bin shim, 3.13.14).
if ! python3 -c 'pass' >/dev/null 2>&1; then
  export PATH="/c/Users/fred1/tools/bin:$PATH"
fi
python3 -c 'pass' >/dev/null 2>&1 || { echo "python3 not usable (Store stub on PATH?)" >&2; exit 2; }
for tool in cargo rustc sha256sum; do
  command -v "$tool" >/dev/null 2>&1 || { echo "$tool not available" >&2; exit 2; }
done

rm -rf "$SCRATCH"
mkdir -p "$SCRATCH" "$RESULTS"

FAILED=0
check() { # check <res-file> <description> <1|0>
  local res="$1" desc="$2" ok="$3"
  if [ "$ok" = "1" ]; then
    printf 'ASSERTION PASS: %s\n' "$desc" | tee -a "$res"
  else
    printf 'ASSERTION FAIL: %s\n' "$desc" | tee -a "$res"
    FAILED=$((FAILED+1))
  fi
}

to_win() { cygpath -w "$1"; }

gen_artifact() { # deterministic ~1 MiB valid GNU ar archive (rustc validates
                 # the archive identifier when consuming -l static=songcore)
  python3 -c 'import sys
data = bytes(((i*31+7)&255) for i in range(1048576-68))
def member(name, body):
    h = name.ljust(16) + "0".ljust(12) + "0".ljust(6) + "0".ljust(6) + "644".ljust(8)
    h += str(len(body)).ljust(10) + "`\n"
    return h.encode() + body + (b"\n" if len(body) % 2 else b"")
sys.stdout.buffer.write(b"!<arch>\n" + member("song.o/", data))' > "$1"
}

make_tree() { # make_tree <tree-dir>: native tree + fake crate skeleton
  mkdir -p "$1/native/include" "$1/native/build/artifacts" "$1/crates/fake-sys/src"
  cp "$HEADER_SRC" "$1/native/include/songcore.h"
  gen_artifact "$1/native/build/artifacts/libsongcore.a"
}

# Oracles. Both read the same physical bytes.
oracle_rel() { # clean coreutils digest: relative path, no escaping possible
  ( cd "$1" && sha256sum "$2" | awk '{print $1}' )
}
oracle_py() { # independent oracle: Python hashlib
  python3 -c 'import hashlib,sys
print(hashlib.sha256(open(sys.argv[1],"rb").read()).hexdigest())' "$1"
}

is_prefixed_hex() { printf '%s' "$1" | grep -Eq '^\\[0-9a-f]{64}$'; }
is_clean_hex()    { printf '%s' "$1" | grep -Eq '^[0-9a-f]{64}$'; }

# ---------------------------------------------------------------------------
# Probe compilation. The probe is the build script itself, compiled standalone
# with rustc and driven via CARGO_MANIFEST_DIR exactly as cargo drives it.
# ---------------------------------------------------------------------------
if [ "$PHASE" = red ]; then
  IMPL_LABEL="pre-fix snapshot build_rs_prefix_snapshot.rs (byte-identical to build.rs at base 05f5fef)"
  PROBE_SRC="$SNAPSHOT"
else
  IMPL_LABEL="current post-fix build.rs (copied live from the crate at green-run time)"
  PROBE_SRC="$SCRATCH/build_rs_current.rs"
  cp "$CRATE/build.rs" "$PROBE_SRC"
fi
EXE=""; [ "$OS" = windows ] && EXE=".exe"
PROBE="$SCRATCH/probe-$PHASE$EXE"
rustc --edition 2024 -o "$PROBE" "$PROBE_SRC" 1>&2
PROBE_SHA="$( ( cd "$(dirname "$PROBE_SRC")" && sha256sum "$(basename "$PROBE_SRC")" ) | awk '{print $1}')"
echo "compiled probe: $PROBE (source sha256 $PROBE_SHA)"

# run_probe <res-file> <title> <manifest-dir-posix> [stripped-path-dir]
# Runs the probe; leaves PROBE_RC/QN_HEADER/QN_ARTIFACT set for assertions.
run_probe() {
  local res="$1" title="$2" md="$3" strip="${4:-}"
  local md_arg="$md"
  [ "$OS" = windows ] && md_arg="$(to_win "$md")"
  {
    printf 'CASE: %s\n' "$title"
    printf 'PHASE: %s    OS: %s (%s)\n' "$PHASE" "$OS" "${MSYSTEM:-$(uname -r)}"
    printf 'IMPL: %s\n' "$IMPL_LABEL"
    printf 'PROBE SOURCE SHA256: %s\n' "$PROBE_SHA"
    printf 'CARGO_MANIFEST_DIR: %s\n' "$md_arg"
    printf 'COMMAND: env CARGO_MANIFEST_DIR="%s"%s %s\n' \
      "$md_arg" "${strip:+ PATH=<stripped-to-emptybin>}" "$(basename "$PROBE")"
    printf -- '--- probe stdout+stderr ---\n'
  } > "$res"
  PROBE_RC=0
  if [ -n "$strip" ]; then
    ( cd "$ROOT" && env PATH="$strip" CARGO_MANIFEST_DIR="$md_arg" "$PROBE" ) >> "$res" 2>&1 || PROBE_RC=$?
  else
    ( cd "$ROOT" && env CARGO_MANIFEST_DIR="$md_arg" "$PROBE" ) >> "$res" 2>&1 || PROBE_RC=$?
  fi
  printf -- '--- probe exit code: %d ---\n' "$PROBE_RC" >> "$res"
  QN_HEADER="$(sed -n 's/^cargo:rustc-env=QN_SONGCORE_HEADER_SHA256=//p' "$res" | head -n1)"
  QN_ARTIFACT="$(sed -n 's/^cargo:rustc-env=QN_SONGCORE_ARTIFACT_SHA256=//p' "$res" | head -n1)"
  cat "$res"
}

# assert_digest <res-file> <expect-prefix yes|no> <qn-value> <clean-oracle>
assert_digest() {
  local res="$1" want_prefix="$2" val="$3" oracle="$4"
  local stripped="$val"
  if [ "$want_prefix" = yes ]; then
    check "$res" "digest carries the coreutils escape prefix (RED signature): value='$val'" \
      "$(is_prefixed_hex "$val" && echo 1 || echo 0)"
    stripped="${val#\\}"
  else
    check "$res" "digest is exactly 64 lowercase hex chars (no prefix): value='$val'" \
      "$(is_clean_hex "$val" && echo 1 || echo 0)"
  fi
  check "$res" "digest hex payload == coreutils oracle ($oracle)" \
    "$([ "$stripped" = "$oracle" ] && echo 1 || echo 0)"
}

# run_path_case <case-id> <case-dirname> <title> <expect-prefix>
run_path_case() {
  local id="$1" case_dir="$2" title="$3" want_prefix="$4"
  local tree="$SCRATCH/$case_dir/tree"
  local md="$tree/crates/fake-sys"
  make_tree "$tree"
  local res="$RESULTS/$OS-$PHASE-case-$id.txt"
  run_probe "$res" "$title" "$md"
  local oh oa ph pa
  oh="$(oracle_rel "$tree/native/include" "songcore.h")"
  oa="$(oracle_rel "$tree/native/build/artifacts" "libsongcore.a")"
  ph="$(oracle_py "$tree/native/include/songcore.h")"
  pa="$(oracle_py "$tree/native/build/artifacts/libsongcore.a")"
  {
    printf 'ORACLE(coreutils) header:   %s\n' "$oh"
    printf 'ORACLE(coreutils) artifact: %s\n' "$oa"
    printf 'ORACLE(hashlib)   header:   %s\n' "$ph"
    printf 'ORACLE(hashlib)   artifact: %s\n' "$pa"
  } >> "$res"
  check "$res" "probe exited 0 (build script completes)" \
    "$([ "$PROBE_RC" = 0 ] && echo 1 || echo 0)"
  assert_digest "$res" "$want_prefix" "$QN_HEADER" "$oh"
  assert_digest "$res" "$want_prefix" "$QN_ARTIFACT" "$oa"
  if [ "$PHASE" = green ]; then
    check "$res" "digest hex == independent python hashlib oracle (header)" \
      "$([ "$QN_HEADER" = "$ph" ] && echo 1 || echo 0)"
    check "$res" "digest hex == independent python hashlib oracle (artifact)" \
      "$([ "$QN_ARTIFACT" = "$pa" ] && echo 1 || echo 0)"
  fi
}

# run_case_e <impl-note> <manifest-dir-posix> <target-dir> <res-prefix> <tree-posix>
run_case_e() {
  local impl_note="$1" md="$2" tgt="$3" res_prefix="$4" tree="$5"
  local md_arg="$md"
  [ "$OS" = windows ] && md_arg="$(to_win "$md")"
  local nat="$tree/native"
  [ "$OS" = windows ] && nat="$(to_win "$nat")"

  local res out_file orig_qh orig_qa oh oa
  local cargo_cmd
  cargo_cmd=(cargo build --manifest-path "$md_arg/Cargo.toml" --target-dir "$tgt" -v)

  echo "=== case e [$impl_note] s1: cold build (real cargo, real build script) ==="
  res="$res_prefix-s1-cold.txt"
  {
    printf 'CASE e (%s): cold cargo build; build script record comes from cargo itself (target/debug/build/*/output)\n' "$impl_note"
    printf 'IMPL: %s\n' "$impl_note"
    printf 'QIANQIAN_NATIVE_DIR: %s\n' "$nat"
    printf 'COMMAND: %s\n' "${cargo_cmd[*]}"
    printf -- '--- cargo output ---\n'
  } > "$res"
  ( cd "$ROOT" && QIANQIAN_NATIVE_DIR="$nat" "${cargo_cmd[@]}" ) >> "$res" 2>&1 \
    || { printf 'ASSERTION FAIL: cold cargo build exited non-zero\n' | tee -a "$res"; FAILED=$((FAILED+1)); return 1; }
  out_file="$(find "$tgt/debug/build" -name output -path '*qianqian-songcore-sys*' | head -n1)"
  if [ -z "$out_file" ]; then
    echo "ASSERTION FAIL: build script output file not found" >&2
    FAILED=$((FAILED+1)); return 1
  fi
  orig_qh="$(sed -n 's/^cargo:rustc-env=QN_SONGCORE_HEADER_SHA256=//p' "$out_file" | head -n1)"
  orig_qa="$(sed -n 's/^cargo:rustc-env=QN_SONGCORE_ARTIFACT_SHA256=//p' "$out_file" | head -n1)"
  oh="$(oracle_rel "$tree/native/include" "songcore.h")"
  oa="$(oracle_rel "$tree/native/build/artifacts" "libsongcore.a")"
  {
    printf 'cargo build exit code: 0\n'
    printf 'build-script output file: %s\n' "$out_file"
    printf 'QN_SONGCORE_HEADER_SHA256  (cargo-recorded): %s\n' "$orig_qh"
    printf 'QN_SONGCORE_ARTIFACT_SHA256(cargo-recorded): %s\n' "$orig_qa"
    printf 'ORACLE(coreutils) header:   %s\n' "$oh"
    printf 'ORACLE(coreutils) artifact: %s\n' "$oa"
  } >> "$res"
  check "$res" "cold build succeeded" 1
  if [ "$PHASE" = red ]; then
    # Same branch rule as the probe cases: the escape-prefix RED signature is
    # only expected on Windows paths (case-e runs on both hosts).
    assert_digest "$res" "$WANT_PREFIX" "$orig_qh" "$oh"
    assert_digest "$res" "$WANT_PREFIX" "$orig_qa" "$oa"
  else
    assert_digest "$res" no "$orig_qh" "$oh"
    assert_digest "$res" no "$orig_qa" "$oa"
    check "$res" "header digest == python hashlib oracle" \
      "$([ "$orig_qh" = "$(oracle_py "$tree/native/include/songcore.h")" ] && echo 1 || echo 0)"
    check "$res" "artifact digest == python hashlib oracle" \
      "$([ "$orig_qa" = "$(oracle_py "$tree/native/build/artifacts/libsongcore.a")" ] && echo 1 || echo 0)"
  fi

  local rerun_re='Running.*build-script-build'
  count_reruns() { awk -v re="$rerun_re" '$0 ~ re {n++} END{print n+0}' "$1"; }
  cargo_e() { # cargo_e <res-file> ; asserts rc==0, records output
    ( cd "$ROOT" && QIANQIAN_NATIVE_DIR="$nat" "${cargo_cmd[@]}" ) >> "$1" 2>&1
  }

  echo "=== case e [$impl_note] s2: rebuild with no change (expect fresh) ==="
  res="$res_prefix-s2-fresh.txt"
  { printf 'CASE e (%s): rebuild with no change — expect NO build-script rerun\nIMPL: %s\nCOMMAND: %s\n--- cargo output ---\n' "$impl_note" "$impl_note" "${cargo_cmd[*]}"; } > "$res"
  rc=0; cargo_e "$res" || rc=$?
  local c2; c2="$(count_reruns "$res")"
  printf 'cargo exit code: %d\nbuild-script rerun lines observed: %s\n' "$rc" "$c2" >> "$res"
  check "$res" "no-change rebuild exits 0" "$([ "$rc" = 0 ] && echo 1 || echo 0)"
  check "$res" "no-change rebuild does NOT re-run the build script (fresh)" \
    "$([ "$c2" = 0 ] && echo 1 || echo 0)"

  echo "=== case e [$impl_note] s3: untracked file control (expect fresh) ==="
  res="$res_prefix-s3-untracked.txt"
  : > "$(dirname "$md")/untracked-e-probe.txt"
  { printf 'CASE e (%s): created untracked file untracked-e-probe.txt next to the manifest — expect NO rerun (the emitted rerun-if-changed lines replaced the default package-wide watch)\nIMPL: %s\nCOMMAND: %s\n--- cargo output ---\n' "$impl_note" "$impl_note" "${cargo_cmd[*]}"; } > "$res"
  rc=0; cargo_e "$res" || rc=$?
  rm -f "$(dirname "$md")/untracked-e-probe.txt"
  local c3; c3="$(count_reruns "$res")"
  printf 'cargo exit code: %d\nbuild-script rerun lines observed: %s\n' "$rc" "$c3" >> "$res"
  check "$res" "untracked-file rebuild exits 0" "$([ "$rc" = 0 ] && echo 1 || echo 0)"
  check "$res" "untracked package file does NOT re-run the build script" \
    "$([ "$c3" = 0 ] && echo 1 || echo 0)"

  echo "=== case e [$impl_note] s4: header content change (expect rerun + new digest) ==="
  res="$res_prefix-s4-header-change.txt"
  cp "$tree/native/include/songcore.h" "$SCRATCH/header.orig"
  printf '>> e-probe append\n' >> "$tree/native/include/songcore.h"
  { printf 'CASE e (%s): appended a line to the tracked header — expect rerun and a NEW digest\nIMPL: %s\nCOMMAND: %s\n--- cargo output ---\n' "$impl_note" "$impl_note" "${cargo_cmd[*]}"; } > "$res"
  rc=0; cargo_e "$res" || rc=$?
  local c4 qh4 qa4 oh4
  c4="$(count_reruns "$res")"
  qh4="$(sed -n 's/^cargo:rustc-env=QN_SONGCORE_HEADER_SHA256=//p' "$out_file" | head -n1)"
  qa4="$(sed -n 's/^cargo:rustc-env=QN_SONGCORE_ARTIFACT_SHA256=//p' "$out_file" | head -n1)"
  oh4="$(oracle_rel "$tree/native/include" "songcore.h")"
  {
    printf 'cargo exit code: %d\n' "$rc"
    printf 'build-script rerun lines observed: %s\n' "$c4"
    printf 'QN_SONGCORE_HEADER_SHA256  (cargo-recorded, after change): %s\n' "$qh4"
    printf 'QN_SONGCORE_ARTIFACT_SHA256(cargo-recorded, unchanged artifact): %s\n' "$qa4"
    printf 'ORACLE(coreutils) header after change: %s\n' "$oh4"
  } >> "$res"
  check "$res" "header-change rebuild exits 0" "$([ "$rc" = 0 ] && echo 1 || echo 0)"
  check "$res" "header content change DOES re-run the build script" \
    "$([ "$c4" -gt 0 ] && echo 1 || echo 0)"
  check "$res" "digest actually changed after content change" \
    "$([ "$qh4" != "$orig_qh" ] && echo 1 || echo 0)"
  if [ "$PHASE" = red ]; then
    assert_digest "$res" "$WANT_PREFIX" "$qh4" "$oh4"
  else
    assert_digest "$res" no "$qh4" "$oh4"
    check "$res" "post-change header digest == python hashlib oracle" \
      "$([ "$qh4" = "$(oracle_py "$tree/native/include/songcore.h")" ] && echo 1 || echo 0)"
  fi
  check "$res" "artifact digest unchanged (artifact file untouched)" \
    "$([ "$qa4" = "$orig_qa" ] && echo 1 || echo 0)"

  echo "=== case e [$impl_note] s5: header restored (expect rerun + original digest) ==="
  res="$res_prefix-s5-header-restored.txt"
  cp "$SCRATCH/header.orig" "$tree/native/include/songcore.h"
  { printf 'CASE e (%s): restored original header bytes — expect rerun and the ORIGINAL digest\nIMPL: %s\nCOMMAND: %s\n--- cargo output ---\n' "$impl_note" "$impl_note" "${cargo_cmd[*]}"; } > "$res"
  rc=0; cargo_e "$res" || rc=$?
  local c5 qh5
  c5="$(count_reruns "$res")"
  qh5="$(sed -n 's/^cargo:rustc-env=QN_SONGCORE_HEADER_SHA256=//p' "$out_file" | head -n1)"
  {
    printf 'cargo exit code: %d\n' "$rc"
    printf 'build-script rerun lines observed: %s\n' "$c5"
    printf 'QN_SONGCORE_HEADER_SHA256 (cargo-recorded, after restore): %s\n' "$qh5"
  } >> "$res"
  check "$res" "header-restore rebuild exits 0" "$([ "$rc" = 0 ] && echo 1 || echo 0)"
  check "$res" "header restore DOES re-run the build script" "$([ "$c5" -gt 0 ] && echo 1 || echo 0)"
  check "$res" "restored digest equals the original digest" \
    "$([ "$qh5" = "$orig_qh" ] && echo 1 || echo 0)"
}

# ===========================================================================
echo "==================== $OS / $PHASE ===================="
# RED signature ("\-prefixed digest") is expected only for the pre-fix
# implementation on Windows paths. On Linux/WSL the old implementation is
# correct (no backslashes) — recorded as the platform contrast.
if [ "$OS" = windows ] && [ "$PHASE" = red ]; then WANT_PREFIX=yes; else WANT_PREFIX=no; fi

echo "=== case a: absolute path as passed to sha256sum ==="
run_path_case a-backslash "case-a" "a) absolute path as the build script passes it to sha256sum" "$WANT_PREFIX"

if [ "$OS" = windows ]; then
  echo "=== case a-mechanism: raw sha256sum output, both path spellings ==="
  res="$RESULTS/$OS-$PHASE-case-a-mechanism.txt"
  t="$SCRATCH/case-a-mechanism"
  mkdir -p "$t"
  printf 'mechanism probe\n' > "$t/probe.txt"
  {
    printf 'CASE a-mechanism: raw GNU coreutils sha256sum output for the same file\n'
    printf -- '--- command 1: sha256sum with a Windows backslash path ---\n'
  } > "$res"
  rc1=0; ( cd "$ROOT" && sha256sum "$(to_win "$t")\\probe.txt" ) >> "$res" 2>&1 || rc1=$?
  printf 'exit code: %d\n--- command 2: sha256sum with a POSIX relative path ---\n' "$rc1" >> "$res"
  rc2=0; ( cd "$t" && sha256sum ./probe.txt ) >> "$res" 2>&1 || rc2=$?
  printf 'exit code: %d\n' "$rc2" >> "$res"
  cat "$res"
fi

echo "=== case b: path containing spaces ==="
run_path_case b-space "case-b/has space" "b) path containing spaces" "$WANT_PREFIX"

echo "=== case c: path containing non-ASCII characters ==="
run_path_case c-nonascii "case-c/ünïcodé-中文" "c) path containing non-ASCII characters" "$WANT_PREFIX"

echo "=== case d: no sha256sum on PATH ==="
res="$RESULTS/$OS-$PHASE-case-d-no-sha256sum.txt"
make_tree "$SCRATCH/case-d/tree"
mkdir -p "$SCRATCH/emptybin"
run_probe "$res" "d) sha256sum absent from PATH" "$SCRATCH/case-d/tree/crates/fake-sys" "$SCRATCH/emptybin"
if [ "$PHASE" = red ]; then
  check "$res" "build script PANICS without sha256sum (spawn failure), exit code $PROBE_RC" \
    "$([ "$PROBE_RC" != 0 ] && echo 1 || echo 0)"
  check "$res" "panic message names sha256sum" \
    "$(grep -q 'sha256sum failed' "$res" && echo 1 || echo 0)"
else
  check "$res" "GREEN: build script completes WITHOUT sha256sum on PATH (semantic change: no external tool dependency)" \
    "$([ "$PROBE_RC" = 0 ] && echo 1 || echo 0)"
  oh="$(oracle_rel "$SCRATCH/case-d/tree/native/include" "songcore.h")"
  oa="$(oracle_rel "$SCRATCH/case-d/tree/native/build/artifacts" "libsongcore.a")"
  printf 'ORACLE(coreutils) header:   %s\nORACLE(coreutils) artifact: %s\n' "$oh" "$oa" >> "$res"
  assert_digest "$res" no "$QN_HEADER" "$oh"
  assert_digest "$res" no "$QN_ARTIFACT" "$oa"
fi

echo "=== case e: rerun-if-changed across content changes ==="
if [ "$PHASE" = red ]; then
  E_TREE="$SCRATCH/case-e/tree"
  E_MD="$E_TREE/crates/fake-sys"
  make_tree "$E_TREE"
  cp "$CRATE/Cargo.toml" "$E_MD/"
  cp "$CRATE/src/lib.rs" "$E_MD/src/"
  cp "$SNAPSHOT" "$E_MD/build.rs"
  run_case_e "throwaway crate copy with the PRE-FIX build.rs snapshot" \
    "$E_MD" "$SCRATCH/case-e/target" "$RESULTS/$OS-$PHASE-case-e" "$E_TREE"
else
  E_TREE="$SCRATCH/case-e/tree"
  make_tree "$E_TREE"
  run_case_e "the REAL crate (fixed build.rs), scratch target dir" \
    "$CRATE" "$SCRATCH/case-e-target" "$RESULTS/$OS-$PHASE-case-e" "$E_TREE"
  # NOTE: do not delete the crate-local Cargo.lock — it is a tracked file
  # (this dependency-free crate reproduces it byte-identically, so cargo
  # builds leave it unmodified; verified 2026-10-29 with cargo 1.96.0).
fi

echo
echo "==================== SUMMARY: $FAILED failed assertion(s) ===================="
if [ "$FAILED" -ne 0 ]; then
  exit 1
fi
rm -rf "$SCRATCH"
echo "scratch removed; results retained in results/"
