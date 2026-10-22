#!/usr/bin/env bash
# Gate II — H1 M1–M9 mutation admission (issue #308; frozen semantics:
# APP-C target table, run on the H1 bounded-channel mechanism).
#
# Arms, each a SEPARATE OS process in a FRESH copy of the crate:
#   A   clean copy            → harness must PASS (oracle GREEN)
#   M1..M9  patched copies    → harness must FAIL, and the mutant's
#                               designated oracle check must be among
#                               the FAILs
#   C   fresh clean copy      → harness must PASS
#
# Patch provenance (GATE-II-MECHANISM-MAP.md §3): M2/M6/M8/M9 are the
# SHARED session-core mutants — the very same patch files the H0
# campaign applies to src/session_core.rs (the A_NARROW shared business
# function IS H1's real execution path); M1/M3/M4/M5a-d/M7 are the H1
# dispatch-path mutants against src/h1.rs. There are no LW/PB arms here:
# those kill H0's own fence/permit defenses, which are H0 mechanism
# surface; H1's channel defenses get the dedicated controls in
# src/h1.rs tests and the §15 error classes.
#
# The main worktree is never modified; run artifacts go to a UNIQUE
# UNTRACKED run dir (GATE_II_H1_MUTATION_OUT overrides); scratch is
# unique per run (Gate II GII-G) and removed on success, kept on
# failure. A non-blocking flock fails closed on a concurrent campaign
# of this same driver.
#
# rc: 0 = all arms MET; 1 = at least one arm VIOLATED.
set -u

CRATE="$(cd "$(dirname "$0")/.." && pwd)"
EV="${GATE_II_H1_MUTATION_OUT:-${TMPDIR:-/tmp}/wrk-scan-h1-mutations-$$-$(date +%s%N)}"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/wrk-scan-h1-mutation-arms-$$-XXXXXX")"
mkdir -p "$EV" "$WORK"
echo "mutation run dir: $EV"
echo "scratch dir: $WORK"
export CARGO_TARGET_DIR="$WORK/shared-target"
ROUND="${ROUND:-}"

LOCKFILE="${TMPDIR:-/tmp}/wrk-scan-h1-mutation-campaign.lock"
exec 9>"$LOCKFILE"
if ! flock -n 9; then
  echo "FLOCK-HELD: another H1 mutation campaign is already running ($LOCKFILE) — refusing to start (fail-closed)" | tee -a "$EV/cycle-summary$ROUND.txt"
  exit 1
fi

SUMMARY="$EV/cycle-summary$ROUND.txt"
: > "$SUMMARY"
VIOLATIONS=0

# designated primary oracle check per mutant (APP-C §C.6 table)
designated() {
  case "$1" in
    M1) echo "O1_terminal_coverage" ;;
    M2) echo "O2_exactly_once_effects" ;;
    M3) echo "O4_no_effect_after_item_terminal" ;;
    M4) echo "O5_abandon_legitimacy" ;;
    M5) echo "O6_executors_joined_before_quiescence" ;;
    M6) echo "O3_content_vs_ref" ;;
    M7) echo "O7_no_admission_after_cancel" ;;
    M8) echo "O2_exactly_once_effects" ;;
    M9) echo "O3_content_vs_ref" ;;
    *) echo "" ;;
  esac
}

arm_filter() {
  echo "--test mutation_harness_h1"
}

# Where an arm's patch files live: shared session-core mutants sit in
# mutations/, H1 dispatch-path mutants in mutations/h1/.
arm_patch_dir() {
  case "$1" in
    M2|M6|M8|M9) echo "$CRATE/mutations" ;;
    *) echo "$CRATE/mutations/h1" ;;
  esac
}

fresh_copy() {
  local dst=$1
  local root
  root="$(cd "$CRATE/../.." && pwd)"
  rm -rf "$dst"
  mkdir -p "$dst/crates" "$dst/research"
  (cd "$root" && tar -cf - crates/qianqian-composition) | (cd "$dst" && tar -xf -)
  (cd "$root" && tar -cf - \
    --exclude='research/wrk-scan/target' \
    --exclude='research/wrk-scan/evidence' \
    research/wrk-scan) | (cd "$dst" && tar -xf -)
  cat > "$dst/Cargo.toml" <<'TOML'
[workspace]
resolver = "3"

[workspace.package]
edition = "2024"
version = "0.1.0"
TOML
}

