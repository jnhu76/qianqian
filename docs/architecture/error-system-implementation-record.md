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
   the default hook's raw fd 2 write before its `catch_unwind`; §9). A thread that never
   registered chains (fail-open). The registered set is the threads whose progress is
   awaited: stdin reader, decode worker, render leg, and (since #321) the observation
   analyst. The hook's critical path is atomics-only and never renders: the counter
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

**DG-4b registry membership — SETTLED by the owner's #287 review (§9); extended by
#321.** The predicate is the outlet's own thread fact (`is_alpha_producer`), not a
name list, and the posture installs at transport startup. The review's finding showed
the "reader-only" reading was wrong in principle: a render/decode panic that chains to
the previous hook can block in the default hook's direct raw fd 2 write (rustc 1.97.1:
`sys::stdio::panic_output` returns the raw `FileDesc`, not the `io::stderr()` handle)
BEFORE its `catch_unwind`, and the threads that `join` those legs — the session
teardown's worker join, the render cone that must run after the catch — would then
wait on a diagnostic sink.

The same criterion applied incompletely to the observation analyst: its progress is
joined inside the decode worker's exit funnel (`session.rs`: `observation.close();
analyst.join()`), yet it did not register, so a panicking analyst × blocked stderr
could wedge the funnel before `worker.join()` and leave K0 without a `DisposeVerdict`.
#321 adds the analyst registration at its thread start (one registration per fresh
thread, created and dropped on that thread). This is a defect correction under the
adopted criterion, not a new obligation.

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
                                         + #321 analyst join E2E (evidence/analyst-join-321/)
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
| P1 — `previous(info)` on render/decode panics | The hook's isolation predicate was a thread-NAME list (`qianqian-stdin`), so a panic on a registered decode/render thread chained into an unknown hook — the default one writes directly to raw fd 2 and blocks BEFORE `catch_unwind`, so the join/completion paths that wait on those threads were gated on a diagnostic sink | The predicate is the outlet's own registration fact (`is_alpha_producer`); the posture installs at transport startup, before any decode worker/render thread exists | `machine_input/tests.rs` DG-4b witness: registered-noncritical-name isolated, reader-named-unregistered chains, and a registered thread panicking twice with a FULL stderr still unwinds and ends |
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
  the state space. The delta re-review caught two
  silent application failures in the first wording commit (the A-2
  WorkerSeek-comment sentence and the A-5 unmodeled-list declaration did
  not match their targets and were not applied, while the fix inventory
  claimed them — the same record-vs-artifact defect class the slice had
  flagged at A-1); both were then applied for real with per-edit
  occurrence asserts, together with the inventory correction (this
  paragraph). Final state at HEAD: eligibility "lock-free reads" → three
  separate single-lock acquisitions (ledger); the W-B first-half window's
  nondeterministic-placement idiom disclosed in the ledger AND the
  WorkerSeek comment; EofClose's commit-only gate labeled a model scoping
  choice in README/.tla; processing-failure (session.rs:827-831/:856-866)
  and panic (:914-916) exits declared outside the frozen six-path
  inventory in the .tla header and README unmodeled lists. The fifth
  finding (witness sensitivity overclaim) had already been independently
  found by the mutation/witness slice's residual round and fixed at
  9f95eef with bidirectional counterfactual verification.
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

### 13.6 External final-review round (PR #292, 2026-10-17) — atomicity convergence

The owner-routed independent final review of PR #292 returned REVISE
(1×P1, 2×P2; `FULL_CAUSAL_FIDELITY = REVISE`, `PRODUCTION_DEFECT_FOUND =
NO`, `MODEL_REFINEMENT_GAP = YES`). All three findings were verified
against BASE_SHA source and fixed on the same PR; no production change.

- **P1 (confirmed) — failure-route atomicity was over-fused.**
  `ReadSurfacesFatal` fused `decode_failed` (publication+settlement),
  `edge.fail()` and the `worker_exited()` funnel into one action; only the
  publication+settlement pair shares one `publish_evidence` lock hold
  (completion.rs:1319-1331). `edge.fail()` (session.rs:898, edge lock) and
  the funnel (:928) are outside it, and `publish_evidence` drops evidence
  once the outcome is settled (:1322-1324) — the old model's unconditional
  `workerGone=TRUE` on this route was a fusion artifact. The lost legal
  interleaving: after the failure settlement commits `Failed{decode}`,
  `request_stop` still records intent and still executes its data-plane
  stop ("after settlement: nothing changes" refers to the outcome, not to
  the actions), so `committedOutcome=failedDecode ∧ edgeTerminal=stopped`
  is a legal terminal cell (edge first-wins does not overwrite; D11 rule 1
  never reclassifies). Fix: split into `FatalFailurePublish`
  (publication+settlement atomic; `SeekMutatedThenFailed` split the same
  way) → `WorkerEdgeFailAdvance` (first-wins edge advancement) →
  `WorkerExitFunnel` (`workerGone' = (committedOutcome = "none")` — FALSE
  on the failure route, the faithful dropped-publication value);
  `RecordStopIntent` lost its settle gate (production `request_stop` is
  "safe from any thread and any state"). Acceptance: new witness
  `W_FAILED_DECODE_WITH_STOPPED_EDGE` reachable (6-state counterexample —
  TLC counts include the initial state — archived, exactly the reviewer's
  sequence).
- **P2 (confirmed) — the eligibility→provider window rested on an unproven
  union argument.** The A-2 "nondeterministic intent placement" idiom was
  judged insufficient (correctly). Fix: explicit split
  `SeekEligibilityPass` (atomic checks; consumes the seek_aborted verdict)
  → `SeekProviderBegin` (no re-check — production calls the provider after
  the checks with no re-examination, session.rs:620-650); new worker value
  `seek_eligible`; the A-2 union argument retired from model and ledger
  (PENDING_FATAL_DAG §1 W-B). Acceptance: base space 6330 → 7470 distinct
  (the window states); the D-T4 chain trace passes through both actions.
- **P2 (confirmed) — D-T4 was a double defect injection.** It changed both
  latch erasure and worker destination (→ idle rerun), violating
  one-mutation-one-defect, and the universal invariant fired at the landing
  state before any surfacing. Fix: single defect (skip erasure only;
  destination stays `repositioned`); `FatalFailurePublish` gained a
  `repositioned` arm (see §13.7 for the re-review correction that removed
  an initial `committed` gate whose justification overstated production);
  added named-diagnostic
  `DiagnosticNoFailurePublicationAfterLanding` (the publication half of
  `NoOldFatalAfterApplied`; referenced only by the D-T4 diagnostic cfg,
  not in the 11-invariant baseline — the AGENTS.md "named as diagnostics"
  provision). Acceptance: the D-T4 counterexample now walks the full
  surfacing chain (archive: counterexample-DT4-surfacing-chain-v2.log,
  8 states, direct — no commit needed); the universal invariant's
  violation recorded in a disclosed second run
  (counterexample-DT4-full-invariants-v2.log, terminates at the landing
  state as before).

Post-fix full suite (f5-full-suite-run5-finalrev-fixes.log): first run all
green — base PASS 22349/7470 exhausted; 12/12 witnesses reachable; 11/11
mutations counterexample-witnessed; M1–M7 no regression. Also recorded in
RESULTS: under TLC `-continue` a violating state is reported but not
expanded, so a mutation's reported violation set is complete over its
generated states (D-T1's extra `NoOldFatalAfterApplied` firing is a
mutant-only post-cut re-eligibility artifact; M7's resumed worker stopping
into a non-matrix exit cell is likewise mutant-only). PENDING_FATAL_DAG
bumped to REV.2 (W-E window, corrected atomicity rulings, `seek_eligible`
position row, D-T4 single-defect row). No production file touched; BASE_SHA
line anchors unchanged.

### 13.7 Re-review round (same day, 2026-10-17) — delta fixes after the final-review fixes

Two fresh-context re-reviewers ran against 2a516bf after the §13.6 fixes.

