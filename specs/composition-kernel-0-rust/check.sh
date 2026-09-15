#!/usr/bin/env bash
# FV-RUST-0 runner: scenario verification of the real qianqian-composition
# kernel (campaign QIANQIAN-VERIFICATION-CAMPAIGN-1, Phase B1).
#
# This runs the channel that produced the recorded evidence (see
# RESULTS.md): the dual-gated harnesses in src/kernel_verify.rs executed
# natively and under Miri, plus the known-bad mutation controls. The
# harnesses ALSO compile as Kani proof harnesses (cfg(kani)), but the
# symbolic Kani channel is recorded TOOLING-INSUFFICIENT (no convergence
# within ~40 min CPU per harness; formulas dominated by std
# BTreeMap/String expansion). It is not run here; see RESULTS.md for the
# concrete bounds before attempting it.
#
# Fail-closed contract:
#   - all 7 harness matrices must pass natively and under Miri;
#   - the §G.6 cross-locus refinement oracle (lifecycle_oracles integration
#     test; teardown-closure locus — see CompositionKernel0 RESULTS §4)
#     must pass;
#   - each mutation MUST be caught natively — a clean mutated run is
#     TOOLING-FAIL and exits != 0;
#   - mutations touch the working tree only briefly and are restored to
#     a snapshot-verified state.
#
# Result vocabulary follows issue #124. A green run states BOUNDED-CLEAN
# under the harness bounds in RESULTS.md — never "the architecture is
# proven correct".

set -uo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"

KERNEL="crates/qianqian-composition/src/kernel.rs"
DIR="specs/composition-kernel-0-rust"

fail=0
if [[ -n "$(git status --porcelain -- "$KERNEL")" ]]; then
  echo "TOOLING-FAIL: $KERNEL is dirty; refusing to run mutations" >&2
  exit 2
fi
KERNEL_SNAPSHOT="$(mktemp)"
cp "$KERNEL" "$KERNEL_SNAPSHOT"

echo "== harness matrices, native channel =="
if cargo test -p qianqian-composition --lib verify:: > /dev/null 2>&1; then
  echo "RESULT matrices(native) BOUNDED-CLEAN"
else
  echo "RESULT matrices(native) FAILED"
  fail=1
fi

echo "== §G.6 cross-locus refinement oracle (integration; teardown-closure locus coverage) =="
# CompositionKernel0 RESULTS §4: the teardown-closure empty-accumulator
# latch locus is NOT separately modeled in TLA; this integration oracle is
# its production refinement witness and is therefore gate-wired here.
# Fail closed on BOTH "no ok line" and "filter matched nothing".
ORACLE_LOG="$(mktemp)"
if cargo test -p qianqian-composition --test lifecycle_oracles \
    violation_latch_semantic_family_is_locus_invariant > "$ORACLE_LOG" 2>&1 \
  && grep -q "violation_latch_semantic_family_is_locus_invariant ... ok" "$ORACLE_LOG" \
  && grep -q "test result: ok" "$ORACLE_LOG"; then
  echo "RESULT §G.6 cross-locus oracle TEST-PASS"
else
  echo "RESULT §G.6 cross-locus oracle FAILED"
  tail -5 "$ORACLE_LOG"
  fail=1
fi
rm -f "$ORACLE_LOG"

echo "== harness matrices, Miri channel (UB/leak/overflow) =="
if command -v cargo +nightly miri > /dev/null 2>&1 || cargo +nightly miri --version > /dev/null 2>&1; then
  if cargo +nightly miri test -p qianqian-composition --lib verify:: > /dev/null 2>&1; then
    echo "RESULT matrices(miri) MIRI-CLEAN"
  else
    echo "RESULT matrices(miri) FAILED"
    fail=1
  fi
else
  echo "TOOLING-FAIL: nightly miri toolchain not available" >&2
  fail=1
fi

echo "== negative controls (known-bad production mutations MUST be caught) =="
ctl() {
  local patch="$1" harness="$2" name log
  name="${patch%.patch}"
  log="$(mktemp)"
  if ! git apply --whitespace=nowarn "$DIR/mutations/$patch"; then
    echo "TOOLING-FAIL: mutation $name does not apply" >&2
    fail=1
    return
  fi
  if cargo test -p qianqian-composition --lib "verify::$harness" > "$log" 2>&1; then
    echo "RESULT $name TOOLING-FAIL (mutation was NOT caught)"
    fail=1
  elif grep -q "test result: FAILED" "$log"; then
    echo "RESULT $name COUNTEREXAMPLE-WITNESSED"
  else
    echo "TOOLING-FAIL: mutation $name run errored unexpectedly" >&2
    tail -3 "$log" >&2
    fail=1
  fi
  rm -f "$log"
  git apply -R --whitespace=nowarn "$DIR/mutations/$patch"
  if ! cmp -s "$KERNEL" "$KERNEL_SNAPSHOT"; then
    cp "$KERNEL_SNAPSHOT" "$KERNEL"
    echo "TOOLING-FAIL: kernel.rs did not restore to the expected state" >&2
    fail=1
  fi
}
ctl M-K1IgnoreGeneration.patch k1_stale_fiber_id_never_addresses_a_reused_slot
ctl M-K2DropReliedGuard.patch k2_relied_provider_is_never_removed_behind_an_open_committed_view
ctl M-K3NoOverlapGuard.patch k6_single_source_survives_replacement_and_violation

rm -f "$KERNEL_SNAPSHOT"
if [[ "$fail" -eq 0 ]]; then
  echo "SUITE: BOUNDED-CLEAN (matrices clean natively + Miri; §G.6 cross-locus oracle green; all mutations caught)"
else
  echo "SUITE: FAILED (see RESULT lines above)"
fi
exit "$fail"
