#!/usr/bin/env bash
# specs/f5-seek-implementation/check.sh — F5 seek IMPLEMENTATION mutation
# gate (QIANQIAN-F5-SEEK-IMPLEMENTATION-1).
#
# The seven guardrails M1–M7 correspond to the F5-GATE formal suite's
# mutations (specs/f5-seek-discontinuity/mutations, TLA+). There each was
# a model mutation; here each is applied to the PRODUCTION Rust code and
# MUST be caught by the executable suites — the proof that the same
# guardrail is load-bearing in the shipped mechanism, not only in the
# model:
#
#   M1 CommitBeforeTailPurge      commit_seek_cutover drops the park ∧
#                                 quiesced conjunct (completion.rs)
#                                  → caught by the white-box commit-
#                                 conjunction test (commit without ANY
#                                 engagement evidence must refuse).
#   M2 SeekMidWritePurgeFirst     the ONE purge fires before the provider
#                                 outcome exists (session.rs)
#                                  → caught by the refusal zero-loss
#                                 matrix (a purge before RefusedUnchanged
#                                 loses pre-cut content: discontinuity in
#                                 a refusal).
#   M3 ParkHoldUnrouted           the cut's hold is never routed to the
#                                 render gate (completion.rs)
#                                  → caught by the forward-seek matrix
#                                 (the commit boundary is unreachable;
#                                 the cut either never happens or the
#                                 content oracle sees no single cut).
#   M4 StalePositionWriter        the position cell's rebase becomes a
#                                 monotone RMW — the one legal backward
#                                 step disappears (ports.rs)
#                                  → caught by the position algebra
#                                 (rebase-to-lower must lower the sample).
#   M5 CommitBeforeLanding        the worker commits before the landing
#                                 evidence is published (session.rs)
#                                  → caught by the forward-seek matrix
#                                 (the commit refuses → abort release →
#                                 the projection never rebases).
#   M6 RefusalDropsRemainder      the refusal discards the preserved
#                                 unwritten tail (session.rs)
#                                  → caught by the refusal zero-loss
#                                 matrix (frame-exact no-seek control).
#   M7 ResumeAfterMutatedSeek     the worker continues old-cursor
#                                 production after a destructive provider
#                                 failure (session.rs)
#                                  → caught by the destructive matrix
#                                 (production must stop for good at the
#                                 Failed terminal).
#
# Fail-closed contract:
#   - the native regression (playback + audio-api crates) must come back
#     green and non-empty, else exit != 0;
#   - each mutation must APPLY, must be CAUGHT (its targeted suite goes
#     red — a green mutated run is TOOLING-FAIL), and the touched files
#     must restore byte-exact;
#   - refuses to run on a dirty touched file.
#
# Result vocabulary follows issue #124. A green run states that the seven
# guardrails are load-bearing in the implementation under the pinned
# oracles — not a general proof of the seek semantics (that authority is
# ADR-PBK-002 §20 D14.5; the model-level evidence is the f5-seek-
# discontinuity suite).

set -uo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"

MUTDIR="specs/f5-seek-implementation/mutations"

# The union of files the mutations touch: the whole set must be clean
# and is snapshotted, then restored as a whole around every mutation.
FILES=(
  "crates/qianqian-playback/src/completion.rs"
  "crates/qianqian-playback/src/session.rs"
  "crates/qianqian-audio-api/src/ports.rs"
)

for f in "${FILES[@]}"; do
  if [[ -n "$(git status --porcelain -- "$f")" ]]; then
    echo "TOOLING-FAIL: $f is dirty; refusing to run" >&2
    exit 2
  fi
done

SNAPSHOT="$(mktemp -d)"
for f in "${FILES[@]}"; do
  mkdir -p "$SNAPSHOT/$(dirname "$f")"
  cp "$f" "$SNAPSHOT/$f"
done
restore() {
  for f in "${FILES[@]}"; do
    cp "$SNAPSHOT/$f" "$f"
  done
}

fail=0

echo "== native regression (playback + audio-api crates) =="
# Fail closed on ANY failure AND on an emptied suite: at least one target
# must report a positive pass count (guards a silent no-op green).
NATIVE_LOG="$(mktemp)"
if cargo test -p qianqian-playback -p qianqian-audio-api > "$NATIVE_LOG" 2>&1 \
  && ! grep -q "test result: FAILED" "$NATIVE_LOG" \
  && grep -qE "test result: ok\. [1-9][0-9]* passed" "$NATIVE_LOG"; then
  echo "RESULT native BOUNDED-CLEAN"
else
  echo "RESULT native FAILED"
  tail -5 "$NATIVE_LOG"
  fail=1
fi
rm -f "$NATIVE_LOG"

# run_mutation NAME PATCH TEST_ARGS... — apply, expect the targeted
# suite RED, revert, verify byte-exact restore.
run_mutation() {
  local name="$1"; shift
  local patch="$MUTDIR/$1"; shift
  echo "== mutation $name =="
  if ! git apply --whitespace=nowarn "$patch"; then
    echo "RESULT $name TOOLING-FAIL (mutation does not apply)" >&2
    fail=1
    restore
    return
  fi
  local log
  log="$(mktemp)"
  cargo test "$@" > "$log" 2>&1
  if grep -q "test result: FAILED" "$log"; then
    echo "RESULT $name COUNTEREXAMPLE-WITNESSED (guardrail caught)"
  else
    echo "RESULT $name TOOLING-FAIL (mutation NOT caught)"
    tail -5 "$log"
    fail=1
  fi
  rm -f "$log"
  git apply -R --whitespace=nowarn "$patch"
  for f in "${FILES[@]}"; do
    if ! cmp -s "$f" "$SNAPSHOT/$f"; then
      echo "TOOLING-FAIL: $f did not restore to the expected state" >&2
      fail=1
    fi
  done
  restore
}

run_mutation M1CommitBeforeTailPurge M1CommitBeforeTailPurge.patch \
  -p qianqian-playback --lib \
  completion::tests::the_commit_boundary_requires_its_full_conjunction

run_mutation M2SeekMidWritePurgeFirst M2SeekMidWritePurgeFirst.patch \
  -p qianqian-playback --test seek_seam \
  a_refused_seek_finishes_its_own_remainder_with_zero_content_loss

run_mutation M3ParkHoldUnrouted M3ParkHoldUnrouted.patch \
  -p qianqian-playback --test seek_seam \
  a_committed_forward_seek_jumps_the_content_exactly_at_the_actual_landing

run_mutation M4StalePositionWriter M4StalePositionWriter.patch \
  -p qianqian-audio-api --test position_evidence \
  a_committed_rebase_is_the_one_legal_backward_step

run_mutation M5CommitBeforeLanding M5CommitBeforeLanding.patch \
  -p qianqian-playback --test seek_seam \
  a_committed_forward_seek_jumps_the_content_exactly_at_the_actual_landing

run_mutation M6RefusalDropsRemainder M6RefusalDropsRemainder.patch \
  -p qianqian-playback --test seek_seam \
  a_refused_seek_finishes_its_own_remainder_with_zero_content_loss

run_mutation M7ResumeAfterMutatedSeek M7ResumeAfterMutatedSeek.patch \
  -p qianqian-playback --test seek_seam \
  a_destructive_provider_failure_fails_the_episode_and_never_resumes

if [[ "$fail" -eq 0 ]]; then
  echo "SUITE: GUARDRAILS-LOAD-BEARING (native green; all 7 mutations caught)"
else
  echo "SUITE: FAILED (see RESULT lines above)"
fi
exit "$fail"
