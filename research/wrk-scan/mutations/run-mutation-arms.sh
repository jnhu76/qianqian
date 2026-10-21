#!/usr/bin/env bash
# Gate I — H0 M1–M9 mutation admission (frozen semantics: APP-C).
#
# Arms, each a SEPARATE OS process in a FRESH copy of the crate:
#   A   clean copy            → harness must PASS (oracle GREEN)
#   M1..M9  patched copies    → harness must FAIL, and the mutant's
#                               designated oracle check must be among
#                               the FAILs (same table as APP-C §C.6)
#   C   fresh clean copy      → harness must PASS
#
# The main worktree is never modified: every arm runs in a copy under
# $WORK, all mutations exist only as patch files in mutations/. Logs are
# archived under evidence/gate-i-mutations/ (no /dev/null discards).
#
# rc: 0 = all arms MET; 1 = at least one arm VIOLATED.
set -u

CRATE="$(cd "$(dirname "$0")/.." && pwd)"
EV="$CRATE/evidence/gate-i-mutations"
WORK="${TMPDIR:-/tmp}/wrk-scan-mutation-arms"
mkdir -p "$EV" "$WORK"
export CARGO_TARGET_DIR="$WORK/shared-target"
# ROUND (optional) stamps the archived logs so repeat cycles are all
# retained (e.g. ROUND=r2-); every run's logs are kept, none discarded.
ROUND="${ROUND:-}"

SUMMARY="$EV/cycle-summary$ROUND.txt"
: > "$SUMMARY"
VIOLATIONS=0

# designated primary oracle check per mutant (APP-C §C.6)
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
    # Supplementary arms (NOT frozen M1–M9 families): they kill the
    # campaign's OWN defenses — LW removes the post-fence predicate
    # re-check (the lost-wakeup defense), PB reintroduces the defect-path
    # B-permit wedge. Both are detected by the fence/control tests'
    # watchdog, never by the oracle.
    # "panic:" prefix = the designated signature is the control's panic
    # message (plain substring), not a harness test-name FAIL line.
    LW) echo "panic:worker never consumed" ;;
    PB) echo "watchdog:" ;;
    *) echo "" ;;
  esac
}

# LW/PB run the LIB tests (the fence/panic controls live there), not the
# mutation harness.
arm_filter() {
  case "$1" in
    LW|PB) echo "--lib" ;;
    *) echo "--test mutation_harness" ;;
  esac
}

fresh_copy() {
  # Copy with the repo-relative layout preserved: research/wrk-scan's
  # path dependency `../../crates/qianqian-composition` must resolve
  # inside the copy too.
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
  # Minimal stand-in for the repo-root workspace manifest: exists only so
  # crates/qianqian-composition's `edition.workspace`/`version.workspace`
  # inheritance resolves inside the copy (the real root manifest lists
  # members that do not exist here).
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
    if [ "$primary" = "watchdog:" ]; then
      hit=$(grep -c "watchdog:" "$log")
    elif [ "${primary#panic:}" != "$primary" ]; then
      # panic-message signature: a libtest panic line carries the message
      # itself, never a "  <test>  FAIL" table row — match the substring.
      hit=$(grep -cF "${primary#panic:}" "$log")
    else
      hit=$(grep -c "^  $primary  *FAIL$" "$log")
    fi
    if [ "$rc" -ne 0 ] && [ "$hit" -gt 0 ]; then
      echo "$name rc=$rc MET (RED; failed: ${failed:-watchdog/control detection})" >> "$SUMMARY"
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
for m in M1 M2 M3 M4 M5 M6 M7 M8 M9 LW PB; do
  dir="$WORK/$m"
  fresh_copy "$dir"
  applied=0
  for p in "$CRATE/mutations/$m"*.patch; do
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
# Only tracked-file MODIFICATIONS count; untracked new files are the
# campaign's own deliverables.
git -C "$CRATE" status --porcelain -- src tests | grep -v '^??' > "$EV/worktree-untouched.txt" || true
echo "worktree src/tests modifications: $(wc -l < "$EV/worktree-untouched.txt") entries (0 = untouched)" >> "$SUMMARY"

cat "$SUMMARY"
exit $([ "$VIOLATIONS" -eq 0 ] && echo 0 || echo 1)
