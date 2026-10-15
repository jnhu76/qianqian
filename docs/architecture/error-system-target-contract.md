# Error-system target contracts (#231, #280)

> **Truth class: DESIGN_CANDIDATE / NOT AUTHORITY / NOT IMPLEMENTED.**
> Every goal below is a **candidate obligation**. Nothing here changes D11, D14.*, U2,
> the SongCore ABI, K0 semantics, or any accepted document; merging or reviewing this file
> authorizes no implementation and adopts no policy. Where a goal would change frozen
> behavior or accepted authority, it is marked `AUTHORITY-AMENDMENT-REQUIRED` and its
> concrete candidate text lives in
> [error-system-migration.md](error-system-migration.md) §3.
> **Evidence basis:** the frozen facts in
> [error-system-reconciliation.md](error-system-reconciliation.md) (R1) at
> `5e60da4c3bd5f0fa93b4215670f4e898e2ac844e`; scans #281/#282; #231 audit + erratum.

## 0. How to read a goal

Each goal record has: **Basis** (authority/code anchor), **Current**, **Target**,
**Change class**, **Owners**, **Counterexample** (what observable would falsify the goal
if violated), **Acceptance** (how a future implementation proves it), **Open** (unsettled
sub-decision). Change classes:

```text
PRESERVE          current behavior is already the contract; protect it explicitly
LOCAL-REP         representation change inside one owner; no public/semantic surface moves
NEW-OBLIGATION    adds an obligation without touching frozen semantics
AUTH-AMEND        requires an authority amendment before any implementation
OPEN-DECISION     owner must choose; options listed, none implemented by this round
```

Priorities follow the R0 ordering: representation only where a consumer + insufficiency of
local reporting is shown; diagnostics never become recovery permission; no new Plugin,
EventBus, Error/Recovery/Logging Plugin, K0 primitive, or second lifecycle may be earned by
this document (D13 razor; any such identity needs its own D13 review with the review
prompt in AGENTS.md).

---

## M1 — Error representation

### 1.1 The minimal layered model (candidate)

```text
L0  SongCore C ABI (FROZEN, v1)
    song_status 0/1/100-113 + song_error{message≤255B, native_code}
    Borrowed diagnostic: valid until the next same-handle SongCore call (songcore.h:99-102)
L1  Decode adapter INTERNAL (qianqian-decode-songcore)          ← the retention layer
    typed status + native_code + message, retained locally; not public
L2  Public decode/output ports (qianqian-audio-api)
    DecodeOpenError/DecodeError/OutputError stay {message: String} by default;
    a diagnostic-only structured field is addable ONLY via the T1-3 gate
L3  Session internal (qianqian-playback)
    WorkerFailureOrigin{Decode,Processing} (typed, D14.11) + optional fault snapshot
L4  Public observation (frozen truth classes, D14.2)
    EpisodeTerminalOutcome{Completed,Stopped,Failed} + failure_diagnostic(Option<String>)
    + activation_error + last_processing_refusal — no new variants, ever
L5  App / process
    OpenOutcome 4-class (D14.6), exit codes {0,1,2}, machine report rows (contract)
```

The layer rule: **each layer keeps only the distinctions its next owner can act on or is
obligated to surface; everything else stays local as diagnostic provenance.** Upward
promotion of any distinction requires the T1-3 gate; downward collapse is free for
presentation but never for control evidence.

| Goal | Record |
|---|---|
| **T1-1** Adapter-internal fault retention | **Basis:** #231 R6 candidate B; L1/L2 (typed status + `native_code` already cross the FFI, `native_code` production reads = 0). **Current:** `_ => Err(DecodeError{message})` drops status+native (lib.rs:288-291). **Target:** adapter-internal `DecodeFault{status, native_code, message}` retained at the error site; message-only remains the public face. **Change:** LOCAL-REP. **Owners:** qianqian-decode-songcore. **Counterexample:** any public type gains a field, or a session control branch reads the fault class. **Acceptance:** adapter unit test pins status/native capture per synthesized fault; workspace-wide public API diff is empty. **Open:** none (pure additive; still CANDIDATE until owner adoption). |
| **T1-2** Probe/open diagnostic symmetry | **Basis:** L3/L15 (probe discards an available `last_error()`; `song_open` has none by contract, songcore.h:216-219). **Current:** probe formats decimal status only (lib.rs:188-200). **Target:** probe reads `last_error()` (message+native) before closing the handle and embeds the owned copy in `DecodeOpenError.message`; `song_open`'s no-diagnostic contract is preserved untouched. **Change:** LOCAL-REP (message text only). **Owners:** decode adapter. **Counterexample:** any code path treats "status 104" text as machine input. **Acceptance:** binding-crate test asserts the diagnostic text accompanies a probe failure on a garbage fixture. **Open:** OPEN-DECISION D-2 (one-line change; awaiting owner). |
| **T1-3** Public-field admission gate (carries disposition-contract §5 forward, verifiable form) | **Basis:** disposition-contract §5.1-5.4; D14.2. **Target:** a public error/observation field may be added only with (a) a named consumer **obligation** — which surface or programmatic consumer, which diagnostic/control duty it discharges; the DUTY is the criterion, not the syntax the consumer uses (rendering an error reason may legitimately run through `format!`), (b) proof local reporting is insufficient for that duty, (c) smallest backend-neutral shape, (d) for diagnostic-only fields, a demonstrated control-inertness witness (a test that flips the field and observes zero control divergence). **Change:** NEW-OBLIGATION (process, not code). **Counterexample:** a field no admitted consumer obligation uses — whose readers, if any, are incidental (a stray `format!`, a test) rather than the duty the field was minted to serve. |
| **T1-4** Stable code ≠ raw code passthrough | **Basis:** CE2 (107 never produced in the decode loop; 108 lumps media, host-I/O-adjacent and decoder-internal). **Target:** if/when any stable product-facing class exists, it is minted at the layer that can prove it, never forwarded from `song_status` verbatim; SongCore vocabulary must not leak into product enums (binding-architecture boundary). **Change:** NEW-OBLIGATION. |
| **T1-5** No mega-enum, no unbounded cause chains | **Basis:** disposition-contract §1 (four dimensions, non-exclusive); current six-carrier inventory (A REPORT §一). **Target:** representation stays per-operation results + typed session origin + presentation strings; a cause snapshot is bounded (≤ N bytes, truncated, owned, copied inside the same-handle window); no universal `QianqianError`, no `source: Box<dyn Error>` chains across owners, no `retryable: bool`. **Change:** PRESERVE. |
| **T1-6** Specific-boundary retention notes | **Target statements per boundary (each LOCAL-REP at most):** Host `io::Error`: open-path Display already carries `(os error N)` (L15 asymmetry acceptable). **Mid-read OS detail does not cross the callback return value** (callback returns `-1`; the errno is not in the return) — but it IS retainable through the adapter-owned `userdata` side channel (**R1.1 correction of the prior "not retainable" verdict**): `song_io.userdata` is created and owned by the binding (lib.rs:163-168, `Box::into_raw(File)`), so a callback may record the `io::Error` into an adapter-private context before returning `-1`, and the adapter reads that context after the SongCore call returns — no ABI or callback-shape change. This candidate must still discharge: per-operation association (which binding-level call the failure belongs to), stale-error clearing at operation start, multiple internal callbacks per call (FFmpeg may issue several — first/last-wins must be defined), and context ownership/lifetime with the endpoint (`Box<File>` becomes `Box<IoContext>`). Recorded as a compared candidate with obligations (migration §1.11), not as an impossibility. SongCore status/native: T1-1. probe/open: T1-2. read/seek: seek keeps `classify_seek_status` as the only typed-status semantic consumer; read retains internally per T1-1. DSP: typed origin already correct (D14.11). Output HRESULT: message text remains the carrier; T2-5 gives the verdict/diagnostic split delivery. Worker panic: payload downcast already captures `&str`/`String`; keep the non-string fallback literal. Activation: message path via `activation_failed` is correct; **K0 stays message-free (§J.3)** — no `FiberDiagnostic` message field without a K0 authority review. Teardown: verdict-only at K0; disposal warnings render at App. Host output: machine rows keep protocol role (T5-7). |

---

## M2 — Propagation (three responsibility classes)