- **Causal-fidelity re-review: APPROVE.** All three §13.6 fixes verified
  faithful against source (the publish_evidence lock hold, request_stop's
  any-state behavior, the three separate single-lock eligibility
  acquisitions, the D-T4 single-defect path); the full suite re-run was
  line-identical to the archived run5 log; every TerminalCellConformance
  cell was independently re-derived against the new action set with no
  counterexample (including the compatibility arguments for the relaxed
  `RecordStopIntent` against cells (2e)/(2g)).
- **Evidence-integrity re-review: REVISE (docs-only).** Suite reproduced
  exactly; the four evidence-archive files verified state-by-state;
  findings: the stale PENDING_FATAL_DAG §8 D-T4 row contradicting the
  REV.2 header claim, the run1/run2 base-count wording in RESULTS'
  inventory, and two count/notation P3s.
- **Delta fixes applied** (docs + one model guard line):
  1. PENDING_FATAL_DAG §8 D-T4 row rewritten (single defect, diagnostic
     target, direct chain) — the edit had been planned but never applied
     in 2a516bf while the REV.2 header claimed it.
  2. The `committed` gate on `FatalFailurePublish`'s `repositioned` arm
     was removed: its justification ("production resumes reading only
     after commit routing, :775-778") overstated production — the cutover
     wait's `Aborted` branch (session.rs:721) exits the second wait (:750)
     and resumes reading (:775-778) without a commit. The arm stays
     base-dead (Applied always erases), so the base space is unchanged
     point-for-point (22349/7470, re-verified: f5-base-rewiden-recheck-
     run5c.log); the D-T4 counterexample is now a direct 8-state chain
     (counterexample-DT4-surfacing-chain-v2.log; both pre-fix D-T4 logs
     retained, marked superseded in the RESULTS inventory).
  3. Line-citation corrections (:671-672 for the Mutated arm's
     release+publish, :673 for its edge.fail), the witness counterexample
     stated in TLC state-counting convention (6 states = initial + 5
     action steps), the workerGone action count (four unconditional real
     exits + the conditional funnel), the run1/run2 counts
     (13117/5216) restored in the RESULTS inventory, and a disclosure note
     for the pre-existing benign family where LegObserve* records
     drainVerdict after settlement (inert in-model; TerminalCommit is
     gated on the unsettled outcome).

---

## 14. Machine stdout/stderr failure conformance round (#290, 2026-10-17) — E-9 three modes + E-8 main-thread report sites

> **Truth class: EVIDENCE RECORD for issue #290. NOT authority; ratifies nothing.**
> BASE_SHA `6a9b05101022ac8534dc1da791eda5f3a12a1a4a` (all line anchors below verified
> at this SHA). Branch `evidence/machine-stdout-conformance-290`. Scope: verify the
> EXISTING documented host-failure classes (T5-7; playback-execution-model D3/D5 §4.8;
> migration §1.7 two-class split; reconciliation §8 E-8/E-9) against runtime reality,
> and produce the D-D owner decision package. **PRODUCTION_BEHAVIOR_CHANGE = NONE;
> D11/K0/ABI/ADR changes = NONE.** D-D Option A product work (if any) is a separate
> owner-authorized PR; this round freezes scenarios first (§14.1–14.6), then records
> results (§14.8) against them.

### 14.1 SOURCE_DAG — reader status stdout (one admitted operation)

```text
physical stdin read            read_input machine_input.rs:317-319 (catch #R-read at :319)
  ↓ read Ok(line)
input.line(line, handle, out)  machine_input.rs:322
  ↓ admit() :114               operation_active=true under bookkeeping mutex (None ⇒ admission closed ⇒ line() false ⇒ reader loop ends)
  ↓ catch #R-dispatch :119     covers parse + command dispatch + format_status + output closure
  ↓   Status ⇒ output(Stdout, format_status(observe))   :131-134
  ↓     output closure         spawn_reader :269-278
  ↓       Stdout ⇒ print!("{line}") :273;  let _ = io::stdout().flush() :274
  ↓       Stderr ⇒ eprintln!("{line}") :276
  ↓ catch is_err ⇒ operation.fail(HostFailure::Panic, handle) :142-145; line returns false ⇒ reader exits
operation.fail ⇒ fail_with_response :166-174
  ↓ record first_failure (get_or_insert, under mutex)   :171   ← RECORD precedes RESPONSE
  ↓ response :160-162: if handle.observe().terminal_outcome.is_none() ⇒ handle.request_stop()
Operation::drop :177-183       operation_active=false + notify  ← ACK after record+response
  ↓ (reader loop exited; read_input returned)
entry finish_episode           entry.rs:548-579
wait_terminal :553 → dispose :554 → HostInput::seal :559
  seal :84-100: admission_closed=true → condvar-wait !operation_active (ACK) → read first_failure
  → report(Stderr, failure.report()) :561 → outcome_report :563-566 → machine_exit_code(host_failed⇒1) :574-578
```

Boundary answers (Phase-A questions):

1. `print!` is covered by catch **#R-dispatch** (machine_input.rs:119), inside its
   admitted operation.
2. `HostFailure::Panic` is recorded at :143 (dispatch/output unwind), :296 (catch
   #R-outer over the whole reader closure), :331 (read unwind).
3. The conditional `request_stop()` runs at :160-162 — only when the observation shows
   no committed terminal; a racing terminal makes the existing command inert.
4. Yes — `Operation::drop` (the ack) runs when `line()` returns, which is after
   `fail()` (record + response) completed; the condvar wait in `seal` observes it.
5. `seal` closes admission, then waits on the completion condvar until
   `operation_active == false`, then reads `first_failure` under the same mutex
   (:84-100) — the result linearization point.
6. A blocked/blocked-forever output sink stalls the reader INSIDE the admitted
   operation (`print!`/`flush`/`eprintln!` do not return) ⇒ the Operation never drops
   ⇒ `seal` waits at :87-93 indefinitely ("an active output effect can delay seal",
   module doc machine_input.rs:4; ledger #5).
7. Yes — `let _ = io::stdout().flush()` at :274 discards the flush result entirely: no
   record, no Stop, no exit contribution, no report. It bypasses the HostFailure path
   completely (the D-D subject).
8. No. The panic hook (install_panic_hook_posture :217-244) runs BEFORE
   catch_unwind and only performs diagnostic bookkeeping (class-α suppression /
   previous-hook chaining); classification is owned by the catchers. It cannot change
   a category.
9. Failure admitted before seal ⇒ first_failure fixed at seal ⇒ exit 1 regardless of
   the genuine terminal (machine_exit_code :160-170). Terminal committed before the
   failure response ⇒ Stop arm inert, outcome keeps its D11 class. Failure arriving
   after admission closure ⇒ `failure()`/`line()` admission returns None ⇒ discarded
   without effect (:102-106, :114-115).

### 14.2 CATCH_BOUNDARY_MAP

| Catch | Covers | On unwind |
|---|---|---|
| #R-read :319 | one physical `read_line` | `failure(HostFailure::Panic)` :330-333 ⇒ record + conditional Stop (admits a fresh operation) |
| #R-dispatch :119 | parse + dispatch + `format_status` + output closure (incl. `print!`, `flush`, `eprintln!`) | `operation.fail(HostFailure::Panic)` :143 ⇒ record → conditional Stop → (drop ⇒ ack) ⇒ reader exits |
| #R-outer :295 | the entire reader closure | `failure(HostFailure::Panic)` :296 |
| hook (process-wide, :217-244) | nothing semantic | class-α threads: `note_critical_panic()` only; others chain to previous hook |
| main thread | **no catch** | unwind out of `run()` ⇒ process exit 101 (no `panic=abort`; Rust ignores SIGPIPE ⇒ closed pipes yield EPIPE write errors, not signal death). A closed stdio fd (EBADF) is deliberately swallowed by Rust std (pretends success) — probed on Linux this round; on Windows the swallow is std-source-derived, not probed here. A closed HANDLE never panics `print!`; only a real write error to an open descriptor does |

### 14.3 HOST_FAILURE_ORDER_MAP (admitted operation)

```text
admit (:114) → work → [unwind ⇒ record :171 → response :160-162] → drop/ack :177-183 →
owner seal: close admission → wait ack → read first_failure → reports → exit contribution
```

Physical reads are outside admission (:305-309); post-closure events are inert;
first-failure is FIRST-WINS and bounded (one `Option<HostFailure>`).

### 14.4 REPORT_SITE_INVENTORY (E-8 scope, re-verified at 6a9b051)

Corrects the stale claim that "#1–#3 run on Linux without playback": #2/#3 live inside
`reference_player_transport`, which is `#[cfg(feature = "playback")]` (entry.rs:331-332).

| # | Site (HEAD) | Thread / stage | Gate | Note |
|---|---|---|---|---|
| 1 | entry.rs:70 (`print!` usage), :74 (`println!` version), :102-103 (`eprintln!` arg error) | main, pre-episode | none | Linux-portable real-binary |
| 2 | entry.rs:371-373 (`println!("scanning…")` + `let _ = flush()` :373 — second discarded flush) | main, reference pre-open | `playback` | not compiled without the feature |
| 3 | entry.rs:378 fail-stop, :387 shell failed, :404 open refused, :406 activation failure, :414 expansion refusal | main, reference open/shell phase | `playback` | ordering per arm: the :387 shell-failure report precedes `player.quit()` :390 (whose disposal); the reference failure-clean arm (`ActivationFailedClean`, :405-407) disposes the attempted root BEFORE its report (player.rs `failure_clean_start`); the report-precedes-dispose shape is the machine NotEstablished path (site #7, :515→:516) — the old ledger-#3 anchor (entry.rs:380-381) no longer exists |
| 4 | entry.rs:529-535 (`println! source:` / `println! playing {}`) | main, machine post-establishment, BEFORE reader spawn :538 | `any(playback, test)` | broken write ⇒ 101 skipping wait_terminal/dispose/seal/reader spawn |
| 5 | machine_input.rs:269-278 | stdin reader, per command | `any(playback, test)` (module gate lib.rs:42-43; no per-site cfg) | **E-9 scope** (protocol row) |
| 6 | entry.rs:561-573 via `emit_machine_report` :582-587 (machine; AFTER seal); entry.rs:392-397 (reference; after quit) | main, terminal/disposal/final reports | machine: `any(playback, test)`; reference: `playback` | panic skips LATER reports only (episode already disposed; D11 committed) |
| 7 | entry.rs:515 (`eprintln!` activation report) — **precedes `dispose()` :516** | main, machine NotEstablished | `any(playback, test)` | report panic ⇒ unwind skips dispose (root leak — `QianqianApp` has no Drop teardown) |

### 14.5 FEATURE_PLATFORM_MATRIX + evidence grades

Environment facts established this round (probe logs were in the round
evidence archive — LOST after execution, §14.8.8; the values below are the
record's own transcription and the Windows rows are re-obtainable only by a
fresh cross-build + interop session):

```text
native/build/artifacts/libsongcore.a = Windows COFF (mingw) only; Linux ELF SongCore
artifact NOT available in this environment; a Linux FFmpeg+SongCore rebuild is
#172/#288 release-engineering territory and was NOT attempted to completion.
⇒ no Linux playback-feature build (binary or test) can link here.

x86_64-pc-windows-gnu cross-build with --features playback SUCCEEDS against the
mingw artifact (target/x86_64-pc-windows-gnu/debug/qianqian{,-headless}.exe,
PE32+, rustc 1.97.1). WSL2 interop RUNS the exe on the real Windows host:

  ./qianqian.exe --version                                  ⇒ "qianqian 0.1.0", rc=0
  ./qianqian.exe --machine play <real wav>                  ⇒ source:/playing rows,
                                                              reader answered `status`,
                                                              "EOF: played out completely",
                                                              rc=0  (REAL WASAPI episode)
  ./qianqian.exe --machine play C:\nonexistent\garbage.wav  ⇒ stderr
                                                              "playback session failed to
                                                              activate: decode open failed: …
                                                              (os error 3)", rc=1, stdout empty
  argv reference transport                                  ⇒ "scanning …" row + TUI rendered
                                                              under interop piped stdio; the TUI
                                                              does not terminate cleanly on a
                                                              piped 'q' (no clean exit evidence)

Interop limitation (probe-evidenced): the Windows-interop stdio proxy ABSORBS the
exe's writes; closing the WSL-side reader does not fail the exe's writes (broken-pipe
probe returned rc=0 through interop). A CLOSED handle (>&-) is swallowed by Rust std's
EBADF guard (Linux-probed this round; Windows half source-derived) and additionally
produces interop-layer noise ("write failed 22: Invalid argument" on stderr, not a
Rust panic — interop artifact, not product evidence). ⇒ broken-pipe / full-pipe
failure-mode experiments on the Windows exe require a native Windows console
session (#288 host); they are NOT reproducible under interop.
```

Evidence grades used below:

```text
L2_REAL_BINARY      production binary, real assembly/runtime, real OS pipe choreography
L1_TEST_TRANSPORT   real production dispatch/record/ack/seal/report code driven in a
                    test-binary subprocess with a fixture episode; injection at the
                    operation boundary only; real OS pipe choreography
L2_STD_PROBE        std-only characterization probe (toolchain/OS behavior; no qianqian code)
NOT_RUN(reason)     honestly not executed
```

### 14.6 BDD scenarios (frozen before implementation; results in §14.8)

**B1 — reader stdout print failure** (E-9 mode 1)

```text
GIVEN   an established, unsettled episode; the reader operation admitted (:114)
WHEN    the real status stdout protocol write fails ⇒ `print!` panics
THEN    panic caught by #R-dispatch; HostFailure::Panic recorded FIRST (:171);
        conditional request_stop routed to the unsettled episode (:160-162);
        Operation ack after record+response (:177-183); seal waits ack then returns
        Some(Panic); host exit 1 (machine_exit_code); D11 terminal NOT forged Failed
        (real Stopped/Completed from genuine evidence); host-failure report text is
        HostFailure::Panic.report() on stderr
ORACLE  exit code, D11 outcome class, failure_diagnostic absence, seal value, report
        lines — exact-value assertions, not permissive
SIGHT CONTROL  same child with a HEALTHY stdout sink ⇒ normal completion + correct
        rows (proves the pipe choreography does not itself create the failure)
FALSIFIERS  (a) panic escaping the catch ⇒ reader-thread crash (exit 101 +
        uncaught-panic stderr naming the reader thread); (b) silent diagnostic-style
        drop ⇒ seal None + exit 0; (c) record after response/ack ⇒ seal_witness
        snapshot ≠ 5 (existing M-F/M-S mutation class); (d) D11 forged Failed ⇒
        outcome report "playback failed"
```

**B2 — reader stderr write failure** (E-9 mode 1, stderr arm)

```text
GIVEN   an admitted reader operation producing a real stderr diagnostic
        ("ignored input: …" from an unparseable line, :139; "not wired yet", :135-138;
        "cannot read seek time", :126-130)
WHEN    that eprintln! write fails ⇒ panic
THEN    same #R-dispatch contract as B1 (one admitted-operation failure contract;
        migration §1.7: protocol write failure = host-operation failure)
OBSERVED-SHAPE NOTE  the subsequent OWNER host-failure report targets the same broken
        stderr (entry.rs:561 ⇒ emit_machine_report eprintln!) ⇒ main-thread panic ⇒
        recorded exit 101 AFTER D11 committed — ledger #6 class ("skipping later
        reports only"); this is the two-class boundary made observable and is
        recorded verbatim, not normalized away
```

**B3 — flush-Err checkpoint** (E-9 mode 2; D-D subject)

```text
GIVEN   `print!` genuinely completed (write error-free) — never after a write panic
WHEN    the explicit flush() at :274 would return Err
THEN    record whether that state is reachable WITHOUT changing production output
        shape (every reader stdout line is '\n'-terminated), the std writer, or
        ordering
CLASS LADDER (pre-registered; pick exactly one)
        REAL_PRODUCTION_REPRODUCED     constructed on the production path as-is
        INJECTED_WRITER_ONLY           only a non-production writer/shape constructs it
        NOT_REPRODUCED_WITH_SCOPED_ASSUMPTIONS
                                       not constructed; scope, attempts, toolchain,
                                       uncovered cases stated
        NOT_RUN                        not executed
RESEARCH QUESTIONS (pre-registered)
        RQ1 format_status return paths all end '\n'?      (status.rs:24-74)
        RQ2 pinned toolchain Stdout buffering?            (std source + behavior)
        RQ3 pipe/terminal/file differences?               (probe)
        RQ4 after a successful print!, can the buffer still hold unflushed data?
        RQ5 can another writer leave buffered data between print! and flush?
            (single-writer discipline per transport phase — §14.1)
        RQ6 is there a real independent error window without reshaping output?
PROBE DESIGN (L2_STD_PROBE, std-only, no qianqian code)
        PA newline-terminated: print!('\n'-terminated line) succeeds into a pipe;
        parent closes read end; child flush() ⇒ if Ok, flush performed NO write
        (a write would EPIPE); control: a second print! then MUST panic
        PB no-newline tail (INJECTED shape): print! without '\n' stays buffered;
        flush() performs the fd write ⇒ can Err while print! succeeded
IN-REPO L1   output closure that swallows a flush-shaped Err exactly like :274 ⇒
        line() returns true, no record, no Stop, seal None (current behavior is
        INDIFFERENT to flush Err); negative control inside the same test: a
        closure that PANICS ⇒ false + Panic recorded (proves the harness
        distinguishes swallow from panic)
```

**B4 — blocked output** (E-9 mode 3)

```text
GIVEN   reader admitted; writing to a real sink that stops consuming
WHEN    sink stays blocked
THEN    reader stays inside its admitted operation; seal cannot complete; no
        "reader joined" claim; no synthetic completion; after the sink releases the
        real operation completes and classification is CORRECT (blocking is not a
        failure: no record, no Stop, exit per genuine outcome)
WATCHDOG  kill/timeout is safety equipment only; if it fires the result is
        INCONCLUSIVE/BLOCKED, never PASS
```

**B5 — late output failure vs seal boundary**

```text
Covered by existing oracles (cited, executed again this round):
  seal_retains_admitted_postclosure_output_failure_and_rejects_fresh_work  (retention + rejection)
  admitted_output_panic_records_before_ack_even_during_seal                (ack ordering)
  postseal_commands_reports_and_failures_are_inert                          (late events)
  first_failure_storage_is_bounded_and_sealed_result_immutable              (first-failure cut)
  preseal_failure_preserves_natural_completed_and_runtime_failed            (D11 immutability)
```

**B6 — main-thread broken pipe (E-8, per site)**

```text
GIVEN   the target report site's real execution path is reached (sight control:
        healthy-sink run of the same site captures the exact report line first)
WHEN    its stdout/stderr write hits a closed downstream pipe
THEN    record the ACTUAL process behavior (panic 101 / caught / other) per platform
        lane; panic paths must show exit 101 + std panic text naming the site
ATTRIBUTION  healthy-control content match + (stdout-broken cases) panic location in
        the captured default-hook message + partial-output witnesses; a process exit
        not demonstrably caused by the named site is NOT attributed to it
DISPOSAL-SKIP (sites #3/#4/#7)  runtime disposal witness only via a test-transport
        child that observes its disposed marker after catching the unwind
        (disposed=false ⇒ skip witnessed at L1; the real-binary skip remains
        source-inferable — no runtime witness claimed without one)
```

### 14.7 NOT_RUN register (pre-declared)

```text
E9_WIN_BROKEN_SINK          reader print-panic/broken pipe under REAL Windows playback —
                            interop stdio proxy absorbs downstream closure (§14.5); needs
                            native Windows console session
E9_WIN_BLOCKED_SINK         same interop limitation for a deterministic full-pipe
E8_WIN_BROKEN_PIPE_SITES    all E-8 broken-pipe rows on Windows — same limitation
E8_LINUX_PLAYBACK_REAL_BIN  #2/#3/#7 + the #6 reference arm (:392-397) real-
                            binary on Linux — no Linux ELF SongCore
                            artifact in this environment (§14.5)
E8_SITE3_FAILSTOP_ARM       reference fail-stop (entry.rs:378) needs a latched §G.6
                            violation — not constructible by argv; not forced
E9_TERMINAL_SINK_PROBE      B3 flush-Err probe against a real terminal — no tty in
                            this harness; the WSL-interop console proxy is a pipe
                            (§14.5), so the pipe result is the covering evidence
```

Post-freeze amendments to §14.1–§14.8 (declared; the frozen §14.6 scenario
text is byte-unchanged since 9386592):

```text
§14.1  admit-line wording precision (None ⇒ line() false ⇒ reader loop ends —
       not merely "drop the line"); R1 review NOTE.
§14.4  site-#3 note rewritten: the imported ledger-#3 anchor (entry.rs:380-381)
       no longer exists; the ordering facts were re-derived per arm (R1 review
       P1: the reference failure-clean arm disposes BEFORE its report).
§14.4  site-#5 gate cell corrected "always" → any(playback, test) — the module
       gate (lib.rs:42-43) governs; verified against source (R1 review P1).
§14.7  E8_LINUX_PLAYBACK_REAL_BIN extended with the #6 reference arm
       (:392-397) — the matrix's Linux gap was registered incompletely
       (R3 review); E9_TERMINAL_SINK_PROBE added after the B3 probe ran
       (terminal lane unavailable).
§14.8.3 B3 probe replaced by the P1 repair (owner review of this PR,
       2026-10-17): the v1 scratch probe's PA and interleave arms had a
       premature flush between print! and the observed flush (and the PA-2
       control ran on a different process's sink), so their FLUSH=Ok
       observations were non-discriminating and are WITHDRAWN; the v2 probe
       is the committed `e9_flush_probe_*` tests (one sink, no intermediate
       flush, the row's arrival observed by the parent, same-sink second
       print! as the brokenness proof). §14.8.8(b)/(g) updated accordingly.
§14.6/§14.7 B2 scheduling-sensitivity round (owner directive, 2026-10-18;
       evidence: q290-evidence/290-r2-flushprobe first-round red archive +
       q290-evidence/290-r3-b2/): the B2 red (`e9_real_reader_stderr_epipe_…`
       30s marker timeout, once, under the full local gate) was classified by
       building child-aware forensics (byte-arrival timeline, DIED-vs-BLOCKED
       watchdog, /proc thread snapshots, RUN-RETURNED/TERMINAL boundary
       witnesses) and re-running under a bounded CPU-saturated stress config.
       Root cause named with a standalone reproducer (zero qianqian code):
       a fork→exec forkling window — a concurrently spawned child briefly
       holds the WHOLE parent fd table (its own + other tests' pipe read
       ends, O_CLOEXEC not yet applied) — makes a parent-side close(2)
       silently ineffective, so the "broken sink" never breaks: PA-2's
       second print! succeeded (PRINT_PANIC=false), PB's flush returned Ok,
       site6/site7 transports completed on unbroken sinks (UNWIND=0), and
       the epipe arms wedged at wait_terminal (no panic → no Stop → the test
       source never self-completes; all child threads STATIC in futex).
       A/B: 46/100 anomalous FLUSH=Ok with parallel spawns under CPU
       saturation vs 0/60 sequential; the foreign read-end holders were
       named by /proc fd walks (mode=R, O_CLOEXEC set, not the victim's
       parent or child). Fix (test orchestration only;
       PRODUCTION_BEHAVIOR_CHANGE=NONE, D11/K0/ADR/ABI unchanged):
       (a) premise observability — every epipe/site child parks at a gate
       the parent releases only after the close (GATE-READY-witnessed for
       the epipes and site6; site4/site7 park at their pre-existing
       unmarked stdin gates), then runs a canary write that MUST panic
       (`require_broken_sink`; the canary's default-hook text is silenced
       with a momentary hook swap so it cannot pre-satisfy the
       choreography's own "failed printing to stdout" message assertions);
       an unbroken
       sink is a deterministic Q290-PREMISE-REFUTED red (child exit 3,
       named by the watchdog), never a silent anomaly — the stdout canary is
       '\n'-terminated so the LineWriter writes it directly and the buffer
       state each property depends on is untouched (no v1-style premature
       flush); (b) spawn-window exclusion — a process-wide
       `test_spawn_lock` serializes {spawn → first witness} against every
       close (all forkling windows in the test binary are lock-covered, so
       a lock-held close is structurally forkling-free); (c) the child-aware
       watchdog (DIED vs BLOCKED + timeline + /proc + gdb-when-permitted)
       classifies any residual red. Verification: module solo 45/45; under
       20-busy-loop saturation 20/20 module greens (pre-fix 8/10 red) and
       3/3 full-library greens incl. concurrent entry::shutdown_tests
       spawns; canary negative control NC-C (parent skips the close) reds
       deterministically with PREMISE-REFUTED (nc-c-canary-refuted.log).
       The 30s marker-watchdog bound is UNCHANGED (owner: classification,
       not a wider promise). Property texts of the v2 flush probe are
       byte-preserved in meaning: PA/interleave prints stay pre-gate, the
       PB tail print moved post-gate so the canary's newline can never
       carry the tail out, and no arm gained an intermediate flush.
       Post-author sufficiency review (owner-directed; 3 fresh-context
       reviewers, verdicts all APPROVE_WITH_NITS): fixed an uncovered
       spawn in the site1 version arm, lock poisoning (one watchdog red no
       longer cascades into PoisonError reds), an unbounded gdb wait on
       the failure path, the canary-hook message taint named above, and
       record wording; the archive's batch receipts were retroactively
       sidecarred.
```

### 14.8 Results

Executed 2026-10-17 on branch `evidence/machine-stdout-conformance-290` at base
6a9b051 + C1 9386592, toolchain rustc 1.97.1 (8bab26f4f 2026-07-14 — the std whose
panic text/locations appear below). Lanes: Linux native (test transport + REAL
pipes; no Linux playback artifact exists — §14.5), Windows only via WSL interop
(healthy-sink real-Windows rows). Every run was a separate archived full log
per the round's evidence contract; the session-local archive was lost after
execution (disclosed in §14.8.8), so the surviving record is the committed
oracles below plus the PR CI logs. Exact-value assertions below are quoted
from the test oracles, not paraphrased.

#### 14.8.1 B1 — reader stdout print failure (EVIDENCED; lane: test-transport child + REAL pipe)

Child = REAL `spawn_reader` over real stdin inside a REAL admitted operation
(test transport; grade L1_TEST_TRANSPORT per §14.5 — the real reader thread,
real pipes, in-process composition); parent polls the arming marker on the
healthy stderr, closes the stdout read end, then writes `status`.

```text
witness (child stderr, captured):
  Q290-ARMED
  Q290-E9-UNWIND=1 Q290-E9-D11=Stopped Q290-E9-DISPOSED=1 Q290-E9-SEAL=Some(Panic)
asserted by the parent (exact-value):
  HostFailure::Panic.report() reached the healthy stderr;
  "failed printing to stdout" + "Broken pipe" on stderr — the transport's
  final OUTCOME ROW panicked on MAIN against the same broken stdout and the
  panic text reached the healthy stderr (ledger #6: later reports only, after
  D11 committed; the host-failure report itself SUCCEEDED on the healthy
  stderr — that is the first asserted bullet);
  child exit 0 through its witness path (no escaped panic, no crash exit).
```

The parent wrote only `status` — the `Stopped` terminal was committed by the
host failure's own conditional Stop response (the stop-routing property,
machine_input.rs:160-162); the host failure is a separate record, never merged
into the terminal fact. Terminal preservation against a PRE-EXISTING user stop
is a different property, carried by the L1 oracles
`existing_user_stop_and_host_failure_keep_one_genuine_terminal` and
`preseal_failure_preserves_natural_completed_and_runtime_failed` (green, B5).
Sight control (`*-healthy` twin, same choreography, no break): full row set
(`source:`, `playing`, `outcome:`, `stopped before completion`), stderr carries
no host failure, exit 0 — the pipe choreography itself creates no failure.
Falsifiers: (a) escaped panic — at this lane `UNWIND=1` cannot by itself
distinguish the #R-dispatch catch from the #R-outer boundary (both classify
identically); the catch-holding property is carried by the L1 oracles
(`reader_output_panic_full_transport_contract` and the flush test's panic
arm), which fail loudly if the catch is removed; (b) silent drop ⇒
`SEAL=Some(Panic)` asserted; (c) record-after-response ⇒ B5 oracles green;
(d) forged Failed ⇒ D11=Stopped asserted. Tests:
`e9_real_reader_stdout_epipe_*`, `e9_real_reader_stdout_healthy_*`.

#### 14.8.2 B2 — reader stderr write failure (EVIDENCED; lane: test-transport child + REAL pipe)

Same child shape; parent verifies the transport rows reached the open pipe
(`source:`, `playing`) BEFORE closing stderr, then writes `badcmd` (real
`:139` "ignored input" diagnostic path). Captured/asserted:

```text
  Q290-E9-UNWIND=1 Q290-E9-D11=Stopped Q290-E9-DISPOSED=1 Q290-E9-SEAL=Some(Panic)
  stdout (healthy) does NOT contain "stopped before completion" — the owner's
  outcome row was skipped by the host-failure report's own panic on the same
  broken stderr (the OBSERVED-SHAPE NOTE of §14.6-B2, recorded verbatim).
```

Sight control (`stderr-healthy`): the diagnostic reaches the healthy stderr,
full report set completes, no host failure. Tests: `e9_real_reader_stderr_epipe_*`,
`e9_real_reader_stderr_healthy_*`.

#### 14.8.3 B3 — flush-Err checkpoint: class verdict `INJECTED_WRITER_ONLY`

The frozen ladder asks for exactly one class. The executed verdict is
**INJECTED_WRITER_ONLY**, with the stronger scoped statement: with the current
production output shape there is **no independent error window** at the `:274`
flush — every condition that would make `flush()` fail already made the
preceding `print!` fail (and that path is classified as `HostFailure::Panic`).

**P1 probe repair (owner review of this PR, 2026-10-17).** The round's v1
scratch probe (kept below, SUPERSEDED) was defective in a way that voids its
central observation: in its PA and interleave arms an explicit `flush()` sat
BETWEEN the `print!` and the observed flush, so the observed `FLUSH=Ok(())`
proved only "nothing pending after an explicit flush" — true under ANY
buffering regime — and its PA-2 control ran on a DIFFERENT process's sink, so
it could not prove PA's own sink was broken at flush time. Disposition: the
v1 PA and interleave Ok observations are WITHDRAWN as non-discriminating;
the v1 PB Err, PA-2 panic and EBADF observations stand (their conclusions do
not rest on the voided step). The v2 probe below replaces the withdrawn arms
as COMMITTED, CI-executable tests (`e9_flush_probe_*` in
`apps/headless/src/machine_input/tests.rs`, cfg(unix)): one child, ONE sink,
no intermediate flush, the row's arrival OBSERVED by the parent before the
close (the premise is synchronized, not assumed — a missing row REFUTES it
and fails the test), and the same-sink second `print!` as the brokenness
proof. The v2 PA test doubles as the automated tripwire for revisit trigger
T3 (decision-record §7.4): a std buffering regression (the '\n' no longer
draining, or the flush writing on an empty buffer) fails it via its premise,
flush-value, or brokenness oracle.

RQ answers (evidence in parentheses):

```text
RQ1 all format_status paths end '\n'          YES (status.rs:24-74, 8/8 push paths; frozen §14.6)
RQ2 Stdout buffering                          LineWriter, sink-independent (std source;
                                              demonstrated behaviorally by the v2 PB
                                              test: a no-newline print! stays
                                              buffered on a real pipe)
RQ3 pipe/terminal/file                        pipe: v2 PA/PB tests; regular file: v1
                                              probe (corroborative only, receipt lost,
                                              §14.8.8); terminal: NOT_RUN (no tty in
                                              this harness; the interop proxy is a
                                              pipe — §14.5) — registered §14.7
RQ4 unflushed data after a successful print!  NO for '\n'-terminated rows (v2 PA
                                              test e9_flush_probe_newline_row_
                                              flush_ok_on_proven_broken_sink: the
                                              row is OBSERVED in the pipe before
                                              the close, the arm's single
                                              flush returns Ok, and the SAME-sink second
                                              print! panics — the sink was broken
                                              and the flush still performed no
                                              write)
RQ5 another writer leaves buffered data       the mechanism exists only for a NON-newline tail,
                                              and such a tail is carried to the fd by the NEXT
                                              row's own print! (v2 INTERLEAVE test: tail then
                                              row observed in the pipe before any flush ran;
                                              trailing flush Ok on the proven-broken sink);
                                              production writers are all '\n'-terminated
                                              (RQ1 + §14.1 single-writer phases)
RQ6 real independent error window             NO_WITH_SCOPE for the production shape
```

v2 probe (grade: L2_STD_PROBE-equivalent — bare-Stdout characterization; the
child arm carries no qianqian machinery, the harness is qianqian test code;
the committed tests ARE the receipt: the instrument is versioned in this
repository and re-executed by the required Linux CI lane, so the source is
NOT duplicated here; the parent test bodies own the pipe/gate/close order):

```text
PA+PA-2  ONE child, ONE real pipe sink: print!('\n'-terminated row) with NO
     intermediate flush; the parent OBSERVES the row in the pipe (premise
     oracle: the '\n' flushed it), closes the read end, releases the gate;
     the arm's single flush() ⇒ Ok(()) asserted on the child witness; a
     second print! on the SAME sink panics ("failed printing to stdout" +
     "Broken pipe") ⇒ the sink was broken when the flush returned Ok ⇒ that
     Ok can only mean NO write was attempted.
     e9_flush_probe_newline_row_flush_ok_on_proven_broken_sink
PB    control: a non-newline tail stays buffered after a successful print!
     (had it reached the fd, the only flush would find an empty buffer and
     return Ok), so the only flush() performs the write ⇒ Err(BrokenPipe)
     asserted — flush CAN fail standalone while print! succeeded; the
     INJECTED shape production does not have (RQ1).
     e9_flush_probe_no_newline_tail_flush_errs_broken_pipe
INTERLEAVE  a foreign non-newline tail is carried to the fd by the NEXT row's
     own print! (parent observes tail-then-row in the pipe before any flush
     ran); the trailing flush() ⇒ Ok on the same proven-broken sink (closing
     second print! panics).
     e9_flush_probe_foreign_tail_surfaces_in_next_row_print_not_flush
```

Mutant checks (local, archived `290-r2-flushprobe/mutant-*.log`): a
newline-terminated PB tail kills the PB test (the child's flush then finds
an empty buffer, so the Err-marker wait times out); dropping PA's second
`print!` kills the PA test (the brokenness witness never appears). Both
oracles bite.

v1 probe (SUPERSEDED — kept verbatim for the record; scratch std-only binary
`flushprobe`, no qianqian code; run logs `flushprobe-build/run/run2/run3.log`
— LOST with the archive, §14.8.8; every v1 value below was transcription-only):

```text
PA   print!('\n'-terminated row) into the pipe; row lands (ROW-DELIVERED sync);
     parent closes the read end; flush() ⇒ Ok(())  ⇒ flush performed NO write.
     [WITHDRAWN: the arm flushed explicitly before the gate — the Ok is the
     pre-flush's doing, not the newline drain's; non-discriminating.]
PA-2 CONTROL  with the read end closed, a second print! MUST panic — it did
     ("failed printing to stdout: Broken pipe (os error 32)", exit path 101) ⇒
     the PA sink was genuinely broken; PA's Ok means "no write", not "write ok".
     [Observation stands; its evidentiary ROLE (proving PA's own sink broken
     at flush time) is replaced by the v2 same-sink second print!.]
PB   print!(no-newline tail) stays buffered; flush() after the close ⇒
     Err(Os { code: 32, kind: BrokenPipe }) ⇒ flush CAN fail while print!
     succeeded — but only for a writer shape production does not have (RQ1).
     [Stands — this arm had no intermediate flush; re-executed as the v2 PB
     test.]
INTERLEAVE  foreign non-newline tail + clean row: the tail is flushed by the
     row's own print!; trailing flush() ⇒ Ok(()).
     [WITHDRAWN: same premature flush — the tail's carriage by the row's
     print! was NOT demonstrated (the explicit flush could have carried it);
     replaced by the v2 INTERLEAVE test.]
FILE sink: same PA shape ⇒ Ok(()), 12 bytes landed. EBADF: stdout closed at
     exec ⇒ print! did NOT panic and flush did NOT err (std handle_ebadf
     swallows).
     [File cell: corroborative only (never discriminated "no write"), receipt
     lost. EBADF: stands — swallow-vs-panic is independent of buffering.]
```

v1 probe source, inlined so the WITHDRAWN instrument remains inspectable
(scratch std-only binary `flushprobe`, no qianqian code; driver: pipes
stdin/stdout/stderr, waits for the child's `ROW-DELIVERED` stderr sync before
closing the read end, then writes the `go` gate line):

```rust
use std::io::{self, BufRead, Write};

// Parent writes "go" on stdin after it has arranged the sink state
// (closed the read end / opened the file). No timing premise.
fn gate() {
    let line = io::stdin().lock().lines().next();
    assert_eq!(line.expect("gate line").expect("read gate"), "go");
}

fn main() {
    let mode = std::env::args().nth(1).unwrap_or_default();
    match mode.as_str() {
        // PA: '\n'-terminated row lands; read end closes; flush is the :274 shape.
        "pa" => {
            print!("Q290-PA row\n");
            let _ = io::stdout().flush();
            eprintln!("Q290-PA ROW-DELIVERED");
            gate();
            let r = io::stdout().flush();
            eprintln!("Q290-PA FLUSH={r:?}");
        }
        // PB (INJECTED shape): no-newline tail stays buffered; flush writes it.
        "pb" => {
            print!("Q290-PB tail-no-newline");
            gate();
            let r = io::stdout().flush();
            eprintln!("Q290-PB FLUSH={r:?}");
        }
        // RQ5 mechanism: a foreign non-newline tail rides the NEXT row's print!.
        "interleave" => {
            print!("Q290-RQ5 foreign tail");
            print!("Q290-RQ5 row\n");
            let _ = io::stdout().flush();
            eprintln!("Q290-RQ5 ROW-DELIVERED");
            gate();
            let r = io::stdout().flush();
            eprintln!("Q290-RQ5 FLUSH={r:?}");
        }
        // PA control: after the close, a second print! MUST panic — proving the
        // PA sink was really broken and PA's Ok means "no write".
        "pa2" => {
            gate();
            let w = std::panic::catch_unwind(|| {
                print!("Q290-PA2 row\n");
                let _ = io::stdout().flush();
            });
            eprintln!("Q290-PA2 print-panic={}", w.is_err());
        }
        // EBADF posture: stdout closed at exec — std swallows, never panics.
        "ebadf" => {
            let w = std::panic::catch_unwind(|| {
                print!("Q290-EBADF row\n");
                let _ = io::stdout().flush();
            });
            eprintln!("Q290-EBADF print-panic={}", w.is_err());
            gate();
            let r = std::panic::catch_unwind(|| {
                let _ = io::stdout().flush();
            });
            eprintln!("Q290-EBADF flush-panic={}", r.is_err());
        }
        other => panic!("unknown mode {other:?}"),
    }
}
```

In-repo L1 (the committed counterpart): `flush_err_swallow_is_indistinguishable_from_success`
— a flush-shaped Err swallowed exactly like `:274` is indistinguishable from
success (no record, no Stop, seal clean); its in-test negative control proves
the harness distinguishes swallow from panic (a panicking closure yields
`false` + `Some(HostFailure::Panic)` + stop routed).

D-D consequence: Option A's precondition ("only if an independent flush Err is
reachable") is NOT met in the scoped environment — see decision-record §7.

#### 14.8.4 B4 — blocked output (EVIDENCED; lane: test-transport child + REAL pipe; one frozen-premise correction)

Constructed park: the child pre-fills its own unread stdout pipe (~64 KiB
default capacity — measured out-of-band in this round's LOST probe log, not
by committed code; on any capacity mismatch the reader never parks, the child
exits early, and the parent's parked-window poll fails loudly, so the
construction is self-verifying) before starting the transport, so the reader's
status writes park inside the admitted dispatch after a handful of rows,
regardless of scheduling. Observed: the child stayed alive through the parked
window (the transport's `seal` blocked on the parked operation's ack — no
"reader joined" claim possible), and after the parent drained the pipe the run
completed cleanly: exit 0, no `machine input failed`, at least one admitted
status row completed, the transport's own outcome row shipped, and
`Q290-E9-SEAL=None` asserted on the healthy stderr (blocking is not a
failure). Watchdog: bounded non-exit polling; a timeout reports BLOCKED (a
failed oracle), never PASS.

Evidence weighting (honest scheduling status): the parked window is
schedule-BIASED L2 evidence — on a pathologically slow machine the child could
reach neither park nor exit inside the window and the remaining asserts would
still pass; it cannot false-fire (an early exit fails the window loudly), but
it is not a happens-before proof. The load-bearing parked-ack ordering oracle
is the channel-synchronized L1
`seal_retains_admitted_postclosure_output_failure_and_rejects_fresh_work`
(seal waits the admitted operation's ack while a blocked output is in
flight), which fails on schedule removal.

**Frozen-premise correction (recorded, not normalized away).** The B4
choreography as first IMPLEMENTED this round ("a status flood after `stop`
parks the reader" — the d9d179c test construction, not the frozen §14.6-B4
text) is NOT the production semantics: the transport's `seal` closes stdin
admission right after
the stop settles (`machine_input.rs:86` `seal()` sets `admission_closed` before
waiting), so a post-stop flood is answered only until that closure and the
reader then exits on the closed admission long before a flood could fill the
64 KiB pipe — the first executed runs completed cleanly within seconds with
`SEAL=None` and were initially misread as a park failure. The park is real and
now constructed by pre-fill; the corrected oracle asserts the contract, not the
race: the parked row itself completed at release (≥ 1 `outcome:` row), the
transport's own outcome row shipped, and no host failure was recorded. This
sharpens the frozen invariant "admission closed ≠ reader exited": here the
reader exits ON closure, while parked-inside-admission is what delays the seal.

#### 14.8.5 B5 — late output failure vs seal boundary (cited oracles re-run green)

All five oracles cited in §14.6-B5 executed again this round, green (module
solo run and full-workspace run; both runs' logs were lost with the evidence
archive — §14.8.8; the oracles are committed and the post-fix rerun re-confirmed
them green):

```text
seal_retains_admitted_postclosure_output_failure_and_rejects_fresh_work   ok
admitted_output_panic_records_before_ack_even_during_seal                 ok
postseal_commands_reports_and_failures_are_inert                          ok
first_failure_storage_is_bounded_and_sealed_result_immutable              ok
preseal_failure_preserves_natural_completed_and_runtime_failed            ok
```

#### 14.8.6 B6 / E-8 per-site matrix

Lanes: `L1` = test-transport child (real spawn_reader / real report writer /
real pipes, in-process composition); `L2` = real production binary. Grades per
§14.5. All child probes are `#[cfg(unix)]`: the POSIX pipe choreography (incl.
the measured 64 KiB capacity) is the Linux-portable lane; the Windows
broken-sink lane stays NOT_RUN per §14.7 (interop proxy absorbs closure), so
the gate does not pretend to run it on windows-latest.

| SITE | LOCATION (6a9b051) | GATE | LANE | TRIGGER → SINK FAILURE | OBSERVED (exact asserted values) | CLASSIFICATION | GRADE |
|---|---|---|---|---|---|---|---|
| #1 help | entry.rs:70 | none | Linux real binary | `--help`, stdout read end closed pre-write | exit 101; stderr `failed printing to stdout` + `Broken pipe` | main-thread protocol write panic | L2_REAL_BINARY |
| #1 version | entry.rs:74 | none | Linux real binary | `--version`, same | exit 101; same panic text | same | L2_REAL_BINARY |
| #1 arg-error | entry.rs:102-103 | none | Linux real binary | `--bogus-flag`, stderr closed | exit 101; stdout empty; panic text UNOBSERVABLE (the default hook's own write targets the broken stderr) | main-thread protocol write panic (site attribution rides the healthy control: exit 2 + `error:` + `Usage:`) | L2_REAL_BINARY |
| #1 healthy | :70/:74/:102-103 | none | Linux real binary | no break | help starts `Qianqian` + `Usage:`; version `qianqian-headless `; arg error exit 2 `error:` on stderr | SITE ATTRIBUTION controls | L2_REAL_BINARY |
| #4 | entry.rs:529-535 | any(playback,test) | L1 child | stdout closed after arming marker, before rows | `UNWIND=1 DISPOSED=0 TERMINAL=None READER_SPAWNED=0` + panic text on healthy stderr; child exit 0 | panic skips wait+dispose+seal+reader spawn — disposal-skip WITNESSED at runtime | L1_TEST_TRANSPORT |
| #5 | machine_input reader stderr | any(playback,test) | (E-9 B2 lane) | see §14.8.2 | — | covered by B2, not a separate E-8 row | — |
| #6 machine | entry.rs:561-573 via the report writer (`real_report` in the probe lane: a byte-identical replica of `emit_machine_report` :582-587) | any(playback,test) | L1 child | stdout closed after arming (reader spawned on real stdin; parent `stop` settles) | broken: `UNWIND=1 DISPOSED=1 TERMINAL=Stopped` + `failed printing to stdout`/`Broken pipe` on the healthy stderr (panic skips only LATER reports); control: full row set + exit 0 | ledger #6 class at the final reports | L1_TEST_TRANSPORT |
| #7 | entry.rs:515 (dispose :516) | any(playback,test) | L1 child | stderr closed after arming marker | broken: `UNWIND=1 DISPOSED=0 TERMINAL=None`; control: exact `activation_failure_report(None)` line on stderr, disposal ran, exit 1 | disposal-skip WITNESSED (report panic precedes dispose) | L1_TEST_TRANSPORT |
| #2/#3 healthy | entry.rs:371-373/:378-414 | playback | Windows real binary (interop; x86_64-pc-windows-gnu build, §14.5) | healthy sink | `scanning …`, `source:`, `playing` rows captured (machine + reference runs) | healthy-site Windows rows (transcription-only survivors, §14.8.8) | L2_REAL_BINARY (healthy-sink interop only) |
| #7 Windows | entry.rs:515 | playback | Windows real binary (interop; x86_64-pc-windows-gnu build, §14.5) | `garbage` argv (NotEstablished) | rc=1 + activation-failure report on real Windows stderr | healthy-sink Windows row for #7 (transcription-only survivor, §14.8.8) | L2_REAL_BINARY (healthy-sink interop only) |
| #2/#3/#4/#6/#7 broken × Windows | — | playback | — | — | — | NOT_RUN (E8_WIN_BROKEN_PIPE_SITES, §14.7) | NOT_RUN |
| #2/#3/#7 broken × Linux real bin | — | playback | — | — | — | NOT_RUN (E8_LINUX_PLAYBACK_REAL_BIN, §14.7) | NOT_RUN |
| #3 fail-stop arm | entry.rs:378 | playback | — | — | — | NOT_RUN (E8_SITE3_FAILSTOP_ARM, §14.7) | NOT_RUN |

Attribution notes: `print!` panics INSIDE std (`stdio.rs:1166` in this
toolchain), so panic LOCATION text names std, not entry.rs — site attribution
rides the healthy controls' exact rows plus the per-site witness values
(UNWIND/DISPOSED/TERMINAL distinguish #4/#6/#7 unambiguously). Differential
against the frozen §14.6-B6 THEN, recorded here: B6's "std panic text naming
the site" clause is UNSATISFIABLE as frozen — the panic location is always
std-internal (D-γ) and the stderr-arm text is structurally unobservable (the
hook writes into the broken sink itself); the executed attribution formula is
healthy-control rows + per-site witnesses + single-stream elimination, which
is what every evidenced row above actually used. The #7 disposal-skip is
RUNTIME-WITNESSED (disposed marker still false after the unwind), upgrading
the source-inferred claim frozen in §14.4. The #6 REFERENCE arm (:392-397,
`playback` gate) has no Linux broken-sink row — NOT_RUN, registered §14.7.

#### 14.8.7 Discoveries register (this round)

```text
D-α  kernel §L.4 totality raise discharges a fiber's effects during ASSEMBLY:
     the first #7 fixture declared AudioOutputCapability but never installed
     it; the activation raise ran the unwind (kernel.rs §L.4 ⇒ run_unwind),
     pre-discharging the disposal marker before the transport ever ran
     (witnessed as DISPOSED=1 with no transport). Fixture corrected to the
     dependency-refusal shape (declaration ABSENT, assembly.rs Case::Dependency
     pattern). No production change; recorded as a fixture-authoring hazard
     for every future disposal-witness fixture.
D-β  B4 admission-closure semantics (§14.8.4): post-stop stdin floods are cut
     off by seal's admission closure; the park needs a pre-filled sink. The
     round's own oracle wording ("every admitted command" — d9d179c test
     comments, not the frozen §14.6-B4 text) stays correct; the construction
     changed.
D-γ  print! panic location is std's (stdio.rs:1166), never the call site —
     exit-101 attribution must ride healthy-sink row controls (§14.8.6 note).
```

#### 14.8.8 Executed verification of this round

EVIDENCE_ARCHIVE_LOST (disclosed per the round's evidence contract): the
session-local log archive (`/tmp/q290` — every full gate/test/probe log
referenced above, the Windows interop stdout/stderr captures, and the
flushprobe artifacts) was deleted from outside the working session AFTER
execution and AFTER the values below were transcribed into this record.
Surviving evidence, scoped honestly:

```text
(a) the committed test suite — every exact value in §14.8.1/§14.8.2/§14.8.4/
    §14.8.5/§14.8.6's L1 and Linux-real-binary rows is a literal assertion in
    it, rerunnable via the commands below;
(b) the v1 flushprobe SOURCE inlined in §14.8.3 (its recorded OUTPUT values —
    PA Ok / PA-2 panic / PB Err(BrokenPipe) / interleave Ok / file 12 bytes /
    EBADF no-panic — survive as this record's transcription only; the PA and
    interleave Ok values are WITHDRAWN as non-discriminating by the P1 probe
    repair, §14.8.3);
(c) the §14.5 environment facts and §14.8.6's Windows interop rows
    (rc=0/1, row texts, "scanning"/"source:" captures): transcription-only
    survivors — no committed oracle exercises the interop path; re-obtainable
    only by a fresh x86_64-pc-windows-gnu cross-build + interop session;
(d) two lost-log toolchain observations, so marked: the pipe capacity
    (64 KiB default) behind the B4 pre-fill, and the std panic location
    (stdio.rs:1166 at rustc 1.97.1/8bab26f4f); the blocked construction is
    self-verifying against capacity drift (§14.8.4);
(e) PR CI (GitHub-held logs): Linux lane + windows-latest lane both PASS at
    8c68a7f (runs 39030941998/39030942020) and at 8cdc2d2 (runs
    39038920923/39038920918); the review-fix delta runs on the PR;
(f) post-fix local reruns in the RECREATED session archive:
    machine_input 41/41 solo post-fix (run-mi-solo5-fixes.log),
    cargo test --workspace green (run-ws-2-postfix.log),
    full pre-push gate green + push (push3.log, 134.10s);
(g) v2 flush-probe local runs (owner P1 repair; recreated session archive
    `290-r2-flushprobe/`): e9_flush_probe 4/4 green
    (flushprobe-v2-first-run.log), machine_input module 45/45
    (module-machine_input-full.log), mutant kills PA/PB
    (mutant-PA-no-second-print.log / mutant-PB-newline-tail.log); CI receipts:
    the repair tip 1c62399's Linux lane PASS (Rust Regression run
    39046043339, all four e9_flush_probe tests green in the workspace suite)
    and the windows-latest lane PASS (Windows Compile Gate run 39046043276;
    cfg(unix) probes excluded there by declaration, §14.8.6).
```

The lost raw logs are not claimed as verification anywhere; each §14.8 claim
names the evidence class above it actually stands on.

```text
cargo test --workspace                                 green (§14.8.8-f; 0 warnings)
cargo test -p qianqian-headless --lib machine_input::  41/41 green solo (--test-threads=1)
cargo test -p qianqian-headless --lib machine_input::tests::e9   6/6 green solo
cargo test -p qianqian-headless --lib machine_input::tests::e8   8/8 green solo
flushprobe v1 (std-only scratch)                        SUPERSEDED (§14.8.3 P1 repair);
                                                        v1 PA/interleave Ok WITHDRAWN
e9_flush_probe v2 (committed tests)                     PA+PA-2 / PB / INTERLEAVE green;
                                                        mutants killed (§14.8.8-g)
Windows interop healthy-sink runs                      real-Windows rows captured (§14.8.6;
                                                       transcription-only survivors)
NOT executed here: native-Windows broken sinks (NOT_RUN §14.7), terminal-sink
probe (NOT_RUN §14.7). windows-latest CI lane: executed by PR CI, PASS
(§14.8.8-e) — with the pipe-choreography probes cfg(unix)-excluded there by
declaration (§14.8.6).
```

#### 14.8.9 Round closure gates

```text
E9_PRINT_PANIC        EVIDENCED (B1)
E9_STDERR_PANIC       EVIDENCED (B2)
E9_FLUSH_ERR          CLASSIFIED: INJECTED_WRITER_ONLY (B3, §14.8.3; v2
                      committed-probe basis after the P1 repair)
E9_BLOCKING           EVIDENCED, not-a-failure (B4)
HOST_FAILURE_CLASS    VERIFIED (host failure ≠ D11 Failed: B1/B2 assert D11=Stopped
                      with a separate host-failure record)
ACK_SEAL_ORDER        VERIFIED (B5 oracles + B4 parked-ack seal block)
D11_BOUNDARY          PRESERVED (no D11 change anywhere in this round)
D_D_OWNER_RULING      PENDING (decision-record §7; owner only)
```

## 15. Analyst registration correction (#321, 2026-10-28) — K0 Discharge liveness

**Status.** Executed E2E causal witness; defect confirmed; minimal fix in #322.

**Defect.** The observation analyst thread (`qianqian-observation`) is joined by the
decode worker's single exit funnel (`session.rs`: `observation.close(); analyst.join()`),
and session teardown joins that worker before the `PcmDecode` relation can return
`Discharged`. The analyst did not register a class-α pool, so the DG-4b hook chained a
panic on it to the previous hook. With the stderr pipe full and unread, the default
hook's direct raw fd 2 write blocked before the analyst's `catch_unwind`; the funnel
never completed `analyst.join()`, `worker.join()` never returned, and `dispose_root`
produced no `DisposeVerdict` (neither `Discharged` nor `Violated`). D11's terminal Fact
was already committed (`Completed`) and is not violated; the violated obligation is K0
teardown/Discharge liveness and the adopted DG-4b classification.

**Fix.** `AlphaRegistration::register()` as the first statement of the real analyst
thread closure (`observation.rs`); one registration created and dropped on that fresh
thread. No join-decoupling, detach, second lifecycle, Plugin or new noun. The join
result remains telemetry; the normal-path bound remains at most one pending analysis,
and registration does not bound an analyst hang for unrelated reasons.

**Executed evidence.** `evidence/analyst-join-321/`:

```text
baseline main                   Wedged: previous hook entered, not returned; analyst
                                in anon_pipe_write; decode in analyst.join; pipe
                                unread=65536; no DisposeVerdict; TERMINAL=Completed
fix HEAD                        Completed / Discharged; critical_panics=1; previous
                                hook never entered; output effect discharged
blocked sink, no panic          Discharged
broken stderr (EBADF) + panic   previous hook returns; Discharged
negative control (registration
moved inside catch_unwind)      Wedged
permanent unit oracle           observer thread registered; spawning/joining thread not
```

**Truth class.** Implementation record only. The panic injection is test-only and lives
in the evidence overlay (`evidence/analyst-join-321/overlay/`), never in the product
tree. Windows runtime: NOT_RUN (Linux host). Replay:
`evidence/analyst-join-321/replay.sh`.
