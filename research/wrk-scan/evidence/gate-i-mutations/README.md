# Gate I — H0 mutation admission receipts (`cycle-summary*.txt`, `arm-*.log`)

The H0 executor's own M1–M9 admission, with the frozen defect semantics of
APP-C (`evidence/frozen/APP-C-6312145882.md`), re-targeted onto THIS
campaign's H0 executor (`src/h0.rs`).

## Mechanism

- Each mutant exists ONLY as a patch file under `mutations/` — the
  committed `src/h0.rs` contains no mutation code, and the arm driver
  (`mutations/run-mutation-arms.sh`) never edits the main worktree: every
  arm runs in a FRESH copy of the tree under `$TMPDIR` (with the
  `qianqian-composition` path dependency and a minimal workspace-root
  stand-in so it builds standalone).
- `tests/mutation_harness.rs` runs the frozen probe shape (N=48, P=4,
  B=8, cancel after the 24th admission, group 7) and asserts the UNMODIFIED
  O1–O10 oracle GREEN — absolutely, in clean and mutant copies alike. A
  mutant "counts" only when that unmodified oracle goes red.
- Arms, each a separate OS process (`cargo test --test mutation_harness`,
  shared `CARGO_TARGET_DIR`, different source roots):
  `A (clean) → M1..M9 (patched) → C (fresh clean)` — GREEN→RED→GREEN.
- Target items follow APP-C's reachability discipline: all group-1 items
  admitted before the milestone, except M7's item 27 (group 7, after the
  milestone) by construction; M5 parks worker 4 (P=4).
- Two SUPPLEMENTARY arms beyond the frozen families kill the campaign's
  OWN defenses (never part of frozen M1–M9): `LW` removes the post-fence
  predicate re-check (the lost-wakeup defense) and `PB` reintroduces the
  defect-path B-permit wedge. Both are caught by the fence/panic control
  tests' watchdogs (`cargo test --lib`), not by the oracle.

### Run-artifact isolation (PR #307 review round 2)

The files in this directory are intentional archives. Up to cycle r7
they were produced directly — this directory was then the driver's
default output target; since the receipt-isolation fix (c4f12b3) an
archive here can only be produced by an explicit
`GATE_I_MUTATION_OUT=<this-dir>` run. The driver never writes here by
default anymore: a plain replay (`bash mutations/run-mutation-arms.sh`)
creates a unique untracked run directory under `$TMPDIR` (pid+nanos,
echoed on stdout) and writes the per-arm logs, the cycle summary and the
worktree witness THERE — a replay cannot truncate or overwrite any
committed receipt (same receipt-isolation rule as `tests/matrix.rs`'s
`GATE_I_RECEIPT_OUT`). Both paths are proven end to end by the archived
`evidence/logs/phase12-*` files: the r8 verification replay ran the FULL
13-arm cycle with the default temp output (`git status --porcelain`
empty before and after; `cycle-summaryr8.txt`: 13/13 MET, 0 violations,
witness 0), and the override smoke ran another full cycle with
`GATE_I_MUTATION_OUT=/tmp/wrk-scan-ovr-smoke` — every artifact landed in
that directory and the repo stayed untouched.

## Results — final cycles `cycle-summaryr4-.txt`, `cycle-summaryr5-.txt`, `cycle-summaryr6-.txt`

Each final cycle: A GREEN, C GREEN, all nine frozen mutants RED with the
designated frozen check, both supplementary arms (LW, PB) RED via the
control watchdogs, 0 violations, main-worktree `src`/`tests`
modifications 0 (committed tree, witness line 0).