run_arm() { # $1 name, $2 dir, $3 expect(green|red)
  local name=$1 dir=$2 expect=$3 log="$EV/arm-$ROUND$1.log"
  (cd "$dir/research/wrk-scan" && sha256sum Cargo.toml Cargo.lock src/*.rs tests/*.rs \
      > "$EV/arm-$ROUND$1.source-sha.txt")
  (cd "$dir/research/wrk-scan" && timeout 300 cargo test $(arm_filter "$name") -- --nocapture) > "$log" 2>&1
  local rc=$?
  local verdict
  verdict=$(grep -o '^VERDICT = .*' "$log" | head -1)
  if [ "$expect" = green ]; then
    if [ "$rc" -eq 0 ]; then
      echo "$name rc=$rc MET ($verdict)" >> "$SUMMARY"
    else
      echo "$name rc=$rc VIOLATION (clean arm failed)" >> "$SUMMARY"
      VIOLATIONS=$((VIOLATIONS + 1))
    fi
  else
    local primary failed hit
    primary=$(designated "$name")
    failed=$(grep '^  .* FAIL$' "$log" | awk '{print $1}' | tr '\n' ' ')
    hit=$(grep -c "^  $primary  *FAIL$" "$log")
    if [ "$rc" -ne 0 ] && [ "$hit" -gt 0 ]; then
      echo "$name rc=$rc MET (RED; failed: ${failed:-none})" >> "$SUMMARY"
    elif [ "$rc" -eq 0 ]; then
      echo "$name rc=$rc VIOLATION (mutant ESCAPED: oracle GREEN)" >> "$SUMMARY"
      VIOLATIONS=$((VIOLATIONS + 1))
    else
      echo "$name rc=$rc VIOLATION (RED but designated '$primary' not in log; failed: ${failed})" >> "$SUMMARY"
      VIOLATIONS=$((VIOLATIONS + 1))
    fi
  fi
}

# ---- arm A: clean tree ----
fresh_copy "$WORK/clean-a"
run_arm A-green "$WORK/clean-a" green

# ---- arm B: mutants ----
for m in M1 M2 M3 M4 M5 M6 M7 M8 M9; do
  dir="$WORK/$m"
  fresh_copy "$dir"
  pdir="$(arm_patch_dir "$m")"
  applied=0
  for p in "$pdir/$m"*.patch; do
    if (cd "$dir/research/wrk-scan" && patch --forward --silent -p1 < "$p"); then
      applied=$((applied + 1))
    else
      echo "$m PATCH-FAILED on $p" >> "$SUMMARY"
      VIOLATIONS=$((VIOLATIONS + 1))
      applied=-1
      break
    fi
  done
  [ "$applied" -lt 0 ] && continue
  run_arm "$m" "$dir" red
done

# ---- arm C: fresh clean tree ----
fresh_copy "$WORK/clean-c"
run_arm C-green "$WORK/clean-c" green

echo "rc_legend: MET = green arm green / mutant arm RED with designated check" >> "$SUMMARY"
echo "total violations: $VIOLATIONS" >> "$SUMMARY"

# ---- main-worktree byte-identity witness (the driver never edits it) ----
git -C "$CRATE" status --porcelain -- src tests mutations | grep -v '^??' > "$EV/worktree-untouched.txt" || true
echo "worktree src/tests/mutations modifications: $(wc -l < "$EV/worktree-untouched.txt") entries (0 = untouched)" >> "$SUMMARY"

cat "$SUMMARY"
if [ "$VIOLATIONS" -eq 0 ]; then
  rm -rf "$WORK"
  exit 0
fi
echo "scratch kept for forensics: $WORK" >> "$SUMMARY"
exit 1
