#!/usr/bin/env bash
# QIANQIAN #321 — analyst-panic x blocked-stderr E2E replay (Linux only).
#
# The product tree contains no panic-injection seam. This script creates two
# SCRATCH worktrees (baseline and fix), copies the evidence harness into each,
# applies the evidence-only overlay (an env-gated panic INSIDE the real protected
# analyst region), and runs the same oracle under both source shapes:
#
#   baseline  (no analyst class-alpha registration)  -> Wedged
#   fix       (registration before the protected region) -> Completed/Discharged
#
# It also runs the healthy, broken-stderr, and blocked-sink-without-panic
# controls, plus an ordering negative control that moves the registration after
# the protected region (must wedge again).
#
# Usage: ./replay.sh [baseline-rev] [fix-rev]
#   defaults: the pre-#322 main commit 61d834a (defect present) and HEAD
#   (the checkout must contain the fix). Do NOT default the baseline to
#   origin/main after #322 merges: it would then be fix-vs-fix.
# Environment: OUT=<log dir> (default: a temp dir printed at the end)
#
# The script never writes to the product worktree and never touches the network.
# Evidence class: executed Linux subprocess witness. Windows runtime is NOT_RUN.

set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(git -C "$HERE" rev-parse --show-toplevel)"
BASELINE_REV="${1:-61d834ad63bd5b0195496cf9de3a77395dd12a71}"
FIX_REV="${2:-HEAD}"

TMP="$(mktemp -d "${TMPDIR:-/tmp}/qianqian-analyst-join-321.XXXXXX")"
OUT="${OUT:-$TMP/logs}"
mkdir -p "$OUT"

