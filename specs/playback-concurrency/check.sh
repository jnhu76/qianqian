#!/usr/bin/env bash
# FV-CONC-0 runner: Loom model checking of the real PcmEdge (campaign
# QIANQIAN-VERIFICATION-CAMPAIGN-1, Phase B2).
#
# Fail-closed contract:
#   - the loom model suite must come back green, else exit != 0;
#   - the negative control (dropped data_ready notify) MUST be caught by
#     a loom deadlock/failed schedule — a clean mutated run is
#     TOOLING-FAIL and exits != 0;
#   - the mutation is applied to the working tree only briefly and
#     restored immediately; refuses to run on a dirty edge.rs.
#
# Result vocabulary follows issue #124. A green run states SCHEDULE-CLEAN
# within the stated thread/op bounds (see RESULTS.md) — not a general
# proof.

set -uo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"

EDGE="crates/qianqian-playback/src/edge.rs"
PATCH="specs/playback-concurrency/mutations/M-L1DropDataReadyNotify.patch"

fail=0
if [[ -n "$(git status --porcelain -- "$EDGE")" ]]; then
  echo "TOOLING-FAIL: $EDGE is dirty; refusing to run" >&2
  exit 2
fi
EDGE_SNAPSHOT="$(mktemp)"
cp "$EDGE" "$EDGE_SNAPSHOT"

echo "== native regression (playback crate) =="
# Fail closed on ANY failure AND on an emptied suite: at least one target
# must report a positive pass count (guards a silent no-op green).
NATIVE_LOG="$(mktemp)"
if cargo test -p qianqian-playback > "$NATIVE_LOG" 2>&1 \
  && ! grep -q "test result: FAILED" "$NATIVE_LOG" \
  && grep -qE "test result: ok\. [1-9][0-9]* passed" "$NATIVE_LOG"; then
  echo "RESULT native BOUNDED-CLEAN"
else
  echo "RESULT native FAILED"
  tail -5 "$NATIVE_LOG"
  fail=1
fi

echo "== loom model suite (real PcmEdge, --cfg loom) =="
if RUSTFLAGS="--cfg loom" cargo test -p qianqian-playback --features loom --release --test loom_edge; then
  echo "RESULT loom SCHEDULE-CLEAN"
else
  echo "RESULT loom FAILED"
  fail=1
fi

echo "== negative control: drop data_ready notify (M-L1) =="
if ! git apply --whitespace=nowarn "$PATCH"; then
  echo "TOOLING-FAIL: mutation does not apply" >&2
  fail=1
else
  # The mutated run is EXPECTED to fail (deadlock schedule). Note: pipefail
  # would invert a grep-on-pipe here, so capture to a file first.
  MUT_LOG="$(mktemp)"
  RUSTFLAGS="--cfg loom" cargo test -p qianqian-playback --features loom --release --test loom_edge > "$MUT_LOG" 2>&1
  if grep -qE "deadlock|test result: FAILED" "$MUT_LOG"; then
    echo "RESULT M-L1 COUNTEREXAMPLE-WITNESSED (lost wakeup found)"
  else
    echo "RESULT M-L1 TOOLING-FAIL (mutation NOT caught)"
    fail=1
  fi
  rm -f "$MUT_LOG"
  git apply -R --whitespace=nowarn "$PATCH"
  if ! cmp -s "$EDGE" "$EDGE_SNAPSHOT"; then
    cp "$EDGE_SNAPSHOT" "$EDGE"
    echo "TOOLING-FAIL: edge.rs did not restore to the expected state" >&2
    fail=1
  fi
fi

if [[ "$fail" -eq 0 ]]; then
  echo "SUITE: SCHEDULE-CLEAN (models green; mutation caught)"
else
  echo "SUITE: FAILED (see RESULT lines above)"
fi
exit "$fail"
