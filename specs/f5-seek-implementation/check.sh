#!/usr/bin/env bash
# specs/f5-seek-implementation/check.sh — F5 seek IMPLEMENTATION mutation
# gate (QIANQIAN-F5-SEEK-IMPLEMENTATION-1 + its correctives 1 and 3; the
# corrective-2 classification pin lives in the decode crate's own tests).
#
# The guardrails M1–M7 correspond to the F5-GATE formal suite's
# mutations (specs/f5-seek-discontinuity/mutations, TLA+). There each was
# a model mutation; here each is applied to the PRODUCTION Rust code and
# MUST be caught by the executable suites — the proof that the same
# guardrail is load-bearing in the shipped mechanism, not only in the
# model:
#
#   M1 CommitBeforeTailPurge      the cutover decision drops the park ∧
#                                 quiesced conjunct (completion.rs)
#                                  → caught by the white-box commit-
#                                 conjunction test (a decision without ANY
#                                 engagement evidence is Pending, never a
#                                 commit).
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
#   M5 CommitBeforeLanding        the worker's commit point precedes the
#                                 landing evidence's publication
#                                 (session.rs; re-pinned by corrective-3:
#                                 with an idempotent three-valued decision
#                                 a premature call is a Pending no-op, so
#                                 the guardrail is pinned as the program-
#                                 order violation it names — publishing
#                                 the landing only after the wait leaves
#                                 the boundary permanently unsatisfiable)
#                                  → caught by the forward-seek matrix
#                                 (the commit never happens → the cut
#                                 never lands → the matrix's bounded
#                                 landing wait times out on its own
#                                 assert, not the harness bound).
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
# M8 is corrective-1's own guardrail (current-cut attribution): the
#   per-cut evidence latches are reset when a new seek is accepted;
#   deleting the reset lets a second commit ride the FIRST seek's
#   landing evidence (completion.rs)
#    → caught by the white-box second-seek evidence oracle (a_second_
#   accepted_seek_gets_fresh_cut_evidence must see fresh latches and
#   refuse a commit without cycle 2's own landing).
#
# M9 and M10 are corrective-3's own guardrails (the cut's liveness and
# the lost-rebase race):
#   M9  WaitIgnoresDataPlane: the applied cut's wait stops reading the
#       data plane's terminal, so it can only exit on the session's own
#       endings — which teardown records AFTER the worker join (session.rs)
#        → caught by the never-draining-device teardown oracle (the join
#       never returns → the harness bound fires).
#   M10 PendingRoutesAbort: a decision sample without the park/quiescence
#       evidence is classified as an episode ending and routes an abort
#       release (completion.rs)
#        → caught by the park-handover gap oracle (a decision taken in the
#       leg's pause→cut handover must be Pending and route nothing; the
#       abort release there resumes a purged cut's leg with its PRE-CUT
#       position accounting).
#
# Fail-closed contract:
#   - the native regression (playback + audio-api crates) must come back
#     green and non-empty, else exit != 0;
#   - each mutation must APPLY, must be CAUGHT (its targeted suite goes
#     red — a green mutated run is TOOLING-FAIL), and the touched files
#     must restore byte-exact;
#   - refuses to run on a dirty touched file.
#
# Result vocabulary follows issue #124. A green run states that the ten
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
# A mutation is applied to the working tree: an interrupt (or an
# unexpected exit) must never leave one behind. The snapshot is the
# pre-run state, so restoring on exit is exactly the intended end state.
trap restore EXIT INT TERM

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
  # Bounded so a mutated run that DEADLOCKS (the liveness guardrail's own
  # failure mode) fails closed as "not caught" instead of hanging the
  # gate; the bound is far above the slowest targeted suite.
  timeout 900 cargo test "$@" > "$log" 2>&1
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

run_mutation M8StaleCutEvidence M8StaleCutEvidence.patch \
  -p qianqian-playback --lib \
  completion::tests::a_second_accepted_seek_gets_fresh_cut_evidence

run_mutation M9WaitIgnoresDataPlane M9WaitIgnoresDataPlane.patch \
  -p qianqian-playback --test seek_seam \
  a_cut_over_a_never_draining_device_still_tears_down

run_mutation M10PendingRoutesAbort M10PendingRoutesAbort.patch \
  -p qianqian-playback --lib \
  completion::tests::a_park_handover_evidence_gap_is_pending_and_never_an_abort

if [[ "$fail" -eq 0 ]]; then
  echo "SUITE: GUARDRAILS-LOAD-BEARING (native green; all 10 mutations caught)"
else
  echo "SUITE: FAILED (see RESULT lines above)"
fi
exit "$fail"
