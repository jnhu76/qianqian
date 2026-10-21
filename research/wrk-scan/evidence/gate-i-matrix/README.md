# Gate I — 92-cell correctness matrix receipt (`cells.jsonl`)

One file = one complete matrix run: 46 frozen scenario×P×B cells
(SPEC §3.4 derivation at P_phys=10) × K0{ON,OFF}, executed sequentially in
one debug-build binary by `tests/matrix.rs::matrix_92_cells_all_green`.
The run is deterministic (see facts below); the committed file is the
receipt of the latest full-suite run (phase6/9/10 logs). Whole-binary wall
clock (~3 min debug) is harness overhead, not mechanism timing — Gate I
is non-timed and no duration is evidence.

## What each arm runs

- `K0_MODE=ON` — the session is composed on the real
  `qianqian-composition` kernel (`K0ScanSession`): activation spawns the
  worker pool, admission+settlement run on the composing thread, and the
  teardown inverse joins every worker and publishes the aggregate +
  quiescence receipt. `K0_DISPOSE_VERDICT` must be `Discharged`.
- `K0_MODE=OFF` — the same executor (`h0::run_session`) driven directly,
  observed by the minimal `OffReceipt`. No K0 involvement at all.

Both arms are the same binary, same WorkloadCore, same H0 executor, same
B/P semantics (SPEC §4.6 rule 5).

## Field definitions (produced by tests/matrix.rs)

| Field | Meaning | Source |
|---|---|---|
| `ARM` | `H0-{ON\|OFF}-{S#}-P{p}-B{b}` (stable arm identity) | `Cell::arm_id` |
| `ARM_ORDER_VALUE` | the frozen per-seed arm-order value, `SHA256(LE8(seed) ‖ 0x00 ‖ ASCII(arm_id))` first 8 bytes LE u64 (SPEC §7.3); recorded as identity, never ranked in Gate I | `arm_order_value` |
| `SCENARIO`,`P`,`B` | the frozen cell | `Scenario::expand_cells(10)` |
| `K0_MODE` | composition factor (the K0 factor is paired diagonally with the observer factor — ON arms run the full ledger, OFF arms the minimal receipt; no-cancel cells pin BOTH faces to the same independent REF, which transitively establishes the instrument-only equivalence the mandate asks for) | — |
| `SEED` | fixed public Gate-I seed | 0x00C0FFEE03040001 |
| `BATCH_ID` | unique deterministic id per arm (1000 + 2·cell + mode) | harness |
| `INPUT_DIGEST` | sha256 over item-ordered (LE4 id ‖ content_digest[32]); pins the input byte stream. Depends only on (seed, N), so same-N scenarios share a digest (4 distinct values across 7 scenarios is expected) | REF precompute |
| `ADMITTED_COUNT` | admissions (admission-linearization records / OffReceipt items) | executor stats |
| `TERMINAL_COUNT` | CompletedOk + CompletedErr + Abandoned terminal ACKs | ledger/receipt |
| `COMPLETED_OK`,`COMPLETED_ERR`,`ABANDONED` | terminal vocabulary counts (SPEC §5.1) | ledger/receipt |
| `RESULT_DIGEST` | sha256 over item-ordered (LE4 id ‖ LE8 contribution) of the COMPLETED (non-Abandoned) set | ledger/receipt |
| `AGGREGATE_DIGEST` | the published fan-in aggregate | ledger/receipt |
| `MAX_B_IN_FLIGHT` | high-water of admitted-not-terminal (the B accounting set); must be ≤ B | `SessionStats.b_high_water` |
| `MAX_ACTIVE_EXECUTION` | high-water of workers inside item work; must be ≤ P | `SessionStats.active_execution_high_water` |
| `WORKER_EXIT_COUNT` | ExecutorExited records / OffReceipt.worker_exits.len() | ledger/receipt |
| `JOIN_COUNT` | workers joined | `SessionStats.workers_joined` |
| `QUISCENCE_RECEIPT` | the implementation's formal receipt: ON = `QuiescenceReceipt@seq=<ledger seq>`; OFF = `OffReceipt.quiescence_recorded=true` | ledger/receipt |
| `K0_DISPOSE_VERDICT` | kernel teardown verdict (ON only) | `DisposeVerdict` |
| `ORACLE` | frozen O1–O10 adjudication (ON only; OFF has no ledger to adjudicate) | `oracle::adjudicate` |
| `ORACLE_VERDICT` | the arm verdict (see below) | harness |

Field provenance: the per-cell field list follows the Gate I mandate's
record requirements (§10 of the campaign brief; OWNER-GATE §3.B
observables) using the implementation's formal record names.

## Verdict semantics

- No-cancel cells (S0–S4, S6): EXACT equality with the independently
  computed REF — every item Completed{Ok,Err} with its REF contribution
  (`RESULT_DIGEST` == REF), aggregate == `workload::aggregate(REF)`, all N
  admitted and terminal, zero Abandons, B/P/join/quiescence discipline
  intact. Applied identically to ON and OFF.
- S5 (cancel) cells: the ON arm is adjudicated by the frozen O1–O10
  oracle. BOTH arms additionally pass an independent per-item legal-set
  audit: Completed items carry exactly their REF contribution, Abandoned
  items belong to a cancelled group (7), the published aggregate equals
  the re-fold of the actual completed set, the cut bit (admitted < N),
  exactly one cancel of group 7. ON↔OFF identity is deliberately NOT
  required for S5: which cancelled items Abandon vs complete is legal
  interleaving nondeterminism.

## Observed execution facts (both runs)

- All 92 rows `ORACLE_VERDICT=GREEN`; all 46 ON rows `Discharged` with
  `ORACLE=GREEN`.
- S5 rows: admitted exactly 225 of N=256 (deterministic — the cut bites
  at the frozen 164th admission and every later group-7 offer is cut);
  every admitted item reached a terminal. WHICH in-flight group-7 items
  Abandon vs complete is legal schedule-dependent nondeterminism (SPEC
  §6.3): the abandoned count grows with the in-flight window (1–4 across
  P=4/5/10 in the archived runs), and the ON/OFF arms of a cell may
  legally diverge here (or agree, as in the phase10 run) — each row is
  independently GREEN via the oracle
  (ON) and the per-item legal-set audit (both).
- `MAX_B_IN_FLIGHT ≤ B` and `MAX_ACTIVE_EXECUTION ≤ P` on every row.
- No-cancel rows: identical `RESULT_DIGEST` and `AGGREGATE_DIGEST` across
  K0 modes, P, B, and across independent runs — the deterministic
  correctness baseline (one distinct digest per scenario; S5 has one
  digest per legal abandoned-set outcome, so its count can differ between
  runs and between a cell's ON/OFF arms).