archived=0
archive_logs() {
    [ "$archived" = 1 ] && return 0
    archived=1
    if [ -d "$OUT" ] && [[ "$OUT" == "$TMP"/* ]]; then
        local dest="${TMP}-logs"
        rm -rf "$dest"
        mv "$OUT" "$dest"
        OUT="$dest"
    fi
}

fail() {
    archive_logs
    echo "REPLAY_RESULT=FAIL: $*" >&2
    echo "logs: $OUT" >&2
    exit 1
}

cleanup() {
    set +e
    archive_logs
    git -C "$REPO" worktree remove --force "$TMP/baseline" >/dev/null 2>&1
    git -C "$REPO" worktree remove --force "$TMP/fix" >/dev/null 2>&1
    rm -rf "$TMP"
}
trap cleanup EXIT

prepare() {
    local rev="$1" dir="$2"
    git -C "$REPO" worktree add --detach "$dir" "$rev" >/dev/null
    cp "$HERE/harness/analyst_join_e2e_tests.rs" "$dir/apps/headless/src/"
    git -C "$dir" apply "$HERE/overlay/000-harness.patch"
}

# run <dir> <expect> <log> <exact-test>
run() {
    local dir="$1" expect="$2" log="$3" test="$4"
    local rc=0
    # A SHARED target dir lets Cargo reuse the other checkout's artifact for the
    # same crate/feature set, which would silently run the baseline binary in the
    # fix worktree. Keep one target per checkout; the negative control reuses the
    # fix target because it changes source inside the same path.
    local target="$TMP/target-$(basename "$dir")"
    (
        cd "$dir"
        CARGO_TARGET_DIR="$target" \
            QIANQIAN_ANALYST_JOIN_EXPECT="$expect" \
            timeout 600 cargo test -p qianqian-headless --lib -- "$test" --nocapture
    ) >"$log" 2>&1 || rc=$?
    if [ "$rc" -ne 0 ]; then
        tail -60 "$log" >&2
        fail "$test exited $rc (expect=$expect); log=$log"
    fi
}

require() {
    local log="$1" needle="$2"
    grep -Fq "$needle" "$log" || {
        tail -60 "$log" >&2
        fail "missing '$needle' in $log"
    }
}

forbid() {
    local log="$1" needle="$2"
    if grep -Fq "$needle" "$log"; then
        tail -60 "$log" >&2
        fail "unexpected '$needle' in $log"
    fi
}

cat >"$OUT/ENVIRONMENT.txt" <<EOF
utc=$(date -u +%Y-%m-%dT%H:%M:%SZ)
repo=$REPO
baseline_rev=$(git -C "$REPO" rev-parse "$BASELINE_REV")
fix_rev=$(git -C "$REPO" rev-parse "$FIX_REV")
rustc=$(rustc --version)
host=$(uname -a)
EOF

echo "=== preparing scratch worktrees ==="
prepare "$BASELINE_REV" "$TMP/baseline"
prepare "$FIX_REV" "$TMP/fix"

echo "=== baseline: blocked stderr x analyst panic (expect Wedged) ==="
run "$TMP/baseline" wedge "$OUT/01-baseline-wedge.log" \
    analyst_join_e2e_blocked_stderr_oracle
require "$OUT/01-baseline-wedge.log" "ORACLE observed=Wedged"
require "$OUT/01-baseline-wedge.log" "PREVIOUS-HOOK-ENTERED-ANALYST"
require "$OUT/01-baseline-wedge.log" "TERMINAL=Completed"
forbid "$OUT/01-baseline-wedge.log" "DISPOSE-RETURNED"

echo "=== fix: blocked stderr x analyst panic (expect Completed) ==="
run "$TMP/fix" complete "$OUT/02-fix-complete.log" \
    analyst_join_e2e_blocked_stderr_oracle
require "$OUT/02-fix-complete.log" "ORACLE observed=Completed"
require "$OUT/02-fix-complete.log" "DISPOSE-RETURNED verdict=Discharged"
require "$OUT/02-fix-complete.log" "OUTPUT-EFFECT-DISCHARGED=true"
require "$OUT/02-fix-complete.log" "CRITICAL-PANICS=1"
forbid "$OUT/02-fix-complete.log" "PREVIOUS-HOOK-ENTERED-ANALYST"

echo "=== fix control: healthy ==="
run "$TMP/fix" complete "$OUT/03-healthy.log" \
    analyst_join_e2e_healthy_control
require "$OUT/03-healthy.log" "DISPOSE-RETURNED verdict=Discharged"
forbid "$OUT/03-healthy.log" "PREVIOUS-HOOK-ENTERED"

echo "=== fix control: broken stderr (EBADF) x panic ==="
run "$TMP/fix" complete "$OUT/04-broken-stderr-panic.log" \
    analyst_join_e2e_broken_stderr_panic_still_discharges
require "$OUT/04-broken-stderr-panic.log" "DISPOSE-RETURNED verdict=Discharged"
forbid "$OUT/04-broken-stderr-panic.log" "PREVIOUS-HOOK-ENTERED-ANALYST"

echo "=== fix control: blocked sink WITHOUT panic ==="
run "$TMP/fix" complete "$OUT/05-blocked-sink-no-panic.log" \
    analyst_join_e2e_blocked_sink_without_panic_discharges
require "$OUT/05-blocked-sink-no-panic.log" "DISPOSE-RETURNED verdict=Discharged"
require "$OUT/05-blocked-sink-no-panic.log" "CRITICAL-PANICS=0"

echo "=== negative control: registration AFTER the protected region ==="
git -C "$TMP/fix" apply "$HERE/overlay/001-negative-control-registration-after-panic.patch"
run "$TMP/fix" wedge "$OUT/06-negative-control-wedge.log" \
    analyst_join_e2e_blocked_stderr_oracle
require "$OUT/06-negative-control-wedge.log" "ORACLE observed=Wedged"
require "$OUT/06-negative-control-wedge.log" "PREVIOUS-HOOK-ENTERED-ANALYST"
forbid "$OUT/06-negative-control-wedge.log" "DISPOSE-RETURNED"

echo
echo "=== summary ==="
echo "baseline wedge          PASS ($OUT/01-baseline-wedge.log)"
echo "fix complete/discharged PASS ($OUT/02-fix-complete.log)"
echo "healthy control         PASS ($OUT/03-healthy.log)"
echo "broken-stderr control   PASS ($OUT/04-broken-stderr-panic.log)"
echo "blocked no-panic        PASS ($OUT/05-blocked-sink-no-panic.log)"
echo "ordering negative       PASS ($OUT/06-negative-control-wedge.log)"
echo "REPLAY_RESULT=PASS"
archive_logs
echo "logs kept in $OUT"
