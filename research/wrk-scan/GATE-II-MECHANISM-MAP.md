# Gate II — Mechanism-Neutrality Map and A_NARROW Extraction Record (#308)

Authority: Owner GO `GATE_II_H1_GO` (#308 comment 6330781617), choice
`SHARED_CODE = A_NARROW`: only the mechanism-neutral execution function,
unavoidable shared data types, and the business effect-emission logic move
to `session_core.rs`. H0 keeps its FIFO/Condvar/state-lock/exit-funnel
ownership; H1 owns its channel/receiver/dispatch/close/abort. This file is
the reviewable map required by the Gate II prompt §3–§5; it defines no
product semantics.

## 1. Classification by real call relationship (h0.rs @1375d48)

| Class | Items | Disposition |
|---|---|---|
| SHARED_BUSINESS_SEMANTICS | `execute_item` (wait → generate_input → transform → content_digest → contribution → fault lane → `TransformDone`/`ResultPublished` emission); `SessionError`; `SessionStats` (mode-independent face); the terminal-ACK-before-release ORDER discipline | `execute_item` + the two types move to `session_core.rs` (h0 re-exports preserve every existing path). ACK-order discipline is restated per mechanism, not shared as code. |
| H0_DISPATCH_MECHANISM | `VecDeque` queue, `work_cv` park/wake loop, `admit_all` enqueue+`notify_one`, worker drain funnel, `ExecutorSpawned/Exited` emission | stay in `h0.rs` |
| H0_SYNCHRONIZATION_STATE | `H0State` (queue/admitted_not_terminal/pending/cancelled_groups/producer_done/shutdown_requested/active/high waters/results/panic_defect), `Shared{state, work_cv, slot_cv, settle_cv}` | stay in `h0.rs` |
| CANCELLATION_ACCOUNTING | `cancelled_groups` set + `CancelRequested` ledger record under the cancel-order lock; cut check inside the admission critical section; owner observation at dequeue (H0) / post-recv (H1) | semantics shared (frozen SPEC §4.2/§5.3); state and lock per mechanism |
| TERMINAL_ACK_OWNERSHIP | `finish_terminal`: owner ACK first, then B release + obligation discharge + results push + waiter wakeup | per mechanism (touches each mechanism's lock/state); identical order |
| SETTLEMENT_PREDICATES | `producer_done && pending.is_empty()` (or defect) | predicate shape shared, condvar per mechanism |
| K0_COMPOSITION_OWNER | `k0_session.rs`: composition owner stays on the control thread; teardown inverse owns join + aggregate + receipt (or abort, no receipt) | genericized over the executor surface; kernel untouched |
| OBSERVER_BOUNDARIES | `ObserverSink` dual mode (ON ledger / OFF receipt) | fully shared, unchanged |
| cfg(test) shared facilities | owner-panic injection point (`TEST_PANIC_ITEM`, read inside `execute_item`), `session_test_lock`, fence gates | move to `session_core.rs` (test-only) so both mechanisms' controls inject identically; fence gate NAMES remain per mechanism |

## 2. Minimal call DAG

```text
Admission (session owner thread; H0 admit_all / H1 admit_all)
    |
    +-- (A) B permit wait: admitted_not_terminal < B      [mechanism condvar]
    |
    +-- (C) cancel-order critical section                  [mechanism state lock]
    |       cut check -> release permit (never consumed)   [per mechanism]
    |       H0:  sink.admit -> queue.push_back -> pending/count
    |       H1:  pending/count -> try_send -> sink.admit   (rollback-safe order;
    |             sink.admit under the state lock makes the ItemAdmitted record
    |             visible to every later post-recv state step)
    |       ^ ADMISSION_LINEARIZATION = the ItemAdmitted ledger append inside
    |         the cancel-order critical section (frozen SPEC §4.2/§5.2)
    |
Dispatch                                             [H0 work_cv / H1 channel]
    |
execute_item                                          [SHARED session_core]
    |-- synthetic wait + IoWaitDone                   [shared effect]
    |-- generate_input / transform / digests          [shared workload]
    |-- TransformDone + ResultPublished               [shared effects]
    `-- fault lane -> (TerminalKind, contribution)    [shared]
    |
Terminal ACK by the real owner                        [owner = executing worker]
    |
finish_terminal: ACK -> B release -> obligation discharge -> fan-in + wake
    |                                                 [per mechanism state]
Settlement: producer_done && pending empty (or defect) [shared predicate]
    |
Exit / Join                                           [H0 drain funnel / H1 disconnect+flag funnel]
    |
Quiescence: aggregate + receipt as LAST record        [shared shape]
```

`execute_item` never calls `ref_result` (SPEC §2.5(b)); only oracle-side
code (`oracle.rs`, test faces) computes REF.

Lock/channel discipline, stated precisely (review A F4): NO blocking
channel op, NO `recv`/`select`/`join` and NO condvar wait happens while
holding the state lock; the ONE channel op inside the admission critical
section is the non-blocking `try_send` — that is the frozen mechanism
admission action of ADMISSION_RECIPE step C (SPEC §4.2), placed there so
a full channel can never make the producer wait under the lock. The
last-sender drop (`finish_admission`) happens AFTER the state lock is
released; the abort wake signals are sent outside the lock.

## 3. Patch re-anchoring map (code motion consequence, prompt §5)

The H0 mutation patches are context-anchored to `src/h0.rs`. Moving
`execute_item` invalidates exactly the four patches whose hunks live inside
it; everything else applies unchanged (verified by an actual RED run after
the move, never by appealing to the r8 receipt).

| Patch | Old target (h0.rs) | New target | Why |
|---|---|---|---|
| M1 (lost admitted item) | `admit_all` | unchanged | admission path stayed |
| M2 (duplicate result) | `execute_item` | `src/session_core.rs` | execute_item moved |
| M3 (effect after terminal) | `worker_main` | unchanged | worker loop stayed |
| M4 (forged abandon) | `worker_main` | unchanged | worker loop stayed |
| M5a–d (receipt before join) | state struct / drain / `await_quiescence` | unchanged | H0 lifecycle stayed |
| M6 (reduced CPU blocks) | `execute_item` | `src/session_core.rs` | execute_item moved |
| M7 (admission after cancel) | `admit_all` | unchanged | admission path stayed |
| M8 (skipped wait) | `execute_item` | `src/session_core.rs` | execute_item moved |
| M9 (forged Err payload) | `execute_item` | `src/session_core.rs` | execute_item moved |
| LW / PB / PBb | worker wait loop / B-wait defect bail / panic notify | unchanged | H0 sync paths stayed |

Arm classes are never conflated: `PATCH_APPLICATION_FAILURE`,
`TEST_COMPILATION_FAILURE`, `MUTANT_KILLED_BY_ORACLE`, `BASELINE_GREEN`,
`RESTORED_GREEN` are recorded separately in every Gate II receipt.

## 4. What the shared layer must never become

`session_core.rs` holds no queue/channel/condvar/worker-loop/admission or
release logic, so it cannot become a generic scheduler. Both executors call
the SAME business function on their own real dispatch paths, which is what
keeps the H0/H1 comparison about the execution+communication mechanism
rather than two business algorithms (Owner GO letter §2.4).

## 5. Owner merge-review revision (PR #310 REVISE round, P2-01)

The Owner's merge review found one P2: under an abort interleave a worker
could publish TWO `ExecutorExited` records — `handle_item`'s post-recv
shutdown branch recorded an exit, but it only returns to `worker_main`,
whose next top-of-loop check recorded the exit again at the real thread
exit. Fix: that branch no longer records; the only publishers are the two
true exit boundaries in `worker_main` (the top-of-loop shutdown break and
the disconnect funnel), both followed immediately by the thread return.
H0 is unaffected (its single `worker_exit` site is already inside
`worker_main`, immediately before `return`). `shutdown_requested` is
monotone (only `abort` sets it, never cleared), so the loop-top publisher
is deterministically reached after the branch returns.

Deterministic regression: `abort_post_recv_exit_event_is_unique` parks the
worker at the `h1_worker_post_recv` fence holding an admitted-but-unexecuted
item, drives the opener from abort's own control-plane signal, and asserts
exactly one `ExecutorExited` for the joined worker on the same ledger face
the 92-cell matrix reads. Dual-arm evidence at the FINAL source bytes
(sha256 bound in each `run-context.txt`): RED `[1, 1]` vs `[1]`, rc=101
(`evidence/gate-ii-rev2/test-red/`, defect state `efca14bd…` = HEAD body +
final test), GREEN rc=0 (`evidence/gate-ii-rev2/test-green/`, fixed state
`3d8a8e17…`). Earlier draft-test runs are archived unmodified under
`evidence/gate-ii-rev2/superseded/` (the first RED used a draft assertion
expecting worker id 0; H1 worker ids are 1-based — reruns are new files,
the drafts are not silently replaced).

Delta review (two independent fresh-context reviewers, concurrency +
oracle focus): concurrency FIXES_VERIFIED (P1=0, P2=0); oracle initially
FIXES_INSUFFICIENT solely on evidence provenance (archived RED came from
the superseded draft; GREEN context lacked HEAD/sha binding) — remediated
by the regeneration above at the reviewer-reconstructed defect hash
(`efca14bd…` matched independently). All twelve mutation patches still
apply to the fixed tree (independently dry-run verified by both reviewers;
M5a–d mutate `await_quiescence`/the disconnect funnel, not the removed
line); no re-anchoring, no campaign re-run.
