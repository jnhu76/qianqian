#!/usr/bin/env bash
# FV-RUST-0 runner: bounded model checking of the real qianqian-composition
# kernel (campaign QIANQIAN-VERIFICATION-CAMPAIGN-1, Phase B1).
#
# Fail-closed contract:
#   - every K harness must come back VERIFICATION SUCCESSFUL, else exit != 0;
#   - every mutation (negative control) MUST be caught — a clean mutation
#     run is TOOLING-FAIL and exits != 0;
#   - mutations are applied to the working tree only briefly and restored
#     immediately; the script refuses to run on a dirty kernel.rs and
#     verifies restoration afterwards.
#
# Result vocabulary follows issue #124. A green run states
# BOUNDED-CLEAN under the harness bounds in specs/composition-kernel-0-rust/
# RESULTS.md — never "the architecture is proven correct".

set -uo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"

CRATE="qianqian-composition"
KERNEL="crates/qianqian-composition/src/kernel.rs"
DIR="specs/composition-kernel-0-rust"

HARNESS_k1="k1_stale_fiber_id_never_addresses_a_reused_slot"
HARNESS_k2="k2_relied_provider_is_never_removed_behind_an_open_committed_view"
HARNESS_k3="k3_effect_inverses_fire_at_most_once_in_lifo_order"
HARNESS_k4="k4_removed_fibers_leave_nothing_owed"
HARNESS_k5="k5_quiet_is_the_fixed_point_of_step"
HARNESS_k6="k6_single_source_survives_replacement_and_violation"

# mutation -> (patch, harness that must catch it)
MUTATIONS=(
  "M-K1IgnoreGeneration.patch $HARNESS_k1"
  "M-K2DropReliedGuard.patch  $HARNESS_k2"
  "M-K3NoOverlapGuard.patch   $HARNESS_k6"
)

fail=0
restore_kernel() {
  if ! git apply -R --whitespace=nowarn "$DIR/mutations/$1" 2>/dev/null; then
    git checkout -- "$KERNEL"
  fi
}

if [[ -n "$(git status --porcelain -- "$KERNEL")" ]]; then
  echo "TOOLING-FAIL: $KERNEL is dirty; refusing to run mutations" >&2
  exit 2
fi

echo "== K harness suite (real production kernel + cfg(kani) harnesses) =="
for key in k1 k2 k3 k4 k5 k6; do
  h="HARNESS_$key"
  echo "--- ${!h}"
  if cargo kani -p "$CRATE" --harness "${!h}"; then
    echo "RESULT $key BOUNDED-CLEAN"
  else
    echo "RESULT $key FAILED (counterexample or harness/tooling error — classify before touching production)"
    fail=1
  fi
done

echo "== Negative controls (known-bad production mutations MUST be caught) =="
for entry in "${MUTATIONS[@]}"; do
  patch="${entry%% *}"
  harness="${entry##* }"
  name="${patch%.patch}"
  if ! git apply --whitespace=nowarn "$DIR/mutations/$patch"; then
    echo "TOOLING-FAIL: mutation $name does not apply" >&2
    fail=1
    continue
  fi
  if cargo kani -p "$CRATE" --harness "$harness"; then
    echo "RESULT $name TOOLING-FAIL (mutation was NOT caught — harness has no teeth)"
    fail=1
  else
    echo "RESULT $name COUNTEREXAMPLE-WITNESSED"
  fi
  restore_kernel "$patch"
  if [[ -n "$(git status --porcelain -- "$KERNEL")" ]]; then
    echo "TOOLING-FAIL: kernel.rs failed to restore" >&2
    git checkout -- "$KERNEL"
    fail=1
  fi
done

if [[ "$fail" -eq 0 ]]; then
  echo "SUITE: BOUNDED-CLEAN (all K harnesses clean; all mutations caught)"
else
  echo "SUITE: FAILED (see per-item RESULT lines above)"
fi
exit "$fail"