| Arm | Defect (APP-C semantics) | Designated (frozen §C.6) | Observed failed checks (all 3 final cycles) |
|---|---|---|---|
| M1 | admitted obligation silently lost (never queued/tracked/terminal) — item 20 | O1 (+O9) | `O1_terminal_coverage` `O9_fanin_after_all_terminals` |
| M2 | duplicate ResultPublished — item 10 | O2 (+O3) | `O2_exactly_once_effects` `O3_content_vs_ref` |
| M3 | effect after the item's terminal ACK — item 16 | O4 | `O4_no_effect_after_item_terminal` (+`O2_exactly_once_effects`) |
| M4 | forged abandon, group never cancelled — item 22 | O5 | `O5_abandon_legitimacy` |
| M5 | quiescence published before the join; worker 4's exit lands after the receipt | O6 (+O10) | `O6_executors_joined_before_quiescence` `O10_no_effect_after_quiescence` |
| M6 | content corruption (`cpu_blocks-1`) — item 26 | O3 | `O3_content_vs_ref` |
| M7 | post-cancel admission — group-7 item 27 after the cut | O7 | `O7_no_admission_after_cancel` |
| M8 | wait + IoWaitDone both omitted — item 25 | O2 (wait dimension) | `O2_exactly_once_effects` |
| M9 | error-lane contribution forgery — fault item 13 | O3 (Err branch) | `O3_content_vs_ref` |
| LW | post-fence predicate re-check deleted (supplementary — kills the lost-wakeup defense's non-vacuity) | `worker_consumes_item_notified_during_fence_park` (dedicated: no alternative wakeup source in its schedule) | `panic:worker never consumed` — the control's panic message, matched by the driver's `panic:` signature class (deterministic; see `gate-i-finding-lost-wakeup.md` §RED-side status for the masked-sibling history) |
| PB | defect-path B-permit wedge reintroduced (supplementary — reverts the panic-path fix) | control watchdog | `watchdog:` (`owner_panic_with_saturated_b_fails_loudly` times out) |

Signatures match the frozen probe receipt (APP-C §C.6) exactly, except
M3 where this oracle additionally fires `O2` for the late effect — a
superset of the frozen signature with the designated `O4` present.

### Cycle history (retained, not discarded)

`cycle-summary.txt`, `cycle-summaryr2-.txt`, `cycle-summaryr3-.txt` are
the pre-fix cycles. r2 recorded a genuine LW escape: its sibling kills
were premise-by-timing (masked by `finish_admission`'s `notify_all` +
multi-worker drain), which led to the dedicated control. r3 killed LW
3/3 with that control but recorded `LW … VIOLATION` — a DRIVER
classification defect: the designated-hit grep matched only libtest
FAIL-table rows, never panic messages. The mutant was killed; the
recording was wrong. The driver gained the `panic:` signature class
(commit b146935) and the r4–r6 cycles above are the corrected record.
The r3 summary's `worktree … 1 entries` line is the honest witness that
the dedicated-control h0.rs was in flight (uncommitted) at run time.
`cycle-summaryr7-.txt` is the re-admission cycle on the guarded source
(quiescence precondition guard, commit 9ad5e0a): same 13/13 MET,
0 violations, with M5 dying on `O6`+`O10` through the regenerated
`M5d.patch` (f3f2e81).
`cycle-summaryr8.txt` is the receipt-isolation verification replay
(PR #307 review round 2): first cycle executed by the driver AFTER its
default output moved to the unique untracked run dir — same 13/13 MET,
0 violations on the guarded source, and the pre/post `git status`
snapshots (`evidence/logs/phase12-prestate.txt`,
`phase12-poststate.txt`) show the repository completely untouched by
the plain replay. It was invoked as
`ROUND=r8 bash mutations/run-mutation-arms.sh` (the prestate file
records the ambient, unset environment); `ROUND` only stamps artifact
names INSIDE the run dir — hence `arm-r8M1.log`/`cycle-summaryr8.txt`
vs the `r7-` style of earlier rounds — so the isolation claim rests on
the echoed default run dir, not on the ROUND prefix.

## Negative controls (Gate 授权) — mapping to executed evidence

All six controls are deterministic in-process tests (fence-based
interleaving construction, `src/h0.rs` `cfg(test)`), all EXECUTED in the
archived full-suite logs (`evidence/logs/phase23-cargo-test-r*.log`,
`phase45-cargo-test-r*.log`, `phase5-cargo-test-full-r1.log`) and re-run
by every later gate round:

| Control | Test(s) | Defense | Additional kill evidence |
|---|---|---|---|
| lost wakeup (worker / settlement) | `worker_wakeup_survives_notify_before_wait`, `settlement_survives_done_flip_while_waiter_parked` | post-fence predicate re-check | REAL defect found by this control: `evidence/gate-i-finding-lost-wakeup.md`; non-vacuity: LW arm |
| full-B backpressure blocks and recovers | `full_b_producer_blocks_and_recovers` | B = admitted-not-terminal accounting; permit released only after terminal ACK | — |
| owner death fails the session loudly (no forged receipt) | `owner_panic_fails_session_loudly`, `owner_panic_with_saturated_b_fails_loudly`, `k0_on_owner_death_teardown_violated_no_receipt` | panic-contained → explicit `SessionError` → abort joins all, no aggregate/receipt; K0 latches `TeardownViolated`; the B-saturated control pins that the defect path cannot wedge the producer on a leaked permit | non-vacuity: PB arm |
| cancel racing admission (linearization) | `cancel_racing_admission_linearization` | cut check inside the cancel-order critical section; re-check after the fence | — |
| producer done with queue non-empty still drains | `producer_done_with_queue_nonempty_still_drains` | single legal exit funnel requires drained queue | — |
| shutdown with a worker unjoined | abort() path + M5 arm | `join_workers` joins every handle; `SessionStats.workers_joined` | M5 mutation arm: quiesce-without-join killed by `O6`+`O10` |
