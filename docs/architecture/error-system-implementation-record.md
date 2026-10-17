# Error-system implementation record — six-module refactor (2026-10-16)

```text
STATUS:  IMPLEMENTATION RECORD / EVIDENCE INDEX — NOT AUTHORITY
BASE_SHA 79c6d7832ce445115276f9a505901804cfa5415b (PR #284 merged; reviewed head 758a114)
HEAD     the implementation branch's tip: thirty-three commits, each reviewable on its own;
         fifteen of them close the owner's four #287 review rounds (§9 first round,
         §10 second round, §11 the close protocol, §12 the oracle premise); the tip adds the
         loom-lane test-hygiene fix surfaced by the round-4 CI log (§12.6)
```

This document records what an implementation round actually landed against the
stable-named error-system design set, which mechanism choices were made where the
design left a local representation choice open, which evidence was executed and
which was not, and which items still need an owner ruling. It defines no
semantics: the authority owners remain `error-disposition-contract.md` §6 (the
logging firewall), `error-system-reconciliation.md` (frozen facts, ledger rows),
`error-system-target-contract.md` (target design, DG-1..DG-4c),
`error-system-migration.md` (per-boundary migration + verification-instrument
designs), `error-system-decision-record.md` (owner directions), plus the playback
authorities (PBK-001/002/003) for anything playback-semantic.

Read this together with the PR's implementation table (doc clause → current
implementation → modification target → consumers → acceptance evidence →
deletions). That table is the per-row index; this record is the round's summary,
the executed-evidence register and the open-ruling list.

## 1. What landed, by module

| Module (campaign scope) | Landed shape |
|---|---|
| M1 error representation | `DecodeFault{status, native_code, message}` retained at the adapter's error point (DG-3 shape (a), no long-lived `last_fault`); `BoundedIoFault`/`IoContext` attribute host-io callback faults to the top-level operation boundary with slot hygiene (DG-4a); the adapter's public shapes (`DecodeError`/`DecodeOpenError`) unchanged |
| M2 propagation | The typed fault reaches the diagnostic outlet's record fields (`status`/`native_code`) and the App-facing message text; the output abort explanation travels on the drain verdict itself (DG-2 carrier B) |
| M3 handling/escalation | Resolver and terminal vocabulary untouched: `resolve` matches the drain discriminant only, so a carried reason cannot relabel anything; the device-abort classification (`Failed{stage:"device"}`) is unchanged and now has an oracle |
| M4 lifecycle errors | `Late-drain-verdict` policy is drop + count (bounded, honest) with a test witness; probe failures read the native diagnostic before close; open failure stays status-only because the ABI publishes no handle to query |
| M5 logging | New host-owned `qianqian-diagnostics`: class-α ownership-handoff pools, class-β bounded queue with `try_send`, one drainer with the only exporter calls, bounded-or-honest shutdown, non-panicking stderr writer; wasapi's three gated `eprintln!` sites became α records; the cone is structurally log-free; the App installs the outlet per transport run and drains it at exit |
| M6 migration/acceptance | The three gated print sites' information moved into records or the verdict reason; E1-ABI and E1-WORKER built and executed; the two real-cone oracles written and compile-checked (runtime NOT_RUN here); migration/deletion rows below |

## 2. Mechanism choices made this round (local representation, no new authority)

Each of these was left open by the design as a local choice. None changes terminal
priority, recovery strategy, public semantics, the SongCore ABI, or K0 authority.

1. **α pool protocol** — the #284 model's natural-wrap ownership handoff became the
   production pool, with the protocol's arithmetic and both directions
   (Release/Acquire) preserved, plus a one-shot panic latch for DG-4b. The model
   itself was a test model; production re-established single-producer
   registration, storage lifetime, wraparound and loss accounting, and the
   acceptance gates were re-run against the production type.
2. **Loom vs Miri duty split** — the payload is a `std::cell::UnsafeCell`, which
   Loom does not track for data access. The split is explicit: Loom verifies the
   protocol's synchronizes-with structure over the explored schedules (which is
   what the Release/Acquire loads control), and Miri (real threads, many seeds)
   is the tool that catches a payload data race in the recycle window. The
   Relaxed-recycle negative control is red under Miri, not under Loom.
3. **Abort-class enumeration** — of the design's candidates (`Device`, `Panic`,
   `Cone`, `Open`), only `Device` and `Panic` are constructible; the two absent
   ones are documented as absences with their reasons, not published as names.
4. **Exporter admission (T5-5 strategy A)** — the stderr exporter's progress bound
   is documented on the exporter itself, with its environment residual stated:
   producers are isolated, the drainer is not, and that is why shutdown may
   abandon with counted losses.
5. **DG-4b isolation predicate** — the predicate is the outlet's own thread fact
   (`is_alpha_producer()`: this thread registered a class-α pool), consulted on every
   panic, with the posture installed at transport startup (the #287 review round
   replaced the first landed reading — a static registry of thread NAMES — after the
   review showed a render/decode panic could chain into the default hook and block on
   the stderr lock before its `catch_unwind`; §9). A thread that never registered chains
   (fail-open). The hook's critical path is atomics-only and never renders: the counter
   and the one-shot pool record are read by non-critical threads.
6. **DG-4c install point** — the bridge is crate-private and installs from the
   decode plugin's activation. The reason is authority-backed, not convenience:
   the decode chain's public surface is frozen by the plugin-boundary gate
   (audit §7.5 / MAJOR-3) to the plugin constructor and the D14.6 probe query, so
   a public installer would need its own surface amendment. The host keeps the
   decision through the existing `QIANQIAN_FFMPEG_LOG` switch, and activation
   precedes every `song_open` this provider can serve, which is exactly what the
   T5-9 ordering constraint requires.
7. **decode worker as a class-α producer** — the session (which creates that
   thread) registers its pool, because the adapter's record is emitted on the
   worker and the design classifies that thread as isolation-obligated. Without
   it the record would ride the class-β channel, which the classification
   forbids on that thread.
8. **Diagnostics dependency edge** — `qianqian-playback → qianqian-diagnostics` is
   admitted in the boundary gate with its reason recorded in the allowlist
   (ordinary library code: no Plugin identity, no capability, no K0 noun).
9. **`DrainVerdict` loses `Copy`** — the recorded cost of carrying a bounded
   explanation; one bounded clone on the cold publication path. The public
   contract change went through the audio-api surface gate's own regeneration
   procedure with a reviewed diff.
10. **loom feature propagation across the new dependency edge** — `RUSTFLAGS`
    is global (every crate in a `--cfg loom` build sees the cfg) while
    `--features` is package-scoped, so the playback `loom` feature now carries
    `qianqian-diagnostics/loom`. Without it, the pool crate — reached from the
    playback session — compiled under `--cfg loom` with no loom crate to link
    against (E0433), which is what the first CI run of the Rust gate caught;
    the same edge also added `crates/qianqian-diagnostics/**` to that gate's
    trigger paths, and the playback-concurrency lane now plays back the pool's
    own two schedule tests.

## 3. Deletions and replacements (no dual tracks)

* the three wasapi `eprintln!` sites and their gated-print carrier of the abort
  message (the reason now travels on the verdict; the site remains as an α
  record, which is a diagnostic, not a carrier);
* `status_open_error` in the decode adapter (replaced by the typed fault path);
* `Box<File>` host-io userdata (replaced by `IoContext`);
* `DrainVerdict::is_aborted` and the unconstructible `OutputAbortClass::Cone`
  (published in an earlier commit of this branch, removed before the branch was
  final: an unconsumed name is not a description);
