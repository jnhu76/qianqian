# Error-system implementation record — six-module refactor (2026-10-16)

```text
STATUS:  IMPLEMENTATION RECORD / EVIDENCE INDEX — NOT AUTHORITY
BASE_SHA 79c6d7832ce445115276f9a505901804cfa5415b (PR #284 merged; reviewed head 758a114)
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
5. **DG-4b critical-thread registry** — a static registry of thread names (the
   reader today), consulted on every panic; unnamed/unregistered threads chain
   (fail-open). The hook's critical path is atomics-only and never renders: the
   counter and the one-shot pool record are read by non-critical threads.
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

Negative controls executed this round: the DG-1 Relaxed-recycle pool control
(Miri RED, then reverted), the DG-4b isolation-removal control (RED, reverted),
the cone-oracle compile sight control (RED, reverted), and the boundary gate's
own negative-control suite.

## 5. NOT_RUN (declared, with what each would settle)

| Item | Why not run here | What it would settle |
|---|---|---|
| O-CONE-NECESSITY / O-CONE-ORDER runtime runs | `wasapi.rs` is `#[cfg(windows)]`; this host is Linux. The module is written and compile-checked for `x86_64-pc-windows-gnu` | the cone's machine-progress premise (settle with no external stop) and the stop-before-complete order |
| Cone negative controls (delete `stop`, swap `stop`/`complete`) | same; the module documents both mutation procedures and their expected reds | that necessity and order are each load-bearing |
| E-7 tier 1 (writer-swap per site × gate on/off, broken/blocked sink) | needs a Windows host with a real endpoint for #8/#9, and a volume-apply fault seam for #10 (the design names that seam as the one small addition) | that a diagnostic-write failure cannot change terminal class/exit path/wakeup |
| av_log discard-path runtime observation under the pinned FFmpeg | needs the runtime with FFmpeg logging driven | that the default discard behaviour is what C's code says (unchanged by this round) |
| Kani | toolchain not available on this host (declared CI formal-lane tool) | bounded proof of the pool protocol under Kani's model |
| Windows device/audible evidence for the new carrier B reason | no Windows host | that a real device abort's reason reaches the presentation surface |

## 6. Open owner rulings (presented, not decided here)

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

**DG-4b registry membership** — the reader is the only registered critical thread.
Adding render/decode names is a one-line registry change *if* their default prints
ever need suppression; today they do not.

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
DG-1 (facility, isolation, shutdown)     qianqian-diagnostics src/tests + Loom + Miri + the
                                         App install/shutdown wiring in apps/headless/src/entry.rs
DG-2 (carrier B)                         ports.rs surface (+ audio-api snapshot), wasapi abort
                                         sites, e1_worker_tests dg2_* pair, completion.rs mirror
DG-3 / DG-4a (retention, attribution)    decode-songcore unit tests + e1_abi_tests typed-fault test
DG-4b (panic hook)                       machine_input.rs registry + tests.rs subprocess witness
DG-4c (av_log)                           decode-songcore av_log_bridge tests (6)
§7.1/§7.2 real cone                      qianqian-output-wasapi cone_oracle_tests (Windows-only)
§7.4 E1-ABI                              decode-songcore e1_abi_tests (8)
§7.5 E1-WORKER                           qianqian-playback e1_worker_tests (12)
§7.6 F5 extension                        gate verified here; model unchanged (§6 D-C)
```
