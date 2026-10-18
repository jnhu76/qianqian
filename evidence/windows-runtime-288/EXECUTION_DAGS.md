# #288 Windows Runtime Verification — Real Execution DAGs (R1)

Reconstructed from source at main `fc81b68` (PR #287 error-system implementation
`b86a9ce` confirmed an ancestor). Every node carries `SOURCE_FILE:LINE`. The four
DAGs below are **candidate chains derived from real code**; each edge is classified:

```text
PO   = program order (same thread, guaranteed)
SW   = synchronizes-with (a real synchronization edge: lock, Release/Acquire, join, condvar)
SCHED= ordering by scheduling only (NOT a correctness edge; must not be relied on)
```

Level vocabulary used later in the matrix (never mutually promoted):
`COMPILE_ONLY / WINDOWS_DEVICE_FREE_RUNTIME / WINDOWS_NATIVE_BACKEND_RUNTIME /
WINDOWS_REAL_ENDPOINT / DETERMINISTIC_FAULT_INJECTION / PRODUCTION_PATH_OBSERVED / NOT_RUN`.

---

## DAG-1 — WASAPI Failure Cone (the chain A1/A2 verify)

Owner of every node below: the **render thread** (`qianqian-wasapi-render`,
spawned per episode at `wasapi.rs:164-175`), except where noted. The episode's
composition owner is the Playback Session Plugin (`playback_session_spec`).

```text
NODE                                        OWNER            SOURCE_FILE:LINE
R1  steady loop reads edge (read_frames)    render thread    wasapi.rs:792
R2  INJECTED panic in read (test wrapper)   render thread    cone_oracle_tests.rs:199-201
    └─ PO: injected.store(true) BEFORE panic! (same thread)
R3  unwind caught by catch_unwind           render thread    wasapi.rs:290-296
    └─ outcome = LoopOutcome::Aborted{Panic, "render thread panicked"}
R4  match arm Aborted                       render thread    wasapi.rs:307
R5  render_input.stop()          [CONE STOP]  render thread  wasapi.rs:311
    └─ forwards through session edge → edge.terminal becomes non-Open (first-wins)
    └─ decode worker blocked in edge wait wakes (edge internal condvar — SW)
R6  drain.complete(Aborted{reason}) [CONE COMPLETE]  render thread  wasapi.rs:314-317
    └─ PO: strictly AFTER R5 returns (R5 is a plain call; R6 follows in same arm)
    └─ DrainSignal::complete: verdict lock (first-wins), observer OUTSIDE lock
       (ports.rs:219-246)
R7  session mirror observer runs            render thread    completion.rs:395-421
    └─ SW: DrainSignal verdict mutex + synchronous observer call
    └─ publish_evidence: completion lock; drain_verdict=Some; resolve() evaluated
R8  resolve(): Aborted + worker_terminal    main/any         completion.rs:1360-1392
    └─ NOT decidable until worker evidence arrives (None => keep waiting)
R9  decode worker observes edge terminal    decode worker    session.rs decode_worker
    └─ SW: edge terminal publication (R5) wakes the worker
    └─ worker exits → worker_exited published under completion lock
R10 resolve() decisive: Aborted + Stopped, stop_requested==false
                                            publish_evidence caller  completion.rs:1370-1387
    └─ outcome = Failed{stage:"device"} committed under completion lock
    └─ condvar notify (SW) → wait_terminal waiters wake
R11 wait_terminal() returns Failed          host helper thread (oracle)  session wait path
R12 dispose() → teardown: stop_and_join     host thread      wasapi.rs:238-243
    └─ stop forwarded (first-wins no-op), join render thread (SW: join)
    └─ join returned == render thread finished DeviceSession::Drop
       (wasapi.rs:451-463: client.Stop → COM releases in field order) + ComApartment Drop
R13 (post-cone, gate on only) abort record  render thread    wasapi.rs:321-328
    └─ PO: AFTER R6 returns; α-pool publish only (see DAG-2)
```

**Happens-before backbone**: R2→R3 (PO on render thread) · R5→R6 (PO) ·
R6→R7 (SW: DrainSignal lock + synchronous observer) · R5→R9 (SW: edge terminal)
· R9→R10 (SW: completion lock) · R10→R11 (SW: condvar) · R11→R12 (host PO)
· R12 join→R12 return (SW: thread join).