* the session-level "the cone is covered by the teardown oracle" assumption: the
  new oracles exist precisely because the old fixtures cannot speak for
  `wasapi.rs` (the design's REV.2 withdrawal).

## 4. Executed verification (this round, at HEAD)

| Target | Command (all from the repository root) | Environment | Result | Limits |
|---|---|---|---|---|
| Whole workspace | `cargo test --workspace --no-fail-fast` | rustc/cargo 1.97.1, Linux, `/tmp/errlog/dg2-workspace2.log` | 916 passed / 0 failed, all 54 test binaries ok | Linux only; not device evidence |
| Lints | `cargo clippy --workspace --all-targets -- -D warnings` | same | clean | does not cover the workspace-excluded decode chain |
| Format | `cargo fmt --all --check` | same | clean | — |
| Plugin boundary | `python3 tools/check_plugin_boundaries.py` (+ negative controls in the pre-push gate) | same | PASS (topology + export surface + audio-api contract) | structural admission, not runtime |
| E1-WORKER | `cargo test -p qianqian-playback --lib e1_worker` | same | 12/12, every trace in its predicted §2.3.1 cell | scripted schedules, one at a time |
| E1-ABI | `cargo test --manifest-path crates/qianqian-decode-songcore/Cargo.toml --lib e1_abi` | real native SongCore artifact | 8/8 including both gate legs | corpus-limited (see the test module's calibration note) |
| DG-4b witness | `cargo test -p qianqian-headless --lib dg4b` | child processes | green; isolation-removal mutation RED with the documented signature (reverted byte-identically) | one registered critical thread today |
| DG-4c witnesses | `cargo test --manifest-path crates/qianqian-decode-songcore/Cargo.toml --lib av_log_bridge` | real `av_log` through the real callback | 6/6 | β shape: no interpolated values; discard path unobserved at runtime |
| F5 suite (unchanged) | `bash specs/f5-seek-discontinuity/check.sh` | tla2tools 1.7.4 | base PASS (672 distinct states) + 6 witnesses + M1–M7 all COUNTEREXAMPLE-WITNESSED | no model action changed this round |
| Windows compile | `cargo check --target x86_64-pc-windows-gnu -p qianqian-output-wasapi --tests` | mingw target, Linux host | exit 0; a `compile_error!` sight control proved the test target was really compiled | compilation is not runtime evidence |
| Loom (final pool) | `RUSTFLAGS="--cfg loom" cargo test -p qianqian-diagnostics --features loom --release --lib` | same | 2/2 schedule-clean (base + cross-wrap states) | explored-schedule bound, not a general proof |
| Miri (final pool) | `MIRIFLAGS="-Zmiri-many-seeds=0..24" cargo +nightly miri test -p qianqian-diagnostics --lib real_thread_handoff` | nightly Miri, real threads | clean over the seeds | Miri's scheduling is still a sample per seed |
| playback-concurrency lane (CI fix) | `bash specs/playback-concurrency/check.sh` | rustc 1.97.1, Linux, `/tmp/errlog/lane-postfix-20261016T170540.log` | native BOUNDED-CLEAN; loom SCHEDULE-CLEAN (12/12, 158.2 s); pool-loom SCHEDULE-CLEAN (production α pool, 2/2); M-L1 COUNTEREXAMPLE-WITNESSED | bounded schedules; this is the lane the Rust gate job runs |
| Full local gate | `lefthook run pre-push --all-files` | same | 8/8 steps green (commit-convention, vocabulary, plugin-boundary incl. its negative controls, rust-fmt, rust-clippy, rust-test, release-policy, docs-verify); 918 tests passed / 0 failed | the gate proves the working tree, not the Windows/formal/concurrency lanes |
| Commit convention | `python3 tools/check_commit_convention.py --range 79c6d78 HEAD` | same | 14/14 PASS | header grammar only |

Negative controls executed this round, each reverted byte-identically and each
re-verified on the final tree: the DG-1 Relaxed-recycle pool control (Miri RED with
the Record-payload data-race signature, at the final pool), the DG-4b
isolation-removal control (RED with its documented signature), the cone-oracle
compile sight control (RED — proving the Windows test target really is compiled),
and the boundary gate's own negative-control suite.

## 5. NOT_RUN (declared, with what each would settle)

| Item | Why not run here | What it would settle |
|---|---|---|
| O-CONE-NECESSITY / O-CONE-ORDER runtime runs | `wasapi.rs` is `#[cfg(windows)]`; this host is Linux. The module is written and compile-checked for `x86_64-pc-windows-gnu` | the cone's machine-progress premise (settle with no external stop) and the stop-before-complete order |
| Cone negative controls (delete `stop`, swap `stop`/`complete`) | same; the module documents both mutation procedures and their expected reds | that necessity and order are each load-bearing |
| E-7 tier 1 (writer-swap per site × gate on/off, broken/blocked sink) | needs a Windows host with a real endpoint for #8/#9, and a volume-apply fault seam for #10 (the design names that seam as the one small addition) | that a diagnostic-write failure cannot change terminal class/exit path/wakeup |
| av_log discard-path runtime observation under the pinned FFmpeg | needs the runtime with FFmpeg logging driven | that the default discard behaviour is what C's code says (unchanged by this round) |
| Kani | toolchain not available on this host (declared CI formal-lane tool) | bounded proof of the pool protocol under Kani's model |
| Windows device/audible evidence for the new carrier B reason | no Windows host | that a real device abort's reason reaches the presentation surface |

## 6. Owner rulings (D-2/D-10 ratified 2026-10-17, receipt PR #291 comment 6229433676; the rest presented, not decided by any implementation)

**D-2 — probe diagnostic read / adapter retention — RATIFIED 2026-10-17.**
Substance landed (§1 M1/M4: `DecodeFault{status, native_code, message}` copied at
the error point; probe reads `song_last_error` before `song_close`; public shapes
unchanged; SongCore ABI untouched). The owner ruled RATIFY_OPTION_A (scope =
local representation + diagnostic retention only; no T1-3/D-9/recovery approval):
receipt [PR #291 comment 6229433676](https://github.com/jnhu76/qianqian/pull/291#issuecomment-6229433676),
record `error-system-decision-record.md` §6.1. The binding note (migration §3.5)
is applied in songcore-binding-architecture §6.2 by the same ruling.

**D-10 — output reason snapshot carrier — RATIFIED 2026-10-17.** Carrier B landed
(§1 M2/M3: bounded `AbortReason` inside `DrainVerdict::Aborted` on the existing
completion seam; discriminant-only resolve; drop+count late policy;
control-inertness witness). The owner ruled RATIFY_CARRIER_B together with the
PBK-003 minimal clarification, applied as the ADR-PBK-003 §5 TERMINAL EVIDENCE
Amendment note (same receipt; decision-record §6.2/§6.3). The ratified shape
changes no production behavior and adds no obligation on future backends.

**D-C — F5 model extension for the pending-fatal × seek × stop window.** The
design's acceptance-gated trigger is NOW SATISFIED, and this round verified it
rather than assuming it: E1-ABI is green on both legs (healthy control; partial
success then latched fatal) and E1-WORKER's W1–W6 all land in their predicted
§2.3.1 cells. The design also states that a clean E-1 does **not** automatically
convert the extension into NOT_EARNED, and that D-C must be an explicit owner
weighing of (a) residual interleaving risk — which legal × defect trace pairs the
scheduled harness actually exercised versus the full matrix TLC would enumerate;
(b) adequacy of the Rust harness for the stop-race half (it observes the real
locks and exit surface, one schedule at a time); (c) model maintenance cost
against the expected frequency of seek-adjacent change. Because that weighing is
the owner's and the extension freezes the model against ground truth, this round
**did not** build it and did not change any model action; the extension's
obligations (Q5–Q11 of the audit's FORMAL-PLAN §3.1, defect traces D-T1..D-T4
written before any mutation, the scoped safety with the Applied-erasure witness)
are recorded in `error-system-migration.md` §7.6 for the round that does.

**DG-4c tier** — the bridge is β (no interpolated values). Tier α needs a native
`vsnprintf` shim and is admitted only when a named consumer demonstrates that
interpolated values are required; none exists today.

**DG-4c surface** — if the host should ever own the bridge install explicitly
(rather than through activation), the decode chain's frozen public surface needs
its own amendment. Not requested here.

**DG-4b registry membership — SETTLED by the owner's #287 review (§9).** The
predicate is the outlet's own thread fact (`is_alpha_producer`), not a name list, and
the posture installs at transport startup. The review's finding showed the
"reader-only" reading was wrong in principle: a render/decode panic that chains to
the previous hook can block on the process stderr lock (the default hook prints
through it) BEFORE its `catch_unwind`, and the threads that `join` those legs — the
session teardown's worker join, the render cone that must run after the catch —
would then wait on a diagnostic sink.

**Residual representation parameters** — record message bound (192 bytes), β
capacity (256), drain deadline (500 ms), and the abort-reason message cap (512
bytes) are round choices; they carry no semantics and are stated here so a later
review can move them deliberately.

## 7. Architecture properties the round had to preserve (and how)

| Property | How it is held |
|---|---|
| Operation results, reliable lifecycle evidence and discardable diagnostics stay separate | Diagnostics are Class C throughout: no exit code, terminal Fact, resource release, join or Discharge depends on a record. The App's exit code predates and ignores the outlet; the record fields are read only for presentation |
| The Session keeps terminal adjudication authority | `resolve` matches the drain discriminant; the carried reason has no reader in any resolution path (witnessed by a control-inertness test that delivers a reason and shows the terminal unchanged) |
| Evidence publication/resolve/terminal commit keep synchronous semantics | No async resolver, no waiter-triggered adjudication was added; the verdict still publishes inside `complete()` on the caller's stack, and the console-oracle relay proves the observer runs before `complete()` returns |
| Logging decides no terminal, performs no recovery, carries no reliable wakeup | The cone is structurally log-free (stop + complete first, record after); the volume-apply diagnostic is a class-α record on a recoverable failure, so a diagnostic fault cannot escalate it |
| No new Error/Recovery Plugin, EventBus or scheduler/DSL | The outlet is an owned process-lifetime resource with no composition identity; the boundary gate's universe and edge allowlists were updated deliberately and only by one edge |
| Every cross-boundary reason has an owner, lifetime, attribution and consumer | The typed fault is copied at the error point and consumed immediately (outlet fields + message text); the abort reason is owned by the verdict; the io fault lives in the operation-scoped `IoContext` |
| Replaced paths are deleted, no dual tracks | See §3 |

## 8. Evidence correspondence (design clause → instrument)

```text
DG-1 (facility, isolation, shutdown)     qianqian-diagnostics src/tests + Loom (played back
                                         by the playback-concurrency lane) + Miri + the
                                         App install/shutdown wiring in apps/headless/src/entry.rs
DG-2 (carrier B)                         ports.rs surface (+ audio-api snapshot), wasapi abort
                                         sites, e1_worker_tests dg2_* pair, completion.rs mirror
DG-3 / DG-4a (retention, attribution)    decode-songcore unit tests + e1_abi_tests typed-fault test
DG-4b (panic hook)                       machine_input.rs posture + the registration fact
                                         (`is_alpha_producer`) + tests.rs subprocess witness
DG-4c (av_log)                           decode-songcore av_log_bridge tests (6)
§7.1/§7.2 real cone                      qianqian-output-wasapi cone_oracle_tests (Windows-only)
§7.4 E1-ABI                              decode-songcore e1_abi_tests (8)
§7.5 E1-WORKER                           qianqian-playback e1_worker_tests (12)
§7.6 F5 extension                        gate verified here; model unchanged (§6 D-C)
```

## 9. Owner-review round (#287 review, 2026-10-16) — production boundaries closed

The review verdict accepted the CI fix and listed six production findings (3 P1, 3 P2)
against the frozen head `68111ab`. All six were adjudicated VALID against the code and
the design set; none of them required a semantics change, and none was answered by a
documentation note. What changed, and how each one is witnessed:

| Finding | What was actually wrong | Change | Witness (executed) |
|---|---|---|---|
| P1 — pool roles not enforced by the API | `AlphaPool` was `Sync` with public safe `publish(&self)`/`drain(&self, …)` and a public `AlphaRegistration::pool() -> &'static AlphaPool`: safe code could put two producers (or two consumers) on one pool — a data race in safe Rust | The core is crate-private; the producer role is the thread-bound `AlphaRegistration` (not `Send`/`Sync`/`Clone`) via that thread's TLS pointer; the consumer role is the drainer's walk with the position inside the core | protocol suites unchanged and green (arithmetic, Loom, Miri); `pool()` gone from the public surface; the av_log/exit witnesses now observe at the drainer's sink |
| P1 — abandonment report re-entered the stuck sink | `shutdown_host_diagnostics` wrote its report synchronously after `shutdown` returned `abandoned`; for stderr that means waiting on the process stderr lock the stuck drainer holds — exit handed back to the sink | Abandoned reports are attempted by a detached, never-joined writer without a wait; the drained case still wrote synchronously here and was corrected in round 2 (§10: a drained drain does not prove the sink has room for one more line) | `apps/headless/src/entry/shutdown_tests.rs`: child with an unread stderr pipe, drainer stalled at `consumed=1263` with a 256-record backlog (the pipe is objectively full), child printed `SHUTDOWN-RETURNED` and exited 0 |
| P1 — `previous(info)` on render/decode panics | The hook's isolation predicate was a thread-NAME list (`qianqian-stdin`), so a panic on a registered decode/render thread chained into an unknown hook — the default one prints through the global stderr lock and blocks BEFORE `catch_unwind`, so the join/completion paths that wait on those threads were gated on a diagnostic sink | The predicate is the outlet's own registration fact (`is_alpha_producer`); the posture installs at transport startup, before any decode worker/render thread exists | `machine_input/tests.rs` DG-4b witness: registered-noncritical-name isolated, reader-named-unregistered chains, and a registered thread panicking twice with a FULL stderr still unwinds and ends |
| P2 — real sink write failures counted as delivered | `write_stderr_line` swallowed the I/O result and `StderrExporter::export` returned `Ok(())` unconditionally: a closed descriptor (EPIPE) incremented `exported` | The writer returns its I/O result; the exporter maps a failure to an export failure (counted lost, exporter disabled, one self-diagnostic); a failed `flush` went to `export_failed` here and was corrected in round 2 (§10: a flush outcome is a sink-level self-diagnostic, not a record loss); blocked-vs-failed is now explicit | `crates/qianqian-diagnostics/tests/stderr_delivery_failure.rs`: child whose stderr read end is dropped — `exported=0`, `export_failed=5`, identity holds, `self_diagnostics=1` |
| P2 — producer path could wait on clock init | `Record::stamped` called `OnceLock::get_or_init`, so the first producer on a fresh clock entered initialization synchronization | The clock is started on cold paths (installation, registration); `stamped` only reads and reports `0` = no timestamp | `crates/qianqian-diagnostics/tests/clock_cold_path.rs` (own process): two stamps with a 5 ms gap stay 0; registration and installation each start it |
| P2 — per-episode pools leaked and poll cost grew | Every registration `Box::leak`ed a pool into an append-only registry; `poll_pools` searched a cursor per pool, so cost grew with the process's episode history (decode/render threads are per-episode) | Registration `Drop` publishes `retired` (Release); the drainer walks the residual records ordered behind its Acquire load, folds the counters into the facility totals, and releases the pool; the drain cursor lives in the core (no lookup) | `crates/qianqian-diagnostics/src/lib.rs` unit oracles: live pool never reaped, retired pool residual-drained then released (`Arc::strong_count == 1`), counters folded |

Round-local representation notes (no semantics): the fold-into-totals choice is the
target contract's residual `MECHANISM_OPEN` item ("whether per-pool loss counters stay
separate or fold into the global dropped counter") — the review's retirement protocol
makes the fold the only way to keep the totals across a release. The tests that used to
reach into a pool now observe at the drainer's sink
(`crates/qianqian-decode-songcore/tests/common/outlet.rs`), which is the honest
observation boundary the role enforcement leaves.

### 9.1 Executed verification of this round

| Target | Command | Result | Limits |
|---|---|---|---|
| Diagnostics crate | `cargo test -p qianqian-diagnostics` | green (8 unit + 7 integration binaries) | Linux only |
| Decode chain | `cargo test --manifest-path crates/qianqian-decode-songcore/Cargo.toml --lib` | green incl. the re-pointed DG-4c witnesses | corpus-limited as before |
| App + machine input | `cargo test -p qianqian-headless --lib machine_input` | green (23) incl. the extended DG-4b witness and the blocked-stderr legs | Linux only |
| Host exit boundary | `cargo test -p qianqian-headless --lib entry::shutdown_tests -- --nocapture` | green; child evidence `SINK-BLOCKED consumed=1263 backlog=256`, `SHUTDOWN-RETURNED`, exit 0 | Linux only; stderr pipe semantics |
| Clock | `cargo test -p qianqian-diagnostics --test clock_cold_path` | green (own process) | — |
| Broken sink | `cargo test -p qianqian-diagnostics --test stderr_delivery_failure` | green; `exported=0 export_failed=5` | EPIPE path; not a full-disk path |
| Pool protocol (Loom) | `RUSTFLAGS="--cfg loom" cargo test -p qianqian-diagnostics --features loom --release --lib` | SCHEDULE-CLEAN (same two models; protocol unchanged) | explored-schedule bound |
| Pool protocol (Miri) | `MIRIFLAGS="-Zmiri-many-seeds=0..24" cargo +nightly miri test -p qianqian-diagnostics --lib real_thread_handoff` | clean over the seeds | Miri samples one schedule per seed |
| Whole workspace | `cargo test --workspace --no-fail-fast` | 928 passed / 0 failed over 56 test binaries | Linux only |
| Playback-concurrency lane | `specs/check.sh rust` (the CI invocation) | green: `RESULT matrices(native) BOUNDED-CLEAN`, `matrices(miri) MIRI-CLEAN`, M-K1/K2/K3 witnessed, `RESULT native BOUNDED-CLEAN`, loom 12/12 (155.77 s), `RESULT pool-loom SCHEDULE-CLEAN`, `RESULT M-L1 COUNTEREXAMPLE-WITNESSED`; both `SUITE:` lines clean | bounds as before |
| Full local gate | `lefthook run pre-push --all-files` | green | proves the working tree, not Windows/formal lanes |

Negative controls executed for this round (each reverted byte-identically afterwards,
each RED with the documented signature): the synchronous abandonment report (the F2
child never exits), the name-based hook predicate (the F3 isolated leg never exits), the
swallowing stderr writer (F4 reports `exported=5`), the `get_or_init` clock (F5's second
stamp is non-zero), and the disabled pool reaping (F6's release oracle fails).

Commit hygiene note: `cargo fmt` reflowed a few of the round's own lines
(assert-argument wrapping, two `.then(...)` calls) after the four fix commits were
written; the reflow rides in this documentation commit rather than being rewritten
into them. The final tree is fmt-clean and the pre-push gate is authoritative.

### 9.2 Still NOT_RUN (unchanged from §5 unless stated)

* O-CONE-NECESSITY / O-CONE-ORDER runtime runs and their mutations — Linux host,
  `wasapi.rs` is `#[cfg(windows)]` (unchanged).
* E-7 tier 1 writer-swap controls — needs the Windows endpoint and the volume-apply
  fault seam (unchanged).
* Windows runtime evidence for the new carrier B reason (unchanged).
* The F2/F3 witnesses above are Linux-process evidence: the std facts they rely on
  (stderr's global lock; the exit path touching stdout only; the default hook printing
  through that lock) were verified against the pinned toolchain's own source, but the
  runs themselves are Linux.

## 10. Owner-review round 2 (#287 review II, 2026-10-16) — the remaining exit and lifecycle boundaries

The second review verdict (`REVISE_BEFORE_MERGE`, frozen at `33c9e25`) accepted the first round's
direction and named four further production findings (2 P1, 2 P2) plus one documentation drift.
All four were adjudicated VALID against the code; none needed a semantics change, none was
answered by a note, and each was closed RED-first: the new oracle was written against the landed
code, executed RED with its documented signature, and only then was the mechanism changed.

| Finding | What was actually wrong | Change | Witness (executed) |
|---|---|---|---|
| P1 — the DRAINED exit report could still block the host | The round-1 fix moved only the ABANDONED report off the exit path; the drained case kept a synchronous `write_stderr_line`. "Drained" proves the drainer's own writes finished, not that the sink still has room for one more line: a pipe reader that stopped leaves a full sink, and a parked writer holds the process stderr lock | `attempt_exit_report`: EVERY exit-phase report is attempted on a detached, never-joined writer; the drained case waits a bounded instant for that attempt (so a healthy sink still delivers the line), the abandoned case waits not at all | `apps/headless/src/entry/shutdown_tests.rs` `a_drained_report_never_makes_the_host_exit_path_wait_on_its_sink`: child verifies `DRAINED undelivered=0`, an independent writer then saturates the unread stderr pipe (`SATURATED bytes=65230`), the mandatory report (a counted critical panic) runs through the production exit path — child prints `SHUTDOWN-RETURNED` and exits 0. RED signature at `33c9e25`: the child never returns from `shutdown_host_diagnostics` (the 60 s deadline kills it) |
| P1 — duplicate registration broke the isolation fact | `AlphaRegistration::register()` allowed a second registration on one thread: the second overwrote the TLS fast path and the FIRST handle's `Drop` then cleared the live second's — a registered thread silently fell back to the class-β path and its panic back to chaining | `register()` rejects a second live registration on a thread (loud panic, production registers once per thread); re-registering after a drop stays legal | `crates/qianqian-diagnostics/src/lib.rs` unit oracles: the duplicate is rejected and the first handle stays intact (fast path, accepted counts); drop-then-register-again works; cross-thread lifetimes are independent with the later-created dropped first (the non-LIFO order that remains legal) |
| P2 — a failed `flush` was charged a record loss | `final_sweep` did `export_failed += 1` on a flush `Err`. `export_failed` counts records the exporter did not deliver; a flush outcome belongs to no record count, so the drain-boundary identity (`copied_out + beta_consumed = exported + export_failed`) broke at quiescence and `undelivered_now`'s clamp hid it | A failed flush is a SINK-level self-diagnostic (`self_diagnostics`) and never a record loss; both `Exporter` methods now state their delivery contract (one `Ok` from `export` = one delivered record; `flush` failure = sink-level) | `a_failed_flush_is_a_self_diagnostic_not_a_lost_record` (in-crate): an exporter that delivers one record and then fails its flush yields `consumed=1, exported=1, export_failed=0, self_diagnostics=1` and the identity holds. RED signature at `33c9e25`: `export_failed=1` |
| P2 — class α still admitted records after shutdown | `emit` consulted the thread-local pool BEFORE the facility's shutdown state, so a registered thread's record was published into a pool whose drainer had already exited — stranded and accounted as admitted | `emit` checks terminal admission FIRST, for both classes: a late α presentation advances its own pool's admission counters (`reject_presentation`), a late β presentation `beta_rejected`, and neither is published. `note_critical_panic` applies the same first check (the panic is still counted: the counter is the authoritative fact). Producer quiescence is DECLARED as not a precondition, with the one residual (a publication racing the final sweep of its own pool) named on `Diagnostics::shutdown` | `crates/qianqian-diagnostics/tests/shutdown_admission.rs`: child installs, registers, delivers one record, shuts down, then presents one more on the SAME registered thread — `first=AlphaRecorded after=Shutdown accepted=1 rejected=1 attempted=2 outstanding=0` with `drained=true`. RED signature at `33c9e25`: `after=AlphaRecorded accepted=2 outstanding=1` (stranded in a dead pool) |
| P3 — documentation drift | `implementation-record` §2 still described DG-4b's predicate as a static thread-NAME registry, contradicting §9 and the landed code; the target contract carried the same stale sentence, in the wrong section | §2 rewritten to the registration fact; the stale target-contract status block deleted (§8's evidence line corrected too) | textual; verified by reading the landed `machine_input.rs` posture and its witnesses |

Construction note for the drained-report witness (why it looks the way it does): the sink must
have room for every record the drainer writes and no room for the report. The oracle therefore
establishes `drained` FIRST (the child verifies the `ShutdownReport`), and only then lets an
independent writer consume the remaining capacity. Two earlier constructions were discarded
after direct measurement on this host: pipe space is freed to writers in whole buffer slots
(reading N bytes with N < the head slot's remainder frees ZERO writable space — measured), so a
"free exactly N bytes" arithmetic construction is not portable, and `Stderr`'s write path makes
byte-exact packing by a competing writer unobservable. The landed witness uses no byte
arithmetic at all: the writer's parked state IS "the sink is full", and the child asserts the
premise (`DRAINED`, then `SATURATED`) before the exit path runs.

### 10.1 Executed verification of this round

| Target | Command | Result | Limits |
|---|---|---|---|
| RED-first oracles (before the fixes, at `33c9e25`) | the four oracles above, one at a time | each RED with the documented signature (duplicate accepted; `export_failed=1`; `after=AlphaRecorded outstanding=1`; child hanging past 60 s) | Linux only |
| Diagnostics crate | `cargo test -p qianqian-diagnostics` | green (12 unit tests; 8 integration binaries incl. the new `shutdown_admission`; doc-tests) | Linux only |
| Whole workspace | `cargo test --workspace --no-fail-fast` | 936 passed / 0 failed over 57 test targets | Linux only; not device evidence |
| App exit boundary | `cargo test -p qianqian-headless --lib entry::shutdown_tests` | 4/4 green: drained-report child `BULK-EXPORTED 9 / DRAINED undelivered=0 / SATURATED bytes=65230 / SHUTDOWN-RETURNED exported=9`, exit 0; abandoned-drainer child unchanged | Linux only; stderr pipe semantics |
| Pool protocol (Loom) | `RUSTFLAGS="--cfg loom" cargo test -p qianqian-diagnostics --features loom --release --lib` | SCHEDULE-CLEAN (protocol unchanged; the new pool method is a counter-only path) | explored-schedule bound |
| Pool protocol (Miri) | `MIRIFLAGS="-Zmiri-many-seeds=0..24" cargo +nightly miri test -p qianqian-diagnostics --lib real_thread_handoff` | clean over the seeds | Miri samples one schedule per seed |
| Playback-concurrency lane | `specs/check.sh rust` (the CI invocation) | green: `RESULT matrices(native) BOUNDED-CLEAN`, `matrices(miri) MIRI-CLEAN`, M-K1/K2/K3 witnessed, §G.6 TEST-PASS, `RESULT native BOUNDED-CLEAN`, loom `SCHEDULE-CLEAN`, `RESULT pool-loom SCHEDULE-CLEAN`, `RESULT M-L1 COUNTEREXAMPLE-WITNESSED`; both `SUITE:` lines clean | bounds as before |
| Full local gate | `lefthook run pre-push --all-files` | green | proves the working tree, not Windows/formal lanes |

Negative controls for this round (each executed against the FIXED tree by reverting the single
mechanism, then restored byte-identically with `sha256` verification): the synchronous drained
report (the child hangs again), the accepted duplicate registration, the flush counted as
`export_failed`, and the pre-shutdown-check α publication. Raw logs, commands and hashes are
retained under `/tmp/errlog/rev3/` (controls subdirectory) for re-checking.

### 10.2 Still NOT_RUN (unchanged from §9.2 unless stated)

* O-CONE-NECESSITY / O-CONE-ORDER runtime runs and their mutations — Linux host,
  `wasapi.rs` is `#[cfg(windows)]` (unchanged).
* E-7 tier 1 writer-swap controls — needs the Windows endpoint and the volume-apply fault seam
  (unchanged; acceptance pending).
* Windows runtime evidence for the carrier B reason (unchanged).
* The exit-boundary witnesses above are Linux-process evidence, as in §9.2: the std facts they
  rely on (the process stderr lock; buffered/unbuffered behavior; the exit path touching stdout
  only) were verified against the pinned toolchain's source, and the pipe capacity/slot-release
  semantics were measured on this host (65536 bytes; whole-slot release) — not on Windows.

## 11. Owner-review round 3 (#287 review III, 2026-10-16) — the close protocol

The third review (`HOLD`, frozen at `b018d8f`) accepted the round-2 fixes and named ONE remaining
production boundary: shutdown and the final sweep were not linearized. Two counterexamples were
given (A: a class-α producer whose shutdown check passes and which then publishes while — or
after — the final sweep; B, not covered by the round-2 report: the same window on the class-β
channel between its second check and `try_send`). The review rejected closing this by declaring
a residual, because DG-1's acceptance clause requires a bounded-or-honest shutdown with
admission losses counted at the admission boundary and accountable counters. Path A (tighten the
closing protocol) was chosen; the DAG below was produced BEFORE the protocol was selected, and
every witness was executed RED against `b018d8f` first. The residual DECLARED in §10 (and in the
target contract's round-2 bullet) is CLOSED here; those rows remain as the record of what round 2
landed.

### 11.1 Minimal execution DAG (the landed order, at `b018d8f`)

| Step | Operation | Thread |
|---|---|---|
| D1 | `shutdown.store(true, Release)` | closer |
| P0a / P0b | `shutdown.load(Acquire) == false` — the α terminal check / the β second check | producer |
| P1 / P2 | `pool.publish()` (slot write, `pub_gen`, `accepted`) / `beta_send.try_send()` + `beta_queued` | producer |
| D2 | final sweep: pool walk (+ retired residual, reap), channel drain to empty, exporter flush | drainer |
| D3 | `drainer_exited` → `shutdown()` snapshots `counters_of(...)` into the report | closer |

Counterexample A: `P0a` reads false at t0; D1..D3 run to completion; `P1` publishes at t4. The
record is `accepted` at its pool, `copied_out` never advances, no rejection fact exists, and the
report built at t3 — already returned — says `drained: true, undelivered_at_exit: 0`. No walker
remains for the pool. Counterexample B: `P0b` false at t0; D1..D3; `P2` `try_send`s at t4 — the
`SyncSender` lives in the process-global facility, so the channel is NOT disconnected;
`beta_queued` advances after the snapshot and the record stays in the channel forever. One more
check cannot close either window: the check and the publication are two separate atomic
operations, and the drainer may interleave between them at will.

### 11.2 The protocol (one admission word, one RMW per side)

`Inner.admission: AtomicUsize` — bit 0 `CLOSED`, remaining bits `IN_FLIGHT`. Producer: one
`fetch_add(IN_FLIGHT, AcqRel)` whose returned word's closed bit IS the admission decision
(`AdmissionSection`'s `Drop` performs the single `Release` `fetch_sub` after the publication, on
every path — including the rejection path and the panic-hook publication). Closer: one
`fetch_or(CLOSED, AcqRel)`. Drainer: a bounded RMW observation
(`fetch_add(0, Acquire)`) until `IN_FLIGHT == 0`, then the sweep, then it records whether that
observation succeeded. `ShutdownReport::drained` requires the observation.

Why the argument holds: an RMW reads the value written by the latest modification before it in
the word's modification order. If the producer's `fetch_add` precedes the closer's `fetch_or`,
its word has CLOSED clear (it holds a count) and the drainer's observation cannot read 0 until
that producer's `fetch_sub`, whose Release is in the release sequence the observation's Acquire
reads from — so that publication's writes are visible to the sweep. If the `fetch_add` follows
the `fetch_or`, the RMW reads the closed word and the producer never publishes (it counts the
presentation at its class's boundary instead). A plain (non-RMW) observation would not be enough:
a load may read a stale 0 concurrent with a `fetch_add` that already precedes the closure in the
word's modification order.

### 11.3 Witnesses and controls (all executed)

| Oracle (in `crates/qianqian-diagnostics/src/lib.rs`) | Property | RED at `b018d8f` and under the control |
|---|---|---|
| `an_in_flight_alpha_publication_is_delivered_by_the_final_sweep` | a record admitted before the closure, published while the drainer waits, is delivered exactly once | `hits 0 vs 1`, release decided by `SweptFirst` (the sweep ran before the release) |
| `an_in_flight_beta_publication_is_delivered_by_the_final_sweep` | the same property for the channel | `hits 0 vs 1`, release `SweptFirst` |
| `an_alpha_publication_outlasting_the_quiescence_window_is_not_reported_drained` | the report never claims a drain while such a presentation is in flight; afterwards the record is visible at its own boundary (`outstanding`) and an explicit later walk delivers it — the drainer exited, so none is automatic | report `drained: true, abandoned: false, undelivered_at_exit: 0` with the presentation in flight |
| `a_beta_publication_outlasting_the_quiescence_window_is_not_reported_drained` | the same for the channel (the receiver drops with the drainer, so the presentation is counted as a `beta_rejected` admission loss — an accountable fact) | same |

Construction: a `cfg(all(test, not(loom)))` rendezvous parks the producer INSIDE its admission
section — after the closure decision, before the publication — a `Mutex` + `Condvar` seam that
does not exist in production builds. Each oracle establishes its premise through a signal, never
a sleep: the seam's Paused state, the release valve compared against the sink's own flush signal,
and the `ShutdownReport` itself (the release in the honesty oracles is ordered strictly AFTER the
report). Negative control: revert `enter_section` to a plain-load decision plus an uncounted
section (the landed shape) → all four oracles RED again with exactly the signatures above;
restored byte-identically (sha256 `de1e9627…`).

### 11.4 Executed verification of this round

| Target | Command | Result | Limits |
|---|---|---|---|
| RED-first oracles (before the fix, at `b018d8f`, one at a time) | the four oracles above | each RED with the documented signature | Linux only |
| Diagnostics crate | `cargo test -p qianqian-diagnostics` | green: 16 lib unit tests (the four oracles included) + 9 integration binaries | Linux only |
| Whole workspace | `cargo test --workspace --no-fail-fast` | 940 passed / 0 failed over 57 test targets | Linux only; not device evidence |
| Decode chain | `cargo test --manifest-path crates/qianqian-decode-songcore/Cargo.toml --lib` | 36 passed / 0 failed | Linux only |
| Pool protocol (Loom) | `RUSTFLAGS="--cfg loom" cargo test -p qianqian-diagnostics --features loom --release --lib` | 2 passed (SCHEDULE-CLEAN) | explored-schedule bound |
| Pool protocol (Miri) | `MIRIFLAGS="-Zmiri-many-seeds=0..24" cargo +nightly miri test -p qianqian-diagnostics --lib real_thread_handoff` | clean; all 24 seeds attempted, rc = 0 | Miri samples one schedule per seed |
| Playback-concurrency lane | `specs/check.sh rust` | `RESULT matrices(native) BOUNDED-CLEAN`, `RESULT matrices(miri) MIRI-CLEAN`, §G.6 TEST-PASS, M-K1/K2/K3 COUNTEREXAMPLE-WITNESSED, `RESULT native BOUNDED-CLEAN`, loom + pool-loom SCHEDULE-CLEAN, M-L1 COUNTEREXAMPLE-WITNESSED | bounds as before |
| Host exit boundary | `cargo test -p qianqian-headless --lib entry::` | 4/4 green, including the drained-report and abandoned-drainer child witnesses | Linux only |
| Full local gate | `lefthook run pre-push --all-files` | green | proves the working tree, not Windows/formal lanes |

### 11.5 Still NOT_RUN (unchanged from §10.2 unless stated)

* O-CONE-NECESSITY / O-CONE-ORDER runtime runs and their mutations — Linux host, `wasapi.rs` is
  `#[cfg(windows)]` (unchanged).
* E-7 tier 1 writer-swap controls — needs the Windows endpoint and the volume-apply fault seam
  (unchanged; acceptance pending).
* Windows runtime evidence for the carrier B reason (unchanged).
* The close-protocol witnesses are Linux-host, real-thread evidence: the admission word, the
  drainer's bounded wait and the seams are ordinary std synchronization on real threads, but no
  Windows process/thread evidence was executed this round (acceptance pending, unchanged).

## 12. Owner-review round 4 (#287 review IV, 2026-10-17) — the delivery oracles' premise

The fourth review PASSED the production close protocol and the timeout honesty, and held the PR on
ONE acceptance-oracle gap (P2) plus two wording items (P3). The gap: the two "delivered by the
final sweep" oracles released the parked producer on the round-3 rule "no sweep inside the 150 ms
valve" — a rule that can fire before the closer has even executed `close_admission()`. The
review's counterexample: the closer is delayed, the valve expires, the producer is released and
publishes into an OPEN facility, the drainer's ORDINARY poll consumes the record, the closer then
runs to completion — and every assertion of the oracle still passes (`hits=1`, `Valved`,
`drained=true`) while the target interleaving (admission → close → publication → sweep) never
happened. A negative control that legitimately goes RED does not substitute for every GREEN run
establishing its own premise.

### 12.1 The pinned premise (test-only; production bytes unchanged)

* The releaser waits for two SYNCHRONIZATION signals before releasing: (1) the closure published
  on the admission word (`CloseRace::wait_for_close` — the closer's own RMW), and (2) this
  interior's drainer observed inside its quiescence wait for this very publication
  (`CloseRace::wait_for_quiescence_wait`), read from the new `cfg(all(test, not(loom)))` counter
  `Inner::quiescence_waits`, bumped by `wait_for_publisher_quiescence` on each iteration that
  actually waited. The strict signal SUBSUMES the mandatory one: the drainer only reaches that
  wait through its `is_closed` branch, so the closure provably precedes the drainer's wait; the
  explicit close probe additionally names which premise is missing when one is.
* A premise that cannot be observed is a FAILURE, not a silent pass: the release still happens at
  the valve's expiry (5 s liveness net, so the test can fail and join) and
  `assert_eq!(release, ReleaseReason::ClosedAndWaiting)` reports it. The release rule is otherwise
  unchanged in kind — the same oracle, a stricter premise.
* One residual assumption remains and is named as scheduling LIVENESS, not ordering: after the
  observation, the release lands ~1 ms of local work later, against the drainer's 250 ms wait.
* The delivery oracle additionally witnesses the sweep's ordering: `race.sweeps() == 0` at the
  premise (the drainer cannot have swept while waiting for this publication) and `sweeps() == 1`
  after the close (the ONE final sweep is what delivered the record).

### 12.2 Controls and experiments (all executed)

| Run | Construction | Result |
|---|---|---|
| Targeted control: the drainer's quiescence wait REMOVED (sweep immediately after the closure; report keeps `sweep_quiescent=false`) | `controls/make-control.py` on the pinned tree | both delivery oracles RED — `hits 0 vs 1`, release decided by `Closed`; both honesty oracles PASS (this control removes the ORDERING only, so exactly the ordering oracles flip) |
| Vacuity experiment A (the review's schedule): closer delayed 600 ms past a 150 ms release valve, round-3 release rule restored | `controls/vacuity-experiment.py A` + `--nocapture` | both oracles PASS with `[vacuity-A] old 150 ms rule decides; closure published at release: false` — the counterexample reproduced executably |
| Vacuity experiment B: the same 600 ms delayed closer with the pinned rule | `controls/vacuity-experiment.py B` | both oracles PASS with the premise assertion in force (`ClosedAndWaiting`) |
| Restore | byte-identical | sha256 `98891d1d…` (pre/post every control and experiment run) |

### 12.3 Wording fixes (P3)

* `Diagnostics::shutdown`'s doc and the honesty oracle's own comments no longer let a "later walk"
  deliver the post-timeout α record by itself: the record remains in its pool as `outstanding` and
  is readable by an EXPLICIT later walk — the production drainer has exited, so none is
  automatic. The target contract's close-protocol bullet carries the same correction.
* `emit_without_facility` (and the module doc) now state the installation boundary: that path
  bypasses the admission word by construction, so the close protocol's guarantee begins at
  installation — a host installs the facility before starting any thread that emits, and
  concurrent emission with `Diagnostics::install` is not a supported schedule. (No host change:
  the headless host already installs first, then starts its threads.)

### 12.4 Executed verification of this round

| Target | Command | Result | Limits |
|---|---|---|---|
| Diagnostics crate (pinned oracles) | `cargo test -p qianqian-diagnostics --lib` | 16 passed / 0 failed; the delivery oracles assert `ClosedAndWaiting` (the premise really fired) | Linux only |
| Targeted control | the same command under `make-control.py` | 2 RED (documented signatures) / 2 PASS | Linux only |
| Vacuity A / B | both variants, `--nocapture` | A: pass with `closure published at release: false`; B: pass with the premise assertion | Linux only |
| Whole workspace | `cargo test --workspace --no-fail-fast` | 940 passed / 0 failed over 57 test targets | Linux only; not device evidence |
| Decode chain | `cargo test --manifest-path crates/qianqian-decode-songcore/Cargo.toml --lib` | 36 passed / 0 failed | Linux only |
| Pool protocol (Loom) | `RUSTFLAGS="--cfg loom" cargo test -p qianqian-diagnostics --features loom --release --lib` | 2 passed (SCHEDULE-CLEAN) | explored-schedule bound |
| Pool protocol (Miri) | `MIRIFLAGS="-Zmiri-many-seeds=0..24" cargo +nightly miri test -p qianqian-diagnostics --lib real_thread_handoff` | clean; all 24 seeds attempted, rc = 0 | Miri samples one schedule per seed |
| Close-race oracles (Miri, new this round) | `cargo +nightly miri test -p qianqian-diagnostics --lib publication -- --test-threads=1` | 4 passed / 0 failed (real threads through the seam, the counter and the admission word; no UB, no race) | single seed per oracle |
| Playback-concurrency lane | `specs/check.sh rust` | `RESULT matrices(native) BOUNDED-CLEAN`, `RESULT matrices(miri) MIRI-CLEAN`, §G.6 TEST-PASS, M-K1/K2/K3 COUNTEREXAMPLE-WITNESSED, `RESULT native BOUNDED-CLEAN`, loom + pool-loom SCHEDULE-CLEAN, M-L1 COUNTEREXAMPLE-WITNESSED | bounds as before |
| Host exit boundary | `cargo test -p qianqian-headless --lib entry::` | 4 passed / 0 failed | Linux only |
| Full local gate | `lefthook run pre-push --all-files` | green (8/8) | proves the working tree, not Windows/formal lanes |

### 12.5 Still NOT_RUN (unchanged from §11.5)

* O-CONE-NECESSITY / O-CONE-ORDER runtime runs and their mutations (Linux host; acceptance
  pending).
* E-7 tier 1 writer-swap controls (needs the Windows endpoint; acceptance pending).
* Windows runtime evidence for the carrier B reason (acceptance pending).
* Windows runtime evidence for the close protocol and the pinned oracles — the round-4 witnesses
  are Linux-host real-thread runs; the mechanism is ordinary std synchronization, but no Windows
  execution was performed (acceptance pending, unchanged).

### 12.6 Follow-up: the loom lane's unused test import (surfaced by the round-4 CI log)

The round-4 CI log showed one warning in the loom lane: `unused import: super::*` at
`crates/qianqian-playback/src/completion.rs:1689`. The import belongs to the E1-WORKER test module
whose every item is itself `not(loom)`-gated, so under `--cfg loom` the module is empty and the
glob has no user — a test-hygiene defect introduced with that family, invisible to the
debug-profile gate (where every item is active and the import is used). The import is now gated
under the same cfg as its users; no production code changed.

Verified on the fixed tree: the loom suite 12 passed / 0 failed with **zero warnings** (previously
12 passed + 1 warning); `cargo test -p qianqian-playback --lib` 207 passed / 0 failed; clippy
`--all-targets -- -D warnings` clean; `cargo fmt --all --check` clean; `specs/check.sh rust`
re-run — LANE_RC=0 with the unchanged `RESULT` set and no warnings in the lane log.

## 13. D-C execution round (#289, 2026-10-17) — the pending-fatal model extension

The owner decided D-C = EXTEND_EXISTING_MODEL on 2026-10-17 (receipt
[issue #289 comment 6229911387](https://github.com/jnhu76/qianqian/issues/289#issuecomment-6229911387);
decision-record §6.4; a new decision, not a backfilled #287 ruling). This round
executed it against `origin/main` frozen at
`0e43ef0cbac6fe21d0cd83e14875ace98c73501a`. Scope discipline held: **no
production code, ABI, K0 or ADR change; no new taxonomy; specs-local
representations only** (`f5-seek-implementation/` untouched — its M1–M7
production-patch correspondence is name-stable).

### 13.1 Trace-first frozen ledger (before any mutation)

`specs/f5-seek-discontinuity/PENDING_FATAL_DAG.md` — the six Phase-A maps
(EXECUTION_DAG / LOCK_BOUNDARY_MAP / WORKER_POSITION_MAP / PROVIDER_OUTCOME_MAP
/ LATCH_LIFETIME_MAP / TERMINAL_DECISION_MAP), every line re-anchored at the
frozen BASE_SHA (the reconciliation/migration line numbers were NOT reused),
legal families L1–L6 and defect traces D-T1..D-T4 with the six mandatory
fields each. The Q6 atomicity splits are load-bearing in the model:
`RecordStopIntent` and `EdgeStopFromStop` are separate actions (the intent-
visible-but-edge-open window exists); `WorkerSeek` (eligibility + begin) is
separate from the three provider-return actions (stop may land mid-FFI, and
the return actions are deliberately NOT gated on stop intent); fusions carry
lock citations (`publish_evidence`'s single hold justifies surfacing ⊕ commit).

### 13.2 Model extension and executed evidence

Extended `SeekDiscontinuity.tla` (11 invariants = baseline 6 + pending-fatal
5: `NoOldCursorResumeAfterApplied`, `RetainLatchOnRefused`,
`NoOldFatalAfterApplied`, `ScopedLatchedFatalNoComplete`,
`TerminalCellConformance`), 12 new actions with 1 removal (`Stop`) and every
baseline action re-gated (read split three ways, stop split
two ways, six real exit positions incl. the bookkeeping-only cancel, leg
verdicts, one-shot TerminalCommit), 27 variables each justified against the
Q5 six-question budget (27th = `refusedWhileLatched`, added by the
2026-10-17 independent-review F2 fix to pin W_REFUSED_FATAL_RETAINED's
shape; rejected candidates documented: latchRetained,
stopSeen, providerOutcome, a separate workerPos). TerminalCommit is a MINIMAL
constrained D11 projection (frozen resolver transcription; CommitCut ≠
TerminalCommit; protocol Stop never read as D11 Stopped; the Failed{device}
cells kept as unreachable-in-model frozen-table branches).

Executed (tla2tools v1.7.4 / TLC2 2.19 / java 17.0.20.1, 4 workers, safety
only, no fairness): **base PASS with 6330 distinct states** (15667 generated,
exhausted; 13117/5216 before the review-round `refusedWhileLatched` witness;
baseline was 672 — the growth is the pending-fatal × stop-window ×
exit-position × terminal-projection dimension, no real interleaving fused away,
no bound lowered); **11/11 witnesses reachable** (incl. the mandatory
`W_APPLIED_ERASURE_COMPLETED` — the scoped safety did not kill the legal
new-continuation Completed); **11/11 mutations COUNTEREXAMPLE-WITNESSED**
with M1–M7 showing NO regression and D-T1..D-T4 each caught by its frozen
target (D-T3 additionally evidenced by a diagnostic commit-completing trace:
intent → edge stop → aborted drain → bogus-position exit → TerminalCommit=
stopped, archived separately from the gate config). Calibration facts stand
unmodified: E1-ABI/E1-WORKER remain the Rust real-behavior calibration; TLC
results do not replace them.

Round-honesty records: (1) the base run went red ONCE — the first
`RetainLatchOnRefused` formulation was too strong and was refuted by the
LEGAL never-latched refusal; reformulated with `fatalLatchedEver` as the
"latch existed at refusal time" witness; the red log is archived, not washed
by a rerun. (2) The D-T3 ledger precondition was revised after counterexample
review (the bogus-exit mechanism is latch-orthogonal); both traces are
archived. (3) Four D-T cfgs initially shipped with an incomplete mutation-
constant block and failed for a configuration reason before any exploration;
regenerated and disclosed here.

### 13.3 Executed verification of this round

| Target | Command | Result | Limits |
|---|---|---|---|
| F5 suite (base + 11 probes + 11 mutations) | `specs/f5-seek-discontinuity/check.sh` | `F5-SEEK-DISCONTINUITY: ALL PASS` (SUITE_RC=0, rerun after the review fixes); base 6330 distinct / 0 on queue; per-run "Finished in" observed | CHECKED-IN-MODEL bounds (MAX_TAIL=2); not production evidence |
| Counterexample trace capture | 4 × targeted TLC runs (D-T1..D-T4) + 1 diagnostic run (D-T3 committed-cell chain) | each violates its frozen target; traces archived | evidence artifacts, not gate runs |
| Production tree | untouched (git diff scoped to `specs/f5-seek-discontinuity/**`, `specs/README.md`, three error-system docs) | SOURCE_CHANGES=NONE / ABI_CHANGES=NONE / ADR_CHANGES=NONE | — |

### 13.4 Still NOT_RUN (unchanged)

Nothing new goes unexecuted by this round; the extension adds no Rust
harness obligation (the existing E-1 harness is the calibration baseline, not
rewritten). The pre-existing NOT_RUN items (§12.5: Windows runtime
acceptance for cone / E-7 / carrier-B reason / close protocol) are unchanged
and remain owned by #288/#290.

### 13.5 Independent review round and fixes (2026-10-17)

Three-slice fresh-context adversarial review per the campaign checklist
(R1–R10). Delivered verdicts: authority/baseline/minimality slice **APPROVE**;
mutation/witness/evidence slice **REVISE (3×P2, all fixed)**; the
causal/matrix slice was interrupted by the account's 5-hour quota and is
re-run before final verdict (its slice had no findings recorded). All seven
P2s across the two delivered slices were fixed and the full suite re-executed
(ALL PASS, 6330 distinct — numbers above updated):

- decision-record §6.4: wrong section pointer (§12→§13) and a garbled
  machine-parsable receipt id (now `issue #289 comment 6229911387`).
- f5 .tla: DrainOld's comment had kept the baseline's "tail still plays out
  after failure" rationale while the guard had moved to terminal-before-data;
  comment corrected to the production-faithful reading (no invariant/probe
  depended on post-failure drains).
- ProbeNoDestructiveEver's variable was redefined (episodeFailed→
  workerFailurePublished, a superset); the mutated-route pin now carried by
  W_MUTATED_THEN_FAILED — redefinition documented in f5 README/RESULTS.
- RESULTS: the D-T4 evidence sentence overclaimed (the archived trace ends at
  the latch-retention violation; the surfacing follows by the guard but was
  not walked) — reworded.
- Two witnesses were shape-blind (review F2): W_REFUSED_FATAL_RETAINED now
  pinned by the `refusedWhileLatched` ghost (the "refuse-then-latch" equal-
  depth path no longer satisfies the antecedent), and W_FAILED_FROM_PENDING_
  FATAL now requires `surfacedFatal ∧ stopIntent` (the L5 fatal-beats-stop
  shape; a regression gating surfacing on ¬stopIntent would turn the witness
  green and fail the suite). Model, probes and cfgs updated; full suite
  re-run.
- Evidence-retention location (local archive path + per-file inventory +
  the D-T3 diagnostic-run recipe) now recorded in RESULTS (review F3).
- Causal/matrix slice (third reviewer, independent suite re-execution at
  head — reproduced 6330 distinct / 11+11 exactly): APPROVE with 5×P2,
  all ledger/comment-accuracy fixes, none touching a guard, invariant or
  the state space. Four applied as prescribed (eligibility "lock-free
  reads" → three separate single-lock acquisitions; the W-B first-half
  window's nondeterministic-placement idiom disclosed in the ledger and
  the WorkerSeek comment; EofClose's commit-only gate labeled a model
  scoping choice, not a production necessity, in README/.tla/ledger;
  processing-failure and panic exits declared outside the frozen
  six-path inventory in the unmodeled list). The fifth (witness
  sensitivity overclaim) had already been independently found by the
  mutation/witness slice's residual round and fixed at 9f95eef with
  bidirectional counterfactual verification.
- Review-round delta re-verification (same reviewer): F1/F2(b)/F3 FIXED;
  F2(a)'s defect-sensitivity claim was REFUTED by a reviewer counterexample
  (under the erasure defect, ReadPartialSuccess re-latches and a plain
  occurrence-semantics witness bit still fires). Fixed with the
  survivor-based definition (`refusedWhileLatched' = … ∨ (pendingFatal ∧
  pendingFatal')` — set only when the latch survives the refusal); the
  .tla comment now states the two blind shapes it excludes. Full suite
  re-run (run4): ALL PASS with base counts identical (15667/6330 — the
  survivor semantics is count-neutral on the correct model and splits only
  under the defect), L2 witness still firing.
