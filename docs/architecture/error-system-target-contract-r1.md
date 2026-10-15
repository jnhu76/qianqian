# Error-system target contracts — R1 companion (#231, #280)

> **Truth class: DESIGN_CANDIDATE / NOT AUTHORITY / NOT IMPLEMENTED.**
> Every goal below is a **candidate obligation**. Nothing here changes D11, D14.*, U2,
> the SongCore ABI, K0 semantics, or any accepted document; merging or reviewing this file
> authorizes no implementation and adopts no policy. Where a goal would change frozen
> behavior or accepted authority, it is marked `AUTHORITY-AMENDMENT-REQUIRED` and its
> concrete candidate text lives in
> [error-system-migration-r1.md](error-system-migration-r1.md) §3.
> **Evidence basis:** the frozen facts in
> [error-system-reconciliation-r1.md](error-system-reconciliation-r1.md) (R1) at
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
| **T1-3** Public-field admission gate (carries R0 §5 forward, verifiable form) | **Basis:** R0 §5.1-5.4; D14.2. **Target:** a public error/observation field may be added only with (a) a named consumer **obligation** — which surface or programmatic consumer, which diagnostic/control duty it discharges; the DUTY is the criterion, not the syntax the consumer uses (rendering an error reason may legitimately run through `format!`), (b) proof local reporting is insufficient for that duty, (c) smallest backend-neutral shape, (d) for diagnostic-only fields, a demonstrated control-inertness witness (a test that flips the field and observes zero control divergence). **Change:** NEW-OBLIGATION (process, not code). **Counterexample:** a field no admitted consumer obligation uses — whose readers, if any, are incidental (a stray `format!`, a test) rather than the duty the field was minted to serve. |
| **T1-4** Stable code ≠ raw code passthrough | **Basis:** CE2 (107 never produced in the decode loop; 108 lumps media, host-I/O-adjacent and decoder-internal). **Target:** if/when any stable product-facing class exists, it is minted at the layer that can prove it, never forwarded from `song_status` verbatim; SongCore vocabulary must not leak into product enums (binding-architecture boundary). **Change:** NEW-OBLIGATION. |
| **T1-5** No mega-enum, no unbounded cause chains | **Basis:** R0 §1 (four dimensions, non-exclusive); current six-carrier inventory (A REPORT §一). **Target:** representation stays per-operation results + typed session origin + presentation strings; a cause snapshot is bounded (≤ N bytes, truncated, owned, copied inside the same-handle window); no universal `QianqianError`, no `source: Box<dyn Error>` chains across owners, no `retryable: bool`. **Change:** PRESERVE. |
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
| **T2-2** Settlement ownership untouched | **Basis:** D11/D14.3; first-wins commit (completion.rs:1238-1249); settlement on the session-owned publication path. **Target:** PRESERVE exactly — publish→resolve→commit→notify in one lock hold; no resolver thread; no observer-created terminals; read/wait stay pure. **Counterexample:** any path where `observe_snapshot`/`wait_terminal` could commit, or where a C-class consumer must run before `drain.complete` returns decisively. |
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
| read (partial success) | N frames SONG_OK; fatal next read | latch in C handle | continue until surfaced/stopped/seek-replaced | none (ABI contract) | surfaced at next read → D11 Failed | none | N frames delivered, no rollback (R0 §3) | CURRENT; CANDIDATE witness T3-1 |
| read (fatal) | typed error, 0 frames | edge.fail, worker_failure first-wins | no old-cursor resume | none | D11 Failed{decode} | none (#277 untouched) | buffered PCM not withdrawn: edge-buffered frames abandoned (render-side terminal-before-data); pulled-and-submitted frames MAY be heard but audibility is never asserted — `ReleaseBuffer` can fail after the pull and a stop/device failure can precede consumption (R1 §2.3.1 evidence-level vocabulary, R1.2) | CURRENT |
| seek RefusedUnchanged | old continuation proven intact | remainder finished exactly | continue == no-seek (bit-exact) | worker completes remainder | none | none | remainder zero-loss | CURRENT (production-dead class, contract-live) |
| seek Applied | new cursor + landing evidence | staging discard + history invalidate + purge | commit/rebase per D14.5 | worker protocol | none | none | pre-cut PCM purged; rebase routed | CURRENT; CANDIDATE: superseded-fatal witness T3-1 |
| seek MutatedThenFailed | none (old continuation unprovable) | release-without-commit + edge.fail | never resume | none | D11 Failed{decode: seek failed} | none | none | CURRENT (SEEK_UNSUPPORTED reclass = AUTH-AMEND, migration §3) |
| DSP live refuse | old config continues bit-exact | `last_refusal` latched | continue | control records refusal | none | none | n/a | CURRENT (D14.11/§7.3) |
| DSP accepted failure | none | processing_failed | none | none | D11 Failed{processing} | none | partial block discarded | CURRENT (bypass = NOT_EARNED) |
| volume apply failure | last applied level holds | attempted-bit latch | continue | mechanism retains level | device-invalidated → abort path | none | n/a | CURRENT (D14.9); diagnostic write hazard = N-01 class, T5-4 |
| render steady failure | none | edge stop + verdict | none | none | D11 Failed{device} (or Stopped with intent) | none | device-side frames as rendered | CURRENT; reason delivery = T2-5 |
| drain | padding==0 → Drained | verdict | 5 s cap (local tuning) | none | deadline → Aborted → Failed{device} | none | handed-off accounting | CURRENT; cap semantics = OPEN-DECISION D-4 (AI-7 space) |
| stop | Stopped, or Failed{decode} if a pending fatal surfaces first (R1 §2.3.1 commit race) — first-wins governs terminal COMMIT, not command arrival | intent + gate release + edge stop | — | inert if late | — | — | — | CURRENT |
| replacement | old stopped → fresh episode | whole-composition swap | probe-first (D14.6) | App owns | fail-stop on teardown violation | none | old terminal independent | CURRENT |
| activation | Established or clean failure | unwind LIFO | retry = new Open | none | ActivationFailedClean / FailStop | none | attempted root disposed | CURRENT |
| teardown | Discharged or Violated | LIFO latch | — | none (inverses are total by contract) | §G.6 latch, no exit | none | partial discharge = violation | CURRENT; panic-in-inverse = OPEN-DECISION D-7 (K0 review) |
| host input failure | infrastructure report | first_failure + stop | — | none | exit 1, never a Fact | none | n/a | CURRENT |
| process exit | code {0,1,2} | — | — | — | — | — | reports after dispose/seal | CURRENT; refinement = #231 open question |

| Goal | Record |
|---|---|
| **T3-1** Superseded-pending-fatal witness (CANDIDATE — **currently unobservable, mechanism OPEN**) | **Basis:** R1 §2 (seek-Applied erases a latched fatal with no trace). **R1.1 correction of the original premise:** the latched fatal is NOT "readable pre-seek via the T1-1 snapshot". In the partial-success state the binding saw only `SONG_OK` + frames (lib.rs:286 — the success arm never reads any error state); `ok(h)` has already cleared the one-shot diagnostic that `song_last_error` exposes (C:133-138, C:1155-1156); `fatal_error` itself is internal handle state with **no ABI query**; and a successful `song_seek` clears it before returning (C:1196). The information genuinely does not cross the ABI boundary, so adapter-internal retention alone cannot implement this witness. **Target (if wanted at all):** requires a NEW information source first — an ABI amendment (e.g. a pending-error query) or a SongCore-internal change; the witness then rides the same Class-C diagnostic path as T2-5 — never into classification. **Change:** OPEN-DECISION with a mechanism gap; NOT a LOCAL-REP. **Counterexample:** any control branch or terminal change keyed on the note. **Open:** OPEN-DECISION D-5 (is the witness wanted at all? an ABI amendment for a diagnostic note is a high price; doing nothing remains the cheap legal choice). |
| **T3-2** Explicit non-recovery dispositions | **Target:** the "Recovery owner: none" cells above are the adopted-by-default posture; any future recovery candidate must arrive with the R0 §3 NOT_EARNED proof obligations, unchanged. **Change:** PRESERVE. |

---

## M4 — Lifecycle errors

The five independent facts (R1 §3.5) and their code-enforced relations are the baseline.
Target obligations:

| Goal | Record |
|---|---|
| **T4-1** Five-fact independence is never collapsed | **Target:** design/review language and any new code must keep terminal commit / thread exit / resource release / discharge verdict / join death-cause as separately evidenced facts; no timeout return, projection, or log line may stand in for any of them ("a timeout return must never impersonate thread-exited or resource-released"). **Change:** NEW-OBLIGATION. **Counterexample:** any API returning "stopped" after a deadline without `worker_gone`/join evidence. |
| **T4-2** Original failure and cleanup failure are both preserved | **Basis:** §G.6 partial discharge is a violation, never a noted success; but a panicking inverse today crashes instead of latching (R1 §3.4). **Target:** when cleanup fails after a primary failure, BOTH survive: the primary (already committed or queued as Class-B evidence) and the cleanup verdict (Discharge/Violated or — candidate — a caught-panic latch). They are rendered together at the App (disposal warnings already exist). **Change:** PRESERVE for the explicit-Violated path; OPEN-DECISION D-7 for panic→latch. |
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
| **T5-1** Record shape | **Target:** owned record = {component: &'static str, level, op/stage tag, bounded cause snapshot (typed code + native code + truncated message ≤ N bytes), optional episode-local context if already at hand, monotonic timestamp}. No Episode ID invented (R0 §6). Size/truncation enforced at construction. **Change:** CANDIDATE. |
| **T5-2** RT/firewall entry | **Target:** RT and failure-cone producers use only non-blocking `try_push` with pre-sized storage — **the concrete queue type must be shown to match that promise along the full send path** (construction, enqueue, contention retry, wakeup, destruction), pinned to type + version; the `try_*` name, "bounded", or "pre-allocated buffer" alone guarantee nothing (R1.2: a pre-allocated fixed buffer still does not bound every producer's execution steps); no formatting on producers; a full queue increments a drop counter and moves on. Logging enabled/disabled/dropped/full **never** changes any operation result or terminal. **Counterexample:** any divergence between run-with-logging and run-without beyond the log bytes. |
| **T5-3** No exporter under settlement/resources | **Target:** no sink call while holding the completion lock, the edge lock, or any resource an owner needs for terminal progress; exporter failures increment counters and are themselves logged best-effort. **Basis:** R0 §6 (carried forward, now with the R1 §4 ledger as the baseline map). |
| **T5-4** Producer write-failure isolation (partial remediation, honestly scoped) | **Target:** diagnostic writes from the failure cone must not panic the producer — a write-failing sink is diagnostic loss. **Scope (R1.1, unified table in migration §1.5):** the cone sites are ledger rows #8 (post-catch abort line), #9 (opened line — its panic unwinds into catch #1 AFTER the `Opened` verdict published, cascading to N-01 or a post-activation `Failed{"device"}` misattribution) and #10 (volume); all three swap to a non-panicking writer. #11 (panic hook + chain-to-previous, which restores the default hook's synchronous stderr for every other thread) and #5 (reader protocol writes) are separate classes (T5-8; migration §1.7) and are NOT fixed by a writer swap. **A non-panicking writer is a PARTIAL remediation: it removes the panic/skip escalations; full-pipe BLOCKING remains on every synchronous site until the T5 framework lands (T5-2/T5-3) — the firewall as a whole is not satisfied by it.** This supersedes today's `eprintln!`-panics semantics on exactly those sites; the narrow corrective is independent of any framework adoption — OPEN-DECISION D-3. **AUTH-AMEND:** none (no frozen semantics touched; D14.9 grading unaffected). |
| **T5-5** Lifecycle and shutdown race | **Target:** the facility is host-owned (installed before episodes, drained at host exit); producers never block on the drainer; at shutdown the host drains with a bounded deadline and reports dropped-record counts; no producer waits for a consumer. **Bounded JOIN of the drainer is NOT a property of this shape (R1.1):** a drainer inside a blocking sink call ignores deadlines — the queue's boundedness and the producers' `try_push` discipline bound the producer side only. Bounded join requires exporter admission (cancellable/timeout-capable sinks only), sink isolation (platform-dependent spawn/kill), or abandoning the thread at process exit; this mechanism is OPEN and must be settled before any "bounded shutdown" acceptance claim is made. **Counterexample:** exit hanging on a log flush after the terminal Fact is committed — including via a `join` that outlives its own drain deadline. |
| **T5-6** Sensitive-path policy | **Target:** default diagnostics avoid unnecessary private filenames/absolute paths (R0 §6 carried forward); richer export is a separate product/privacy decision. Current F-17 (paths in decode messages) is recorded as the baseline to improve, not a defect to silently change (protocol-visible texts are contract). |
| **T5-7** Machine stdout keeps its protocol role | **Target:** protocol rows (`source:`, `playing …`, outcome rows, status responses) remain direct writes with their current contract; they are NOT rerouted through the logger; their failure is a host-operation failure (already classified in R1 §4). UI feedback and reliable diagnostics must not depend on best-effort log consumption. **Change:** PRESERVE. |
| **T5-8** Panic hook as a process-global facility | **Basis:** machine_input.rs:192-207 (global swap, chain-to-previous, never restored, machine-transport-only installer). **Target:** any future hook design names ONE process-level installer (the host), documents non-restoration, treats reentrancy (a panicking hook) as terminal, and is never per-episode replaceable. **Chain-to-previous is a CONDITION, not an unconditional duty (R1.2):** invoking an uncontrolled previous hook re-introduces exactly the synchronous-stderr hazard this module isolates, so the target constraints must be co-satisfiable — a future design must state WHICH threads/contexts chain to the previous hook and what the previous hook must satisfy (e.g. chain only outside the failure cone; or replace-with-record and invoke previous only from a non-critical context; or a documented acceptance of the residual with the affected threads enumerated). The current machine-transport hook keeps its chain (its threads are the installer's own); nothing here changes it this round. **The hook's stderr write is NOT cost-bounded by any platform guarantee:** it is an ungated synchronous sink running on whatever thread panics, before any catch — a full pipe blocks the panic translation itself (ledger #11, B F7). The hook therefore remains inside the logging firewall's hazard inventory; its isolation mechanism stays OPEN, but no future design may simultaneously claim critical-path isolation and unconditional chaining. **Change:** NEW-OBLIGATION on any future rework; PRESERVE current single-installer fact. |
| **T5-9** FFmpeg av_log callback | **Target:** stays a C-side process-global with per-`song_open` reinstall and the discard-by-default policy; if product logging later wants FFmpeg chatter, it goes through a dedicated owned bridge copy — never by restoring FFmpeg's default logger in production paths. **Change:** PRESERVE + forward constraint. |

---

## M6 — Migration and acceptance

Delivered in [error-system-migration-r1.md](error-system-migration-r1.md) (per-boundary
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
| R5 | Retry policy executed by a dedicated plugin; loop owns the extension point; durable first-wins retry bookkeeping; producer only classifies | **Adopt with adjustment (mechanisms), not portable (the plugin)** | Adopt the mechanism pieces for any FUTURE earned recovery: stable-code routing sets, backoff+jitter+caps, retry-after honor, abort-aware waits, first-wins durable bookkeeping. Not portable as a Recovery Plugin: Qianqian has no multi-provider policy-composition pressure; under D13 retry stays an owned effect of the operation's Plugin. All recovery remains NOT_EARNED today (R0 §3). |
| R6 | Logger facade per fiber-scoped name; structured record; filtering/format/transport all in the exporter; sinks as fiber effects; ring-buffer default exporter | **Adopt with adjustment** | Unusually portable because their logging is NOT a plugin — matches T5's host-owned facility: component-scoped facade, structured owned record, per-sink levels, formatting at the sink/drainer, sinks as owned effects. Hard adjustment: zero logging on the RT path and the failure cone (their JS costs do not transfer; T5-2 is stricter). |
| R7 | Five dispatch modes with per-mode throw contracts; containment added at call sites that must not let observers break ownership; listener lifetime = registering fiber | **Adopt with adjustment** | Not portable as an EventBus (none exists; none earned). Adopt the disciplines: every existing callback point declares its panic/error contract (drain observer: synchronous, panic loses only the mirror; gate events: current-wins attribution; analyst: contained), lifecycle notifications contained so one observer cannot break cleanup, registrations owned by the registering owner's lifetime (ordinary K0 effects). |

**Explicitly not imported:** Error Plugin / Recovery Plugin / Logging Plugin / EventBus
plugin identity; the retry-plugin topology; the tool-pipeline protocol machinery; JS-style
exception flow inside Rust.

---

## Owner decision points (each with options; nothing decided here)

| ID | Question | Options | Recommendation (non-binding) |
|---|---|---|---|
| D-2 | Probe diagnostic read (T1-2) | (a) adopt one-line read; (b) leave as recorded loss | (a) — cheapest confirmed-loss fix, no contract touch |
| D-3 | Cone write-failure isolation (T5-4) | (a) narrow corrective now (non-panicking writer on ledger rows #8/#9/#10 — **partial**: removes panic escalations, full-pipe blocking remains until the framework); (b) fold into future logging work; (c) leave as documented hazard | (a) — R0 §6 already anticipates "a separate, minimal corrective"; independent of framework choice; the residual blocking must be stated in the landing PR, not silently dropped |
| D-4 | DRAIN_CAP semantics (C-8) | (a) keep as OPEN resolver tuning; (b) amendment defining slow-drain evidence class | (a) until E-5 runtime data exists; any (b) is an AI-7-space authority act; E-5 supplies the datum, not the verdict |
| D-5 | Superseded-fatal witness (T3-1) | (a) adopt diagnostic witness — requires an ABI/SongCore mechanism amendment FIRST (currently unobservable across the ABI, R1.1); (b) accept silent supersession as ABI-coherent | (b) first — the latch belongs to the replaced continuation; (a) must price the ABI change, not assume a local read that does not exist |
| D-7 | Panic-in-teardown-inverse handling | (a) K0 authority review of catch→Violated at dispose boundary; (b) keep crash-propagation, document it | (b) first (honest, cheap), (a) only if owner wants process survivability |
| D-9 | "Concrete consumer" bar (C-6) | (a) presentation obligations count (name the surface, the failure mode it must explain, and why the local path cannot); (b) require programmatic consumers (diagnostic-only fields wait) | owner ruling; gates T1-3 applications incl. T2-5's exposure question. R1.1: the bar is obligation-based — consumer syntax (`format!` vs code) is not the discriminator |
| D-10 | Output reason snapshot carrier (T2-5) | (a') session-owned cell handed off as a `RenderRequest` field (audio-api-typed, the `level`/`position` pattern), read session-internally; (b) parameter on the `drain.complete`-adjacent delivery API | both carriers cross the backend-neutral port crate (R1.1 path drawing) — the genuine open sub-question is observation-surface exposure (T1-3/D-9), not "port vs no port"; migration §3.4's interpretation note applies either way |
| D-11 | Exit-code refinement | already #231 open question; unchanged | defer |