**Counter A / witness B (A2's oracle, `cone_oracle_tests.rs`)**:
- A: wrapper `stop()` forwards to real input THEN increments `edge_cuts`
  (`cone_oracle_tests.rs:206-213`) — on the render thread inside R5, PO before R6.
- B: relay observer runs synchronously inside `complete()` (before it returns),
  snapshots A (`cone_oracle_tests.rs:243-249`) — SW with R6.
- Assertion: snapshot(A) ≥ 1 at settle; monotonicity of A makes later
  `stop_and_join` increments (R12) harmless.

**Waits depending on external scheduling/devices**: R9 wake latency (edge
waiter), R10 worker-exit publication (native decoder returns), R11 (condvar
scheduling). The oracle bounds the whole settle (SETTLE_BOUND = 15 s) — its
firing is the necessity RED, not proof of deadlock.

**Necessity argument (why no external stop ⇒ no settle if R5 deleted)**:
the frozen regime (no EOF, no decode error, no zero-frame stall, no seek, no
external stop/dispose between injection and observation — `cone_oracle_tests.rs:31-39`,
fixture facts) leaves the decode worker's every exit requiring
`edge.terminal() != Open`, which only the cone's R5 (or an external stop) can
produce. Full enumeration: `error-system-migration.md` §7.1 (necessity proof,
R4-corrected form).

---

## DAG-2 — Diagnostic Writer (sites #8/#9/#10; the chain A4 verifies)

The writer swap (PR #287) changed all three sites from `eprintln!` to class-α
records. Pre-swap shapes (from `b86a9ce^`, extracted verbatim for the pre-swap
negative controls):

```text
site   pre-swap shape (b86a9ce^)                      current shape (fc81b68)
#8     wasapi.rs:260-262 eprintln! BEFORE stop and    wasapi.rs:321-328 emit_render_
       complete (match arm on outcome) — panic               diagnostic AFTER complete
       skips R5+R6 ⇒ N-01 hang                            (α publish; relay gate)
#9     wasapi.rs:538-547 eprintln! after Opened         wasapi.rs:596-614 α publish
       verdict published — panic caught by catch #1      (gate: mechanism_log_enabled)
       ⇒ post-activation abort / Failed{device}
       misattribution / N-01 cascade via #8
#10    wasapi.rs:680-685 eprintln! in the recoverable   wasapi.rs:748-758 α publish
       SetAllVolumes arm — panic unwinds into the        (volume_diagnosed latch)
       steady_loop catch ⇒ Aborted ⇒ Failed{device}
       (AF-4 misattribution)
```

Current publish path (post-swap), all three sites:

```text
NODE                                     OWNER           SOURCE_FILE:LINE
W1  mechanism_log_enabled() env gate     render thread   wasapi.rs:107-109
W2  Text::from_static/push_* (bounded,   render thread   diagnostics lib.rs:134-231
    heap-free, no Display)
W3  qianqian_diagnostics::emit(record)   render thread   wasapi.rs:120-126
W4  emit_through: enter_section (ONE     render thread   diagnostics lib.rs:957-964
    RMW on admission word; closed bit decides)
    └─ facility absent (test binary): emit_without_facility (lib.rs:939-950)
       → pool.publish directly, no admission word
W5  pool.publish: atomics only —         render thread   pool.rs:152-171
    attempted++, admission check (d≤2),
    slot write, pub_gen.store(Release)
    └─ NO lock, NO allocation, NO sink call, NO blocking — the isolation claim
W6  drainer thread polls pools (≤ every  drainer thread  lib.rs:1260-1304
    POLL_WINDOW 50 ms) — drain() Acquire
    loads, copies records, cons_gen store
W7  deliver → StderrExporter::export     drainer thread  lib.rs:1196-1234,
                                                          1418-1447
    └─ write_stderr_line: writeln!+flush, returns Err on
       broken sink — NEVER panics (lib.rs:1242-1247)
    └─ broken: Err ⇒ exporter disabled + counted (export_failed),
       ONE self-diagnostic line (lib.rs:1213-1227)
    └─ blocked: the write blocks THE DRAINER (declared residual;
       producers unaffected) — shutdown bounded-or-honest abandons it
W8  failure-cone progress (R5/R6 of DAG-1) NEVER waits on any W-node
```

**The invariant A4 verifies**: broken-sink terminal class / exit path / wakeup
ordering == healthy-sink, per site × gate. The cone (R5, R6) is structurally
upstream of and independent from W1–W7 (post-swap PO: #8 emits only AFTER R6
returns; #9/#10 sit before/outside the cone's reliable publications).

**Pre-swap hazards (what the negative controls must reproduce)**:
- #8 pre-swap + broken sink + gate on: panic at pre-swap position skips R5+R6 →
  N-01 (four preconditions: gate on / write FAILS (broken, not full) / episode
  unresolved / machine path wait_terminal-before-dispose —
  `error-system-reconciliation.md` §3 N-01, narrowed R1.1).
- #8 pre-swap + blocked sink: eprintln blocks render thread before R5/R6 →
  settle delayed unboundedly (B F5).
- #10 pre-swap + broken sink: panic → steady_loop catch → Aborted →
  Failed{device} misattribution (AF-4).
- #9 pre-swap + broken sink: panic after Opened published → catch #1 →
  post-activation abort (misattribution) or N-01 cascade via #8.

---

## DAG-3 — Carrier B (AbortReason to consumer; the chain A6 verifies)

```text
NODE                                     OWNER           SOURCE_FILE:LINE
C1  real backend abort constructs        render thread   wasapi.rs:307-319
    AbortReason{class, message} (≤512 B,
    ports.rs:126-151)
C2  DrainVerdict::aborted_with(reason)   render thread   ports.rs:176-180
C3  drain.complete(verdict) — reason     render thread   wasapi.rs:314; ports.rs:219-246
    rides INSIDE the verdict value (no
    second cell/lifecycle)
C4  session mirror stores verdict        render thread   completion.rs:395-421
    (completion lock, first-wins; late
    verdicts dropped + counted :410-418)
C5  resolve() matches DISCRIMINANT only  resolver         completion.rs:1360-1392
    └─ reason is control-inert by construction: never read on the
       decision path (DG-2; absence always legal)
C6  observe_snapshot(): presentation     host reader      completion.rs:1126-1136
    enrichment — terminal Failed AND class==Device ⇒
    failure_diagnostic += ": {reason.message}"
    └─ Panic-class reasons do NOT enrich (current design; enrichment is
       Device-class only)
C7  machine outcome_report renders the   machine path     machine.rs (outcome_report)
    diagnostic on stderr (entry.rs:563-567)
```

Backbone: C1→C2→C3 (PO) · C3→C4 (SW: DrainSignal observer) · C4→C5 (SW:
completion lock) · C6 reads committed state (SW) · C7 after wait_terminal.

**A6's claims to witness**: (a) a real-endpoint abort constructs a Device-class
reason; (b) it survives C3/C4 (not lost — mirror holds it); (c) resolve()
classifies Failed{device} WITHOUT reading it (inertness); (d) C6 presents it;
(e) display does not bend the session's authority (outcome class unchanged
with/without reason).

---

## DAG-4 — Diagnostics Close Protocol (the chain A7 verifies)

```text
NODE                                     OWNER           SOURCE_FILE:LINE
D1  host installs facility (before any   host main        entry.rs:176-183;
    emitting thread starts; OnceLock)                     lib.rs:1020-1060
D2  drainer thread loop: poll_pools +    drainer thread   lib.rs:1340-1384
    recv_timeout(poll_window)
D3  producer emit: admission section      producer thread  lib.rs:957-1008
    (ONE RMW; closed bit ⇒ Disposition::Shutdown + counted)
    └─ panic hook mirrors this (lib.rs:769-787)
D4  shutdown(deadline): close_admission   host main        lib.rs:1104-1139
    (ONE fetch_or) — linearization point
D5  drainer observes is_closed after a    drainer thread   lib.rs:1371-1381
    poll cycle → wait_for_publisher_
    quiescence (bounded 250 ms; RMW loads
    — lib.rs:572-585) → sweep_quiescent
D6  final_sweep: poll pools (residual     drainer thread   lib.rs:1306-1338
    walk incl. retired pools), drain β
    channel, exporter.flush()
D7  drainer_exited.store(Release)         drainer thread   lib.rs:1383
D8  shutdown(): bounded poll of           host main        lib.rs:1109-1120
    drainer_exited; deadline hit ⇒ ABANDONED report
    └─ abandon ⇒ drainer left running (stuck in sink), never claimed drained
D9  host join (only when exited)          host main        lib.rs:1121-1128
D10 exit report: attempt_exit_report —    host main        entry.rs:209-280
    DETACHED, never-joined writer thread;
    bounded wait (500 ms) only when NOT
    abandoned; abandoned ⇒ no wait at all
D11 process exit (machine exit code       host main        machine.rs exit table
    never depends on any D-node — Class C)
```

Backbone: D1→D2 (thread spawn SW) · D3↔D4 (same atomic word, RMW total order)
· D4→D5 (Acquire load of closed bit) · D5→D6 (PO) · D6→D7 (PO+Release) ·
D7→D8/D9 (Acquire/join SW) · D10 fire-and-forget (detached; SCHED-free).

**Races the protocol owns**: admission-vs-close (D3 vs D4, single-word RMW);
late α publications (retired-pool residual walk D6, pool.rs is_retired
Acquire); late β sends (receiver gone ⇒ counted admission loss,
lib.rs:999-1006); a producer stalled inside its section (D5 window expires ⇒
report honestly NOT drained, `drained:false, abandoned:false`).

**A7's Windows-specific questions**: real Windows thread/pipe semantics for
D8 abandonment timing, D10 detached-writer behavior at process exit (does a
parked detached writer delay or corrupt exit?), anonymous-pipe capacity and
whole-slot release on full-pipe writes (Linux round-3/4 evidence does not
transfer), and admission/close races on real Windows scheduling.

---

## What is already covered elsewhere (not re-derived here)

- Pool protocol arithmetic/wrap/loom/Miri: `pool.rs` module docs + tests.
- Close-race delivery/honesty oracles (Linux real-thread, round-4 pinned):
  `diagnostics/src/lib.rs` tests; A7 re-executes them on Windows.
- `render_order_oracle.rs` pins F4 publication order as source text on all
  platforms; the event-handle lifetime oracle needs a real endpoint
  (executed in this campaign's lane as bonus evidence).