The single load-bearing taxonomy of this module:

```text
Class A  reliable operation result    — sync return / FFI status / command ack / OpenOutcome
Class B  reliable execution/lifecycle evidence — completion publications, drain verdict,
         worker_exited, gate events, OpenVerdict, Discharge verdict
Class C  discardable diagnostics      — logs, TUI rows, telemetry offer, buffered warnings
```

Obligation: **every failure must reach its owner through A or B. No A/B delivery may
depend on any C mechanism.** C may be dropped, delayed, truncated, or lost without
affecting A/B delivery or settlement.

| Goal | Record |
|---|---|
| **T2-1** Delivery-class labeling | **Target:** each propagation edge in the unified ledger (U1-U40) carries an explicit class tag in design docs and review checklists; new edges declare their class before implementation. **Change:** NEW-OBLIGATION (documentation discipline). **Counterexample:** a design where a Failed terminal's publication waits on a log queue/sink. |
| **T2-2** Settlement ownership untouched | **Basis:** D11/D14.3; first-wins commit (completion.rs:1238-1249); settlement on the session-owned publication path. **Target:** PRESERVE exactly — evidence publication, resolve and terminal commit inside ONE completion-lock hold, with waiters notified AFTER the lock is released (the source drops the guard before `notify_all()`, completion.rs:1246-1247; R1.3 wording fix — the earlier "notify in one lock hold" was wrong); no resolver thread; no observer-created terminals; read/wait stay pure. **Counterexample:** any path where `observe_snapshot`/`wait_terminal` could commit, or where a C-class consumer must run before `drain.complete` returns decisively. |
| **T2-3** Borrowed diagnostics never cross an async boundary | **Basis:** songcore.h:99-102; current adapter copies immediately (lib.rs:213-226) — already conforming. **Target:** PRESERVE; codify as a review checklist item ("copy before the next same-handle call; owned data only into async hands; a failed copy never masks the operation result"). |
| **T2-4** Per-edge delivery contracts | **Target table (each row: producer → receiver, delivery point, ordering, duplicate policy, late-result policy, receiver-retirement, wakeup duty):** sync return (stack; n/a; n/a; n/a; caller alive by construction); FFI (status+diagnostic at call return; diagnostic valid until next same-handle call; copies owned from there); decode worker → completion (`publish_worker_failure`/`worker_exited`; single lock hold; first-wins; late evidence inert; receiver never retires before publish — funnel order :911-930); render leg → drain verdict (`drain.complete` first-wins; observer synchronous outside verdict lock; observer panic loses only the mirror — U18); render/output abort → data plane (`render_input.stop` before `drain.complete`; ordering is load-bearing, N-01 shows the failure mode); open verdict (slot + 10 s cap; late verdict dies with the thread — see T2-5); teardown (LIFO relations; `release_pause_gate` before join; `Discharged`/`Violated` verdict); host input (first-failure get_or_insert; seal waits ack, unbounded — documented); process exit codes (final render, compressed — L9). |
| **T2-5** Output verdict / reason split delivery | **Basis:** L7/F3 (reason lost at the Output boundary: `LoopOutcome::Aborted{message}` exists only as gated stderr + thread-local). **Target:** the Output mechanism delivers a **bounded owned reason snapshot alongside the verdict** into the session's diagnostic surface (Class B for delivery reliability, Class C for content — the verdict alone remains the only settlement input; the snapshot never changes classification). **Delivery path (R1.1, drawn end-to-end):** the render thread is created by `qianqian-output-wasapi`, which depends only on `qianqian-audio-api` + `qianqian-composition` — a playback-typed cell cannot reach it directly. Allocation (session, before `open_stream`) → handoff (**crosses `qianqian-audio-api` in every carrier**: (a') a `RenderRequest` field carrying an audio-api-typed cell — the exact pattern `OutputLevel` (D14.9) and `PositionEvidence` (D14.8) already use, ports.rs:962-985; the session constructs the request at session.rs:410 — or (b) a parameter on the `drain.complete`-adjacent delivery API, `DrainSignal` likewise audio-api-typed) → write (render thread at abort stores a bounded owned copy) → read (session publication / internal diagnostic surface; exposure on the public observation plane is a separate T1-3/D-9 question) → retirement (episode teardown drops the session's cell; the mechanism held only a clone). `DrainVerdict` stays two-valued; D11 inputs unchanged. **Change:** NEW-OBLIGATION; the handoff crosses the backend-neutral port surface in BOTH carriers → the PBK-003 interpretation note (migration §3.4) applies to T2-5 as a whole. **Owners:** output-wasapi + playback + audio-api. **Counterexample:** resolve() or any App branch reading the snapshot; snapshot absence changing any terminal. **Acceptance:** stop_seam-style test asserting verdict-only settlement with the snapshot present AND with it absent (identical outcome class). **Open:** OPEN-DECISION D-10 (carrier (a') vs (b); observation-surface exposure). |
| **T2-6** Late/absent delivery has a declared fate | **Target:** every B-class publication names its behavior when the receiver is gone or late (open-handshake late verdict: dies with the mechanism thread, activation reports the timeout honestly — current; late gate events: current-wins attribution; post-seal host failures: silence — current). No new retention buffers "just in case". **Change:** PRESERVE + document. |

---

## M3 — Handling and escalation (per-operation disposition table)

No recovery capability is implemented or adopted by this round. Every row states the
disposition that EXISTS; where none exists, "no recovery" is the explicit disposition.
`CURRENT` rows are authority/code-fixed; `CANDIDATE` rows are new obligations or
authority-dependent choices.

| Operation | Success promise | State change | Continue condition | Local handling | Escalation | Recovery owner | Accepted-work closure | Status |
|---|---|---|---|---|---|---|---|---|
| open/probe | real file probe/open or typed refuse | none on refuse (old world intact) | probe≠episode (D14.6) | App refuses Open | refuse → OpenOutcome::Refused | none | n/a | CURRENT |
| read (clean) | frames or Eof | edge admission | always | EAGAIN consumed in C loop | — | — | delivered frames count | CURRENT |
| read (partial success) | N frames SONG_OK; fatal next read | latch in C handle | continue until surfaced/stopped/seek-replaced | none (ABI contract) | surfaced at next read → D11 Failed | none | N frames delivered, no rollback (disposition-contract §3) | CURRENT; CANDIDATE witness T3-1 |
| read (fatal) | typed error, 0 frames | edge.fail, worker_failure first-wins | no old-cursor resume | none | D11 Failed{decode} | none (#277 untouched) | buffered PCM not withdrawn: edge-buffered frames abandoned (render-side terminal-before-data); pulled-and-submitted frames MAY be heard but audibility is never asserted — `ReleaseBuffer` can fail after the pull and a stop/device failure can precede consumption (reconciliation §2.3.1 evidence-level vocabulary, R1.2) | CURRENT |
| seek RefusedUnchanged | old continuation proven intact | remainder finished exactly | continue == no-seek (bit-exact) | worker completes remainder | none | none | remainder zero-loss | CURRENT (production-dead class, contract-live) |
| seek Applied | new cursor + landing evidence | staging discard + history invalidate + purge | commit/rebase per D14.5 | worker protocol | none | none | pre-cut PCM purged; rebase routed | CURRENT; CANDIDATE: superseded-fatal witness T3-1 |
| seek MutatedThenFailed | none (old continuation unprovable) | release-without-commit + edge.fail | never resume | none | D11 Failed{decode: seek failed} | none | none | CURRENT (SEEK_UNSUPPORTED reclass = AUTH-AMEND, migration §3) |
| DSP live refuse | old config continues bit-exact | `last_refusal` latched | continue | control records refusal | none | none | n/a | CURRENT (D14.11/§7.3) |
| DSP accepted failure | none | processing_failed | none | none | D11 Failed{processing} | none | partial block discarded | CURRENT (bypass = NOT_EARNED) |
| volume apply failure | last applied level holds | attempted-bit latch | continue | mechanism retains level | device-invalidated → abort path | none | n/a | CURRENT (D14.9); diagnostic write hazard = N-01 class, T5-4 |
| render steady failure | none | edge stop + verdict | none | none | D11 Failed{device} (or Stopped with intent) | none | device-side frames as rendered | CURRENT; reason delivery = T2-5 |
| drain | padding==0 → Drained | verdict | 5 s cap (local tuning) | none | deadline → Aborted → Failed{device} | none | handed-off accounting | CURRENT; cap semantics = OPEN-DECISION D-4 (AI-7 space) |
| stop | Stopped, or Failed{decode} if a pending fatal surfaces first (reconciliation §2.3.1 commit race) — first-wins governs terminal COMMIT, not command arrival | intent + gate release + edge stop | — | inert if late | — | — | — | CURRENT |
| replacement | old stopped → fresh episode | whole-composition swap | probe-first (D14.6) | App owns | fail-stop on teardown violation | none | old terminal independent | CURRENT |
| activation | Established or clean failure | unwind LIFO | retry = new Open | none | ActivationFailedClean / FailStop | none | attempted root disposed | CURRENT |
| teardown | Discharged or Violated | LIFO latch | — | none (inverses are total by contract) | §G.6 latch, no exit | none | partial discharge = violation | CURRENT; panic-in-inverse = OPEN-DECISION D-7 (K0 review) |
| host input failure | infrastructure report | first_failure + stop | — | none | exit 1, never a Fact | none | n/a | CURRENT |
| process exit | code {0,1,2} | — | — | — | — | — | reports after dispose/seal | CURRENT; refinement = #231 open question |

| Goal | Record |
|---|---|
| **T3-1** Superseded-pending-fatal witness (CANDIDATE — **currently unobservable, mechanism OPEN**) | **Basis:** reconciliation §2 (seek-Applied erases a latched fatal with no trace). **R1.1 correction of the original premise:** the latched fatal is NOT "readable pre-seek via the T1-1 snapshot". In the partial-success state the binding saw only `SONG_OK` + frames (lib.rs:286 — the success arm never reads any error state); `ok(h)` has already cleared the one-shot diagnostic that `song_last_error` exposes (C:133-138, C:1155-1156); `fatal_error` itself is internal handle state with **no ABI query**; and a successful `song_seek` clears it before returning (C:1196). The information genuinely does not cross the ABI boundary, so adapter-internal retention alone cannot implement this witness. **Target (if wanted at all):** requires a NEW information source first — an ABI amendment (e.g. a pending-error query) or a SongCore-internal change; the witness then rides the same Class-C diagnostic path as T2-5 — never into classification. **Change:** OPEN-DECISION with a mechanism gap; NOT a LOCAL-REP. **Counterexample:** any control branch or terminal change keyed on the note. **Open:** OPEN-DECISION D-5 (is the witness wanted at all? an ABI amendment for a diagnostic note is a high price; doing nothing remains the cheap legal choice). |
| **T3-2** Explicit non-recovery dispositions | **Target:** the "Recovery owner: none" cells above are the adopted-by-default posture; any future recovery candidate must arrive with the disposition-contract §3 NOT_EARNED proof obligations, unchanged. **Change:** PRESERVE. |

---

## M4 — Lifecycle errors

The five independent facts (reconciliation §3.5) and their code-enforced relations are the baseline.
Target obligations:

| Goal | Record |
|---|---|
| **T4-1** Five-fact independence is never collapsed | **Target:** design/review language and any new code must keep terminal commit / thread exit / resource release / discharge verdict / join death-cause as separately evidenced facts; no timeout return, projection, or log line may stand in for any of them ("a timeout return must never impersonate thread-exited or resource-released"). **Change:** NEW-OBLIGATION. **Counterexample:** any API returning "stopped" after a deadline without `worker_gone`/join evidence. |
| **T4-2** Original failure and cleanup failure are both preserved | **Basis:** §G.6 partial discharge is a violation, never a noted success; but a panicking inverse today crashes instead of latching (reconciliation §3.4). **Target:** when cleanup fails after a primary failure, BOTH survive: the primary (already committed or queued as Class-B evidence) and the cleanup verdict (Discharge/Violated or — candidate — a caught-panic latch). They are rendered together at the App (disposal warnings already exist). **Change:** PRESERVE for the explicit-Violated path; OPEN-DECISION D-7 for panic→latch. |
| **T4-3** Continue independent cleanups after one fails | **Basis:** kernel run_unwind stops at the first Violated and holds the rest as tombstones (§G.6: partial discharge is a violation — deliberate). **Target:** PRESERVE K0 semantics; the obligation to *attempt* remaining independent cleanups belongs to each owner's own effect design (e.g. the worker relation stops the edge before joining so the render relation can still release). No K0 change proposed. **Change:** PRESERVE. |
| **T4-4** Logging is never resource-obligation fulfillment | **Target:** no diagnostic record, warning, or export counts as releasing/joining/stopping anything; the disposal-warning path (machine.rs) renders verdicts, it does not produce them. **Change:** NEW-OBLIGATION. |
| **T4-5** Lifecycle-stage coverage map | **Target:** the nine stages (not-started / partial-start / activation-failure / runtime-failure / failure-cancel race / replacement / normal stop / cleanup-failure / join-abnormal / process shutdown) each keep their current routed disposition (R1 + scans) as the regression baseline; any rework re-proves them against the same DAGs. **Change:** PRESERVE. |

---

## M5 — Logging (candidate design; nothing authorized for implementation)

### 5.1 Shape (compared, then recommended)

| Mechanism | RT-path cost | Failure isolation | Shutdown | Dependency | Verdict |
|---|---|---|---|---|---|
| direct `eprintln!` (today) | blocking + panic-on-broken-pipe on the publishing thread | none (N-01/D2 hazards) | n/a | std | baseline to replace on critical cone only |
| synchronous formatter + locked stderr writer | lock wait + write on producer thread | write errors containable (no panic), blocking remains | trivial | std | usable for non-RT surfaces only |
| **bounded lossy queue + one drainer thread (host-owned)** | `try_push` of an owned record; the producer cost is a property of the CONCRETE queue type + version and must be stated along the FULL send path (construction, enqueue, contention retry, wakeup, destruction) — R1.2 correction: at the pinned toolchain, std documents the bounded `sync_channel` buffer as pre-allocated and fixed-size (rustc 1.97.1, std/src/sync/mpsc.rs doc), so no per-record-allocation claim is made for the bounded flavor; a pre-allocated fixed buffer still does NOT by itself bound every producer's execution steps (contention/retry/wakeup paths must be compared, not assumed) | producer never touches the sink; sink failure = drop counter++ | drain with a **bounded deadline, then abandon** — a drainer stuck inside a blocking sink call ignores every deadline, so bounded JOIN is achievable only through exporter admission (cancellable/timeout-capable sinks only) or by abandoning the thread at shutdown; the admission/exit mechanism is OPEN, not a property the queue shape provides | std-only achievable for the queue; exporter policy needs care | **RECOMMENDED core** |
| `tracing` + non-blocking layer | comparable once configured | good | subscriber lifecycle | extern crate + feature decisions | acceptable alternative; not required |
| crossbeam-style bounded channel + worker | differs from std along several send-path dimensions (both directions possible per section) | equivalent | equivalent | extern crate | R1.2: **no advantage verdict is asserted** — the R1.1 "allocation-free advantage" claim conflated std's unbounded and bounded flavors and is retracted; choose by measuring the pinned types on the actual producer set, not by name |

**Recommendation:** a minimal host-owned facility — component-scoped facade, owned
structured record, lossy bounded queue, one drainer thread owned by the App/host (not a
Plugin, not a K0 capability, not an EventBus: it earns no composition identity under D13;
sinks may be registered as ordinary owned effects of the fiber that owns them). Formatting
happens on the drainer, never on producers. Per-component level filtering at enqueue
(cheap) and at the sink (authoritative).

### 5.2 Goal records

| Goal | Record |
|---|---|
| **T5-1** Record shape | **Target:** owned record = {component: &'static str, level, op/stage tag, bounded cause snapshot (typed code + native code + truncated message ≤ N bytes), optional episode-local context if already at hand, monotonic timestamp}. No Episode ID invented (disposition-contract §6). Size/truncation enforced at construction. **Change:** CANDIDATE. |
| **T5-2** RT/firewall entry | **Target:** RT and failure-cone producers use only non-blocking `try_push` with pre-sized storage — **the concrete queue type must be shown to match that promise along the full send path** (construction, enqueue, contention retry, wakeup, destruction), pinned to type + version; the `try_*` name, "bounded", or "pre-allocated buffer" alone guarantee nothing (R1.2: a pre-allocated fixed buffer still does not bound every producer's execution steps); no formatting on producers; a full queue increments a drop counter and moves on. Logging enabled/disabled/dropped/full **never** changes any operation result or terminal. **Counterexample:** any divergence between run-with-logging and run-without beyond the log bytes. |
| **T5-3** No exporter under settlement/resources | **Target:** no sink call while holding the completion lock, the edge lock, or any resource an owner needs for terminal progress; exporter failures increment counters and are themselves logged best-effort. **Basis:** disposition-contract §6 (carried forward, now with the reconciliation §4 ledger as the baseline map). |
| **T5-4** Producer write-failure isolation (partial remediation, honestly scoped) | **Target:** diagnostic writes from the failure cone must not panic the producer — a write-failing sink is diagnostic loss. **Scope (R1.1, unified table in migration §1.5):** the cone sites are ledger rows #8 (post-catch abort line), #9 (opened line — its panic unwinds into catch #1 AFTER the `Opened` verdict published, cascading to N-01 or a post-activation `Failed{"device"}` misattribution) and #10 (volume); all three swap to a non-panicking writer. #11 (panic hook + chain-to-previous, which restores the default hook's synchronous stderr for every other thread) and #5 (reader protocol writes) are separate classes (T5-8; migration §1.7) and are NOT fixed by a writer swap. **A non-panicking writer is a PARTIAL remediation: it removes the panic/skip escalations; full-pipe BLOCKING remains on every synchronous site until the T5 framework lands (T5-2/T5-3) — the firewall as a whole is not satisfied by it.** This supersedes today's `eprintln!`-panics semantics on exactly those sites; the narrow corrective is independent of any framework adoption — OPEN-DECISION D-3. **AUTH-AMEND:** none (no frozen semantics touched; D14.9 grading unaffected). |
| **T5-5** Lifecycle and shutdown race | **Target:** the facility is host-owned (installed before episodes, drained at host exit); producers never block on the drainer; at shutdown the host drains with a bounded deadline and reports dropped-record counts; no producer waits for a consumer. **Bounded JOIN of the drainer is NOT a property of this shape (R1.1):** a drainer inside a blocking sink call ignores deadlines — the queue's boundedness and the producers' `try_push` discipline bound the producer side only. Bounded join requires exporter admission (cancellable/timeout-capable sinks only), sink isolation (platform-dependent spawn/kill), or abandoning the thread at process exit; this mechanism is OPEN and must be settled before any "bounded shutdown" acceptance claim is made. **Counterexample:** exit hanging on a log flush after the terminal Fact is committed — including via a `join` that outlives its own drain deadline. |
| **T5-6** Sensitive-path policy | **Target:** default diagnostics avoid unnecessary private filenames/absolute paths (disposition-contract §6 carried forward); richer export is a separate product/privacy decision. Current F-17 (paths in decode messages) is recorded as the baseline to improve, not a defect to silently change (protocol-visible texts are contract). |
| **T5-7** Machine stdout keeps its protocol role | **Target:** protocol rows (`source:`, `playing …`, outcome rows, status responses) remain direct writes with their current contract; they are NOT rerouted through the logger; their failure is a host-operation failure (already classified in reconciliation §4). UI feedback and reliable diagnostics must not depend on best-effort log consumption. **Change:** PRESERVE. |
| **T5-8** Panic hook as a process-global facility | **Basis:** machine_input.rs:192-207 (global swap, chain-to-previous, never restored, machine-transport-only installer). **Target:** any future hook design names ONE process-level installer (the host), documents non-restoration, treats reentrancy (a panicking hook) as terminal, and is never per-episode replaceable. **Chain-to-previous is a CONDITION, not an unconditional duty (R1.2):** invoking an uncontrolled previous hook re-introduces exactly the synchronous-stderr hazard this module isolates, so the target constraints must be co-satisfiable — a future design must state WHICH threads/contexts chain to the previous hook and what the previous hook must satisfy (e.g. chain only outside the failure cone; or replace-with-record and invoke previous only from a non-critical context; or a documented acceptance of the residual with the affected threads enumerated). The current machine-transport hook keeps its chain (its threads are the installer's own); nothing here changes it this round. **The hook's stderr write is NOT cost-bounded by any platform guarantee:** it is an ungated synchronous sink running on whatever thread panics, before any catch — a full pipe blocks the panic translation itself (ledger #11, B F7). The hook therefore remains inside the logging firewall's hazard inventory; its isolation mechanism stays OPEN, but no future design may simultaneously claim critical-path isolation and unconditional chaining. **Change:** NEW-OBLIGATION on any future rework; PRESERVE current single-installer fact. |
| **T5-9** FFmpeg av_log callback | **Target:** stays a C-side process-global with per-`song_open` reinstall and the discard-by-default policy; if product logging later wants FFmpeg chatter, it goes through a dedicated owned bridge copy — never by restoring FFmpeg's default logger in production paths. **Change:** PRESERVE + forward constraint. |

---

## M6 — Migration and acceptance

Delivered in [error-system-migration.md](error-system-migration.md) (per-boundary
rows, authority-amendment correspondence, acceptance-by-nature designs).

---

## DeepSeek-harness reference mapping

Reference: `deepseek-ai/deepseek-harness` @ `571902028a75c64c1d767f4ae5fd099485f0ddf0`
(files: cordis `fiber.ts`/`events.ts`/`logger.ts`; `llm/error.ts`; `llm/adapter-failure.ts`;
`core/tools/src/index.ts`; `core/agent-loop/src/agent.ts`; `llm-retry/src/index.ts`).
Verdicts use the required three-way label. Nothing below justifies a new Plugin: where the
reference uses a plugin/service, Qianqian's analog is an owned resource/effect or ordinary
library code unless a D13 review independently earns identity.

| # | Reference practice | Adopt / adjust / not portable | Reason and Qianqian mapping |
|---|---|---|---|
| R1 | `code` = stable machine identity, `message` = human-only, `cause` = standard chain; "route on code, never parse message"; cause rendering quarantined as diagnostic-only | **Adopt principle as-is** | Matches D14.2/CE6 exactly. Rust: typed enums / `&'static str` at the ABI edge; `#[source]` chains are owner-bounded. Non-portable piece: their regex message-classifiers exist because third-party providers lack stable codes — SongCore is in-repo, so classification belongs at the ABI (T1-4), not in text. |
| R2 | Exception⇄result conversion at named boundaries (`normalizeLlmFailure`; frozen failure value; hostile-accessor-safe extraction); "operation failure is a durable value, not an abort" | **Adopt with adjustment** | Adopt: one normalization boundary at the FFI edge (C status → typed Rust result; panics never cross the ABI; decode failure becomes the terminal Fact) and at the K0 mount/discharge edge. Adjust: inside Rust, `Result` already is the discriminated union — their throw→result→throw→fact round-trip collapses; only true boundaries need explicit normalization. |
| R3 | Startup failure delivery: state+event for non-awaiting observers, original error rethrown to awaiters; per-effect sync-failure rollback; teardown errors contained per-disposer | **Adopt with adjustment** | Adopt the two-channel principle: K0 already delivers activation failure as lifecycle state + `activation_error` (AI-4) and contains per-inverse violations as §G.6 latch. Adjust: no second lifecycle noun; activation error stays in mount/Reconcile results, never a new FiberDiagnostic message field (§J.3) without K0 authority review. |
| R4 | Three-way operation/protocol/cancellation classification; `TOOL_ABORTED` vs `ABORTED_BEFORE_DISPATCH`; observers cannot corrupt outcomes | **Adopt with adjustment** | Maps onto existing vocabulary: operation failure = D11 Failed; cancellation = Stopped (first-wins already demands the distinction — same body-started-vs-not shape); protocol/mechanism breakage = evidence that must never forge a Fact. Not portable: the "final-result bypasses post-execute" machinery (no tool pipeline). The observer-containment rule transfers (T2-2/T4-4). |
| R5 | Retry policy executed by a dedicated plugin; loop owns the extension point; durable first-wins retry bookkeeping; producer only classifies | **Adopt with adjustment (mechanisms), not portable (the plugin)** | Adopt the mechanism pieces for any FUTURE earned recovery: stable-code routing sets, backoff+jitter+caps, retry-after honor, abort-aware waits, first-wins durable bookkeeping. Not portable as a Recovery Plugin: Qianqian has no multi-provider policy-composition pressure; under D13 retry stays an owned effect of the operation's Plugin. All recovery remains NOT_EARNED today (disposition-contract §3). |
| R6 | Logger facade per fiber-scoped name; structured record; filtering/format/transport all in the exporter; sinks as fiber effects; ring-buffer default exporter | **Adopt with adjustment** | Unusually portable because their logging is NOT a plugin — matches T5's host-owned facility: component-scoped facade, structured owned record, per-sink levels, formatting at the sink/drainer, sinks as owned effects. Hard adjustment: zero logging on the RT path and the failure cone (their JS costs do not transfer; T5-2 is stricter). |
| R7 | Five dispatch modes with per-mode throw contracts; containment added at call sites that must not let observers break ownership; listener lifetime = registering fiber | **Adopt with adjustment** | Not portable as an EventBus (none exists; none earned). Adopt the disciplines: every existing callback point declares its panic/error contract (drain observer: synchronous, panic loses only the mirror; gate events: current-wins attribution; analyst: contained), lifecycle notifications contained so one observer cannot break cleanup, registrations owned by the registering owner's lifetime (ordinary K0 effects). |

**Explicitly not imported:** Error Plugin / Recovery Plugin / Logging Plugin / EventBus
plugin identity; the retry-plugin topology; the tool-pipeline protocol machinery; JS-style
exception flow inside Rust.

---

## DG target designs (2026-10-16 design round, BASE_SHA e44aa80)

Boundary-first target designs for the DESIGN_GAP work packages (#283 CODE-CHANGE-MAP §5).
Each design is a **candidate mechanism recommendation** — none is adopted, none is
implemented, and the owner-approved directions (A–J, recorded in
[error-system-decision-record.md](error-system-decision-record.md)) fix only the
direction-level constraints, not these mechanisms. Candidate interfaces are illustrative;
production APIs land only through their own review. Every code anchor is @e44aa80.

### DG-1 — Logging facility (T5-1/T5-2/T5-3/T5-5; absorbs ledger rows #8/#9/#10)

**Goal.** A host-owned diagnostic logging facility where (1) producers on sensitive
threads never block, allocate unboundedly, or panic on logging; (2) exporters/sinks
execute only on a dedicated drainer thread, never on producer call stacks and never
under a reliable-publication or resource-progress lock; (3) shutdown is bounded-or-honest
(no fake "drain complete"); (4) the machine protocol stdout never passes through it.

**Consumers and obligations.** Records are Class C diagnostics (candidate T2-1): no
terminal outcome, resource release, join, or Discharge may depend on a record being
delivered (direction A). Named consumers: (i) the human operator reading stderr/a file
(presentation); (ii) the App's shutdown report (dropped-count + self-diagnostic
counters); (iii) DG-3's typed fields (status/native_code) whose concrete consumer is
this record shape. No control-flow consumer exists or is proposed (C-6 re-verified).

**Owner.** The host App: installs the facade + drainer at startup, owns exporter
registration/withdrawal as ordinary owned effects (direction D), and tears down at
exit. Not a Plugin, not a K0 primitive, no EventBus (D13: no independent composition
identity needed — one process-lifetime resource).

**Execution-path / thread classification.**

| Thread class | Sites (ledger §4 rows) | May emit | Discipline |
|---|---|---|---|
| render (realtime leg) | #8 (post-catch abort line, wasapi.rs:258-262), #9 (opened line, :538-547), #10 (volume, :678-686) | mechanism diagnostics, gated (QIANQIAN_AUDIO_LOG default off stays) | `try_push` only; record fully constructed (bounded) before push; never format, never exporter |
| decode worker | (future decode-site diagnostics) | mechanism diagnostics | same |
| failure-publication cone (post-catch stop+complete region) | the #8 cone around wasapi.rs:263-273 | the abort record | logging must not sit between the catch and the reliable publications: the record is enqueued (non-panicking, bounded) and the cone's `render_input.stop()` + `drain.complete()` proceed; a lost record may never delay or skip them (direction E/A) |
| cleanup / teardown (session fiber effects, K0 `run_unwind`) | (future) | cleanup-failure diagnostics | `try_push`; teardown progress never waits on the drainer |
| normal host threads (main, stdin reader) | #1–#4, #7 (CLI report lines) | presentation diagnostics | may use the facade; protocol rows #5/#6 NEVER (T5-7: protocol stdout stays direct `print!`+flush — its failure is a host-operation failure, not a log loss) |

Forbidden everywhere: exporter/sink invocation on a producer thread; logging inside the
completion/verdict/edge locks; per-quantum logging on the steady render path (the three
gated sites are cold-path events: abort, opened-once, volume-once — `volume_diagnosed`
first-wins already bounds #10 to once per episode).

**Record shape (T5-1).** Bounded owned struct, sized/truncated at construction:
`{ site: Site (static enum — no runtime strings), level, class: Option<DiagnosticClass>, message: String truncated to ≤256 bytes at construction, status: Option<i32>, native_code: Option<i32>, seq: u64, ts }`.
The status/native_code fields are the DG-3 consumer target. Formatting happens only on
the drainer (matches the reference's sink-side formatting; R6).

**Producer path — compared queues (concrete type + version).**

| Mechanism | Producer path steps | Cost/bounds | Verdict |
|---|---|---|---|
| (a) `std::sync::mpsc::sync_channel` (std 1.97.1, bounded, pre-allocated ring) + `SyncSender::try_send` + one drainer `recv` loop | lock the channel mutex → write slot → notify receiver (only on empty→non-empty) → unlock. No allocation (fixed buffer), no syscall; notify is a futex/condvar signal under the same lock | enqueue = O(1) bounded steps; full queue → `try_send` = `Err(Full)` immediately (T5-2's "try_* is a bounded sequence of steps" — R1.2 correction respected: boundedness is of the producer's execution steps, not of wall-clock) | **Recommended** |
| (b) `crossbeam_queue::ArrayQueue` (0.3.x) `try_push` | lock-free CAS push | slightly cheaper under heavy contention; adds a dependency; single-drainer + cold-path producers give std no measurable contention | Rejected for now (no producer-path evidence demands it; the R1.2 "no crossbeam advantage verdict" stands — re-evaluate only with a measured contention datum) |
| (c) hand-rolled `Mutex<VecDeque> + Condvar + capacity` | explicit, but re-implements (a) with more code and the same lock | more surface to verify | Rejected |

Capacity: 256 records (≈128 KB at ≤512 B/record) — sized to cover one episode's
diagnostic burst; overflow policy below.

**Full queue / consumer stopped / exporter failure / reentry / self-diagnosis.**
Full: `try_send` fails → drop the record, increment a monotonic dropped counter
(AtomicU64, relaxed), and enqueue nothing else (no retry loop on the producer). Consumer
stopped (shutdown entered): same drop+count path; a late producer never blocks or revives
the drainer. Exporter failure (drainer side): the exporter is disabled for the rest of
the run (or of that record batch), its failure counted once in the self-diagnostic
counter, and at most one bounded line is written via the non-panicking stderr writer —
the drainer never calls back into the facade (no reentry: the facade's producer entry is
a no-op when `thread::current()` is the drainer, enforced by a drainer thread-flag).
Resource retirement: records hold only owned bounded data — no handles, no `Arc`
cycles; the queue itself is dropped after the drainer join.

**Shutdown strategies (compared).**

| Strategy | Contract | Assessment |
|---|---|---|
| A — admit only exporters with a verifiable bound | every exporter must implement and document its own progress bound (e.g. deadline `poll(2)` on a nonblocking stderr pipe; plain regular-file writes get an explicit unbounded-residual declaration) | **Recommended as the admission rule.** Honest boundary: even under A, a stalled disk on a file sink is environment-unbounded — each exporter states its residual; A is a contract, not a physics proof |
| B — allow blocking sinks with an explicit abandon boundary | drainer joined with a deadline; at process exit the App may stop waiting (abandon) — threads may still be alive, resources they own are NOT released early, logs are lost, and the boundary is exactly process exit | **Adopted for the exit path only**: the App's shutdown reports the abandon (dropped/self-diagnostic counters), never claiming "drain complete" (T5-5 honesty). 适用边界 = 进程退出; not applicable to per-episode teardown |
| C — separate-process isolation | logger in a child process, IPC transport | Rejected without evidence: no failure class on record requires it; introducing it would add a lifecycle/transport surface the razor forbids |

The trilemma stands as stated: arbitrary sink + zero loss + bounded exit cannot hold
simultaneously; the design claims only "bounded producer paths + bounded-or-abandoned
drainer + counted losses".

**Cost.** Producer: one mutex acquire/release + slot store + occasional notify
(cold-path only). Drainer: one thread, amortized formatting. Memory: capacity × record
bound. No realtime-path cost (no steady-path logging exists to move).

**Migration and deletions.** Landing order is owner's (D-3: correctness-equal):
either the narrow non-panicking writer lands first and the facility later replaces it
(diff-baseline visibility), or the facility lands first and absorbs #8/#9/#10 directly
(CC-1 subsumed). In both cases the landing PR must state the residual full-pipe
blocking (a synchronous stderr/file exporter still blocks the drainer — producers are
isolated, the drainer is not) in the same row that marks the panic class resolved
(H). The `QIANQIAN_AUDIO_LOG` gate maps to a site/level filter on the stderr exporter.

**Evidence.** Reference facts @5719020 (logger.ts): named facade + structured record +
per-exporter thresholds + sink-side formatting + exporter lifetime = registering fiber
→ adopt; its **synchronous inline exporter loop** (`Logger._method`, logger.ts:154-159:
no queue, no isolation, exporter throw propagates to the producer call site) and its
process-crash admission (fiber.ts:287-292) are the documented anti-pattern (direction E:
that implementation cannot serve as Qianqian's isolation evidence). Current-tree facts:
three gated sites and their catch structure (wasapi.rs:251/:320/:258-262/:538-547/:678-686);
protocol/diagnostic separation (machine_input.rs:230-234).

**Remaining decisions (owner).** Capacity number; record message bound; whether the
dropped/self-diagnostic counters are surfaced beyond the shutdown report; exporter
admission texts for file sinks; install point (App startup vs first use).

### DG-2 — Output-reason lifecycle (T2-5; D-10/D-9)

**Goal.** Deliver a bounded, owned explanation of an output-backend abort to the
consumer that already receives the verdict — without letting the reason participate in
terminal adjudication (direction F) and without bending the backend→playback dependency.

**Consumers and obligations.** The App presentation surface (`failure_diagnostic`
text for a `Failed` episode; handle.rs:87 keeps the public terminal enum three-variant)
is the concrete consumer; programmatic consumers: none (C-6/D-9 re-verified). Obligation:
when the backend knows why it aborted, that explanation is delivered with the verdict;
absence never changes classification or control flow (F), but "control-invariant under
absence" does not discharge the delivery obligation — the producer must attempt
delivery on every abort.

**Owner.** The Output Plugin (backend) produces the reason at the abort site; the
backend-neutral `AudioOutput`/port types carry it; the Playback Session mirrors it
beside the drain verdict; the App reads it via the observation surface only.

**Scope (which aborts carry a reason).** (i) normal device abort
(`LoopOutcome::Aborted { message }` — the reason source exists today and is discarded at
wasapi.rs:270-273); (ii) caught render panic (the two `Aborted{"…panicked"}` shapes,
wasapi.rs:254-256/:323-326); (iii) post-catch failure (the #8 cone's own failure path);
(iv) open failure — **out of scope for the drain carrier**: `OpenVerdict::Failed{message}`
already reaches `OutputError{message}` → `activation_failed` (wasapi.rs:168-171 →
session.rs:418 → completion.rs:442-447), which is a diagnostic, not a D11 terminal —
unchanged by this design.

**Compared carriers.**

| Carrier | Shape | Assessment |
|---|---|---|
| A — RenderRequest diagnostic slot (the `level`/`position` pattern; ports.rs:962-985) | session allocates a cell in the request; backend writes it at abort | Rejected as primary: adds a second per-episode mutable cell whose read/clear lifecycle, retirement and double-write rules must be specified beside the verdict it explains; the reason then travels decoupled from the fact (载体关联 cost); no realtime-path advantage (abort is cold-path in both) |
| **B — reason rides the verdict completion interface (recommended)** | `DrainVerdict::Aborted` gains a bounded owned payload (e.g. `AbortReason { class: OutputAbortClass (static enum), message: String≤N }`); the existing single `DrainSignal::complete(verdict)` seam (ports.rs:135-155), first-wins cell, observer-outside-verdict-lock — all unchanged; the payload is constructed at wasapi.rs:270-273 where `LoopOutcome::Aborted{message}` is currently discarded | Recommended: the explanation is delivered atomically with the fact it explains (one call, one cell, one first-wins rule — no separate late/duplicate/retirement semantics to invent); observer signature `Fn(DrainVerdict)` is unchanged; classification still reads only the verdict shape |

**Publication point and ordering.** The reason is published exactly with the verdict
(single `complete` call) — before any observer runs (observers fire after the verdict
lock, ports.rs:145-154) and before the resolver can see `drain_verdict`. Duplicate:
impossible (first-wins). Missing: a backend that cannot construct a payload (e.g.
message allocation failure) delivers `Aborted` with an empty/class-only reason —
verdict adjudication unchanged. Late: impossible (same cell). Receiver retirement: the
payload lives in the mirrored completion state for the episode's lifetime; retirement
follows the episode, no extra rule.

**Rust-side mirror and presentation.** `SessionCompletion`'s drain observer
(completion.rs:386-403) mirrors the payload beside `drain_verdict`; `resolve` does not
read it (the `Failed{stage:"device"}` classification path, completion.rs:1302-1310, is
untouched — direction F); the presentation diagnostic for device failures may include
the reason text (presentation-only enrichment of `failure_diagnostic`). Whether that
enrichment is exposed publicly is the T1-3/D-9 exposure question (owner).

**PBK-003.** No verdict-semantics change: the AudioOutput obligations keep "abort ⇒
Aborted verdict" exactly; the payload is explanatory. The interpretation note (migration
§3.4 candidate) is still required and its concrete text should describe carrier B —
see the updated candidate in migration §3.4 (this round). Backend never learns playback
types; the payload is audio-api-typed.

**Rejected alternatives.** Reason-as-`retryable`-style policy field (forbidden,
direction C); reason consulted by the resolver (forbidden, F); per-request slot A
(above); a new observation channel beside the verdict (duplicates B's job with a second
lifecycle).

**Evidence.** wasapi.rs:233-238 (`LoopOutcome::Aborted{message}` exists),
:270-273 (discarded today); ports.rs:82/:122/:135-155 (single-arg seam, observer
outside lock); completion.rs:386-403 (mirror), :1302-1310 (device classification),
handle.rs:87-97 (public terminal + separate diagnostic). DeepSeek reference R1
(code/message split — class enum + message text mirrors it).

**Remaining decisions (owner).** D-10 (carrier adoption — this design recommends B);
D-9 (whether presentation enrichment counts as the concrete consumer); message bound N;
the `OutputAbortClass` enumeration (candidate: `Device`, `Panic`, `Cone`, `Open` —
final names at implementation review).

**Migration and deletions.** On adoption: the three gated print sites' information
moves into the payload (#8's abort message becomes the reason; #9's opened-line context
stays a log record, not a reason — it is not an abort); the `QIANQIAN_AUDIO_LOG`-gated
aborted-line print at wasapi.rs:258-262 is then redundant with the delivered reason and
can be deleted or kept as the logging-site twin (DG-1 migration row).

### DG-3 — Adapter reason retention and consumers (T1-1/T1-2; ties CC-4; D-2)

**Goal.** Retain the SongCore diagnostic (status + native_code + message) at the error
site as a bounded owned copy, deliver it to its named consumers, and read it in `probe`
before close — without adding an unread long-lived `last_fault` field (direction G) and
without changing the public `DecodeError`/`DecodeOpenError` shapes.

**Consumers and obligations.** (i) DG-1's record fields `status`/`native_code`
(the logging facility is the concrete programmatic consumer — T5-1's typed fields);
(ii) the App presentation surface via the failure-diagnostic text (message). No
control-flow consumer (C-6). T1-3 admission: public field NOT required — retention is
crate-internal + consumed at the logging seam; if a public field is ever requested it
re-enters T1-3 with its own obligation proof.

**Compared retention shapes.**

| Shape | Assessment |
|---|---|
| (a) copy-at-error-point + immediate emission | at the `_` arm (lib.rs:288-290) construct `DecodeFault{status, native_code, message: bounded}` from the borrowed `song_error` before any next same-handle call (T2-3 discipline), emit via DG-1 try_push (site = Decode), drop | **Recommended**: no idle state, bounded by construction, consumed at the point of maximal information |
| (b) long-lived `self.last_fault: Option<DecodeFault>` | Rejected as primary: an idle field with no reader obligation between errors — exactly the "future use" shape direction G forbids; stale-fault attribution rules would have to be invented; the disposition-contract §5 warning applies |
| (c) status-only (today) | the confirmed loss (U-ledger / #231 L1/L2) — baseline being replaced |

**Probe read-before-close (direction G, explicit design goal).** Current:
lib.rs:189-200 closes the handle without reading the diagnostic the C side already
latched (`fail(h, …)` on the probe path). Design: `song_probe` failure → read
`song_last_error` on the handle **before** `song_close` → embed the owned bounded copy
into the `DecodeOpenError` message text (public shape unchanged; CC-4 shape (i)). The
C-side probe `fail()` paths (e.g. songcore_ffmpeg.c:877 region) make the read
available — no ABI change.

**Operation-result vs diagnostic-export independence.** The `DecodeFault` copy never
feeds the `Err(DecodeError{message})` construction beyond the message text that already
flows; classification (`classify_seek_status`, lib.rs:254-260) is untouched; the
emission is best-effort (DG-1 drop policy) — a logging failure can never turn a decode
result into a different decode result (A).

**Evidence.** lib.rs:283-292 (`_` arm), :188-200 (probe), :213-226 (last_error copies
message, ignores `native_code`); C:1111-1113 (latch republish keeps status + native,
generic text), C:1155-1156 + `ok(h)` (partial-success clears the message text — the
first-failure text is lost at the latch; if preserving it is ever required that is a
C-side change and a separate D-5-class ruling, not assumed here); songcore.h ABI
(borrowed-diagnostic lifetime ≤ next same-handle call).

**Remaining decisions (owner).** D-2 adoption (this design is the DG-3 bundle the
CC-4 REV.2 shape (i) requires: the consumer is named); message bound; whether the
DecodeFault's class is ever surfaced in the failure diagnostic (D-9).

**Migration and deletions.** CC-4's `DecodeFault` internal retention = this design's
shape (a); the binding-architecture NOTE (migration §3.5) rides along; nothing public
deleted; `last_error()`'s ignored-`native_code` gap closes by feeding the field into
the retained copy.

### DG-4a — File-I/O callback fault retention (T1-6; E-3 is its evidence vehicle)

**Goal.** Retain the host `io::Error` class behind the `song_io` callbacks as bounded
owned data, attributed as **correlated evidence**, and keep the callbacks panic-free
across the C ABI.

**Owner and lifetime.** The binding crate owns the userdata context:
`IoContext { file: File, fault: IoFaultSlot }` with
`IoFaultSlot = Mutex<Option<BoundedIoFault>>` (first-failure-wins + an AtomicU64 count
of subsequent failures). Today's userdata is the leaked `Box<File>`
(lib.rs:158-168, reclaimed via `Box::from_raw` on error paths :172/:179/:192, moved
back into the stream struct on success :209) — the context wraps the same ownership
pattern; operation boundary = the handle lifetime (FFmpeg's avio keeps calling
read/seek during decode, so the slot spans open+reads, not just open).

**Multi-callback-failure policy (compared).** first-wins + bounded count
(recommended: preserves the origin, O(1) memory, matches the everywhere-else
first-wins discipline) vs last-wins (loses the origin — rejected) vs bounded summary
list (more state, no consumer — rejected).

**Stale-cleanup / success / internal recovery / partial success (attribution rules).**
- Successful `song_open` after transient callback errors FFmpeg internally retried:
the slot's content is **not** a failure — it is cleared at open completion (stale
cleanup) and may only appear in diagnostics if an operator asks (never automatically).
- Failed open: the slot rides the `DecodeOpenError` diagnostic (host-side slot works
even where the C side leaves no usable latch — songcore.h:216-219 notes song_open
failure diagnostics; the slot needs no ABI change, which is exactly the R1.1 flip).
- `read_frames` Err: an io fault in the slot is **correlated evidence, not the
auto root cause** — the C-side decode error may have another origin; both are reported,
attribution stays with the operation result.
- Partial success: the slot describes io-level facts only; it never modifies the
partial-success/pending-fatal distinction (ABI-frozen).

**Panic-freedom across the C ABI.** The callbacks (`file_read/file_seek/file_size`,
lib.rs:70-104) are `unsafe extern "C"`: the design constraint is panic-free by
construction — callback bodies contain only io operations + a bounded slot write; no
unwraps, no formatting, no allocation beyond the bounded fault struct. E-3's harness
exercises fault-injecting callbacks at the ABI level (C-side mapping: callback −1 →
AVERROR(EIO), io_read C:200-208 / io_seek C:210-242); the adapter-side slot is covered
by binding-crate unit tests per synthesized (kind, errno) pairs — E-3 accepts DG-4a
and nothing else in the DG-4 family.

**What E-3 can and cannot prove.** Can: the C side's mapping of failing callbacks
(EIO propagation, latch interaction, open-failure reachability), deterministically, on
any platform with the native lib built. Cannot: real-disk stall classes, the FFmpeg
retry behavior under every callback failure shape (bounded fixture set), and anything
about DG-4b/4c (explicitly out of E-3's acceptance).

**Remaining decisions (owner).** T1-6 adoption; the `BoundedIoFault` fields
(kind-enum + raw_os_error + offset — final set at implementation review); whether the
slot is ever surfaced beyond open-failure diagnostics.

**Migration and deletions.** `Box<File>` userdata → `Box<IoContext>` (same
leak/reclaim discipline); no ABI change; CC-4/DG-3's DecodeFault and this slot remain
separate (decode-status vs host-io origin) with a stated precedence rule for
presentation only.

### DG-4b — Rust panic hook posture (T5-8)

**Goal.** One process-level panic-hook policy: a single installer, isolation for
critical threads, conditional chaining elsewhere, never-restore on exit — and no panic
recovery is granted by any of it.

**Current fact.** Single installer `install_reader_panic_hook`
(machine_input.rs:192-207): `Once`-guarded install, `take_hook()` captures previous,
chaining condition = `thread::current().name() != Some("qianqian-stdin")`
(:200-202), never restored after the host returns (:205-207 — rationale: a detached
reader must not late-print after seal). Reader-side panics are separately caught at
three seams (:119 → `HostFailure::Panic` :142-145; :253-255; :277 → :288-291).

**Compared postures.**

| Posture | Assessment |
|---|---|
| status quo (filter-only hook) | chains everything except the reader thread; suppresses the default print on the reader thread only |
| **critical-thread isolation + conditional chaining elsewhere (recommended)** | the hook maintains a registered set of critical thread names (`qianqian-stdin` today; render/decode names if their default prints ever need suppression): on a critical thread the hook does bounded, panic-free bookkeeping only (a pre-formatted static line via the non-panicking writer at most) and **never calls the previous hook** (an unknown synchronous sink must not run on a thread whose timing/allocations are load-bearing); on any other context it chains to the captured previous hook (preserves host/user tooling — today's behavior) | 
| full-process replacement (never chain) | Rejected without evidence: silently breaks debuggers/tracers/hosts that installed meaningful hooks; no failure class on record demands it |

**Context identification.** Thread names registered at spawn
(`Builder::name(...)` — machine_input.rs:215-218 already names the reader). Cost ~zero
(a lock-free read of the registered static set). Reliability limits: unnamed threads
and name collisions resolve to the chaining branch (fail-open to today's behavior —
the isolation is opt-in per named critical thread, so a miss cannot create a new
hazard, only fail to suppress a print).

**Previous-hook admission.** Chaining is allowed only off critical threads (above).
The hook cannot introspect what the previous hook is; the design therefore does not
claim unknown sinks are safe — it confines them to non-critical contexts and records
that confinement as the T5-8 ruling point (the "chain-to-previous is a condition, not
an unconditional obligation" R1.2 wording becomes: the condition is
`!critical_thread(current)`).

**Reentry / allocation / blocking / recording failure.** The critical-thread path is
panic-free and allocation-free (static pre-formatted line + `libc::write`-class
non-panicking write or the DG-1 non-panicking writer — never `println!`/`eprintln!`);
a failed write increments an atomic self-counter and returns; the hook never calls the
logging facade (a panic inside the hook must not recurse). Exit recovery: none
(never-restore stays, documented rationale above).

**Hook isolation grants no recovery.** Catching/suppressing a default print is not
catching the panic: unwinding still proceeds into the thread's own catch_unwind seams
(render: wasapi.rs:251/:320; reader: machine_input.rs:119/:253/:277) or, on the main
thread, exits the process (D-7 unchanged).

**Evidence.** machine_input.rs:190-207/:209-240 (facts above); render-thread catches
(wasapi.rs:251-256/:320-327); main-thread zero `catch_unwind` (#283 AF-7 re-verified).
A dedicated reentry/isolation witness test belongs to this package (not E-3): inject a
panic on a named critical thread in a subprocess, assert no previous-hook output and
process behavior otherwise unchanged.

**Remaining decisions (owner).** T5-8 adoption; the critical-thread registry
membership (reader-only vs reader+render+decode names); whether the bookkeeping line is
emitted at all (pure silence is also admissible).

**Migration and deletions.** The install site stays machine_input (single installer
rule); if DG-1 lands, the hook's write path reuses its non-panicking writer — the hook
itself never becomes a facade producer.

### DG-4c — FFmpeg av_log posture (T5-9)

**Goal.** Keep av_log discard-by-default; design an optional, opt-in, process-level
bridge into DG-1 with bounded, lock-honest callback discipline; no per-handle routing.

**Current facts (C, @e44aa80).** Discard callback `songcore_discard_av_log` (C:50-56);
`songcore_quiet_ffmpeg_logging` (C:58-61): when `QIANQIAN_FFMPEG_LOG` is **unset** it
installs the discarder; when set it leaves the callback untouched (FFmpeg's default
logger remains). Re-installed on every `song_open` (C:822 — same-value pointer store,
off any realtime path). The callback is process-global: concurrent handles share it
(discarder is stateless). SongCore asserts it is the process's only FFmpeg user
(C:43-44). **Not knowable from this tree**: which FFmpeg-internal locks are held while
the callback runs — FFmpeg is pinned (native/ffmpeg/pin.json) but not vendored; this
is a stated assumption to verify, not a fact (see EVIDENCE_PENDING).

**Compared mechanisms.**

| Mechanism | Assessment |
|---|---|
| **default discard (baseline) + optional controlled process-level bridge (recommended)** | discard stays the default (T5-9 PRESERVE); the bridge is opt-in: host sets `QIANQIAN_FFMPEG_LOG` (existing switch — SongCore then never touches the callback) and installs one bridge callback at startup **before the first open**, copying each line into a fixed pre-allocated buffer (bounded memcpy + truncation) and try_push-ing it into DG-1's queue |
| per-handle routing | Rejected: `av_log_set_callback` is process-global; per-handle routing would require an avctx→context map locked inside the callback — cost and fragility with no consumer today |
| always-on bridge | Rejected: default-on diagnostics contradict the quiet-by-default posture and put callback cost on every FFmpeg call |

**Callback discipline (the load-bearing constraints).** Inside the bridge: no FFmpeg
API re-entry; no allocation (fixed buffer, truncation by memcpy bound); no unbounded
lock (try_push takes the queue's internal mutex — bounded; the queue is never held
across an FFmpeg call by construction, so self-deadlock is structurally excluded);
formatting happens on the drainer. The design does **not** assume which FFmpeg lock is
held at callback time (direction: no presupposition): the discipline above is chosen so
that the answer to that question cannot change correctness — only the pending
source-verification can state what actually holds.

**Install/reinstall/concurrency.** Bridge installs once at host startup, before any
`song_open`; SongCore's per-open reinstall only fires when the env var is unset, so a
bridged host (env set) is never clobbered — the ordering constraint is documented as
part of the T5-9 ruling text. Concurrent handles all share the bridge; records carry
no handle identity (process-level diagnostic; per-handle attribution is explicitly not
provided).

**Copy/truncation/queue cost and error handling.** Per line: one bounded memcpy into
the fixed buffer + one try_push (DG-1 producer discipline) + drop-on-full counted.
av_log lines above the buffer bound are truncated at the copy, never reallocated.

**E-3 is not acceptance for this package.** E-3 exercises song_io callbacks at the ABI
level; av_log behavior under the pinned FFmpeg build needs its own runtime observation
(belonging to this package, Windows/FFmpeg-runtime bound), and the Rust panic hook
(DG-4b) likewise has its own witness. No acceptance is claimed here beyond static
facts.

**Remaining decisions (owner).** T5-9 adoption of the bridge option (vs
discard-forever); bridge buffer bound; whether the bridge is compiled in behind a
cargo feature or always available but env-gated.

**Migration and deletions.** None forced: discard path unchanged; the bridge is
additive and off by default. If the bridge is adopted, `QIANQIAN_FFMPEG_LOG`'s
documented meaning extends from "keep FFmpeg's default logger" to "host owns the av_log
callback" for bridged hosts — a doc note in the binding architecture, not an ABI
change.

---

## Owner decision points (each with options; nothing decided here)

| ID | Question | Options | Recommendation (non-binding) |
|---|---|---|---|
| D-2 | Probe diagnostic read (T1-2) | (a) adopt one-line read; (b) leave as recorded loss | (a) — cheapest confirmed-loss fix, no contract touch |
| D-3 | Cone write-failure isolation (T5-4) | (a) narrow corrective now (non-panicking writer on ledger rows #8/#9/#10 — **partial**: removes panic escalations, full-pipe blocking remains until the framework); (b) fold into future logging work; (c) leave as documented hazard | (a) — disposition-contract §6 already anticipates "a separate, minimal corrective"; independent of framework choice; the residual blocking must be stated in the landing PR, not silently dropped |
| D-4 | DRAIN_CAP semantics (C-8) | (a) keep as OPEN resolver tuning; (b) amendment defining slow-drain evidence class | (a) until E-5 runtime data exists; any (b) is an AI-7-space authority act; E-5 supplies the datum, not the verdict |
| D-5 | Superseded-fatal witness (T3-1) | (a) adopt diagnostic witness — requires an ABI/SongCore mechanism amendment FIRST (currently unobservable across the ABI, R1.1); (b) accept silent supersession as ABI-coherent | (b) first — the latch belongs to the replaced continuation; (a) must price the ABI change, not assume a local read that does not exist |
| D-7 | Panic-in-teardown-inverse handling | (a) K0 authority review of catch→Violated at dispose boundary; (b) keep crash-propagation, document it | (b) first (honest, cheap), (a) only if owner wants process survivability |
| D-9 | "Concrete consumer" bar (C-6) | (a) presentation obligations count (name the surface, the failure mode it must explain, and why the local path cannot); (b) require programmatic consumers (diagnostic-only fields wait) | owner ruling; gates T1-3 applications incl. T2-5's exposure question. R1.1: the bar is obligation-based — consumer syntax (`format!` vs code) is not the discriminator |
| D-10 | Output reason snapshot carrier (T2-5) | (a') session-owned cell handed off as a `RenderRequest` field (audio-api-typed, the `level`/`position` pattern), read session-internally; (b) parameter on the `drain.complete`-adjacent delivery API | both carriers cross the backend-neutral port crate (R1.1 path drawing) — the genuine open sub-question is observation-surface exposure (T1-3/D-9), not "port vs no port"; migration §3.4's interpretation note applies either way |
| D-11 | Exit-code refinement | already #231 open question; unchanged | defer |
