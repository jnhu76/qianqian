# Error disposition contract — R0 design candidate (#231)

> **Truth class: DESIGN_CANDIDATE / NOT AUTHORITY / NOT IMPLEMENTED.**
> **Evidence anchor:** `main@524c6efeeabcfc94ceaf8e9cf1392e88861288c3` (2026-10-15).
> **Review status:** revised after independent Astra REVISE assessment (P1 + P2); repeat review pending; no owner adoption.
> **Scope:** #231 error provenance, stage-local handling, escalation and diagnostic requirements.
> **Explicitly out of scope:** #277 Failed auto-advance decision; changes to existing ADRs, public API, SongCore ABI, implementation, or tests.

This document is a candidate decision record, not a new source of semantics. Where a
sentence describes current behavior, its authority is the code or accepted ADR
linked below. Where it describes desired behavior, it is a **proposal requiring
owner review and, where it changes a frozen obligation, an authority amendment**.
Neither merging this page nor passing a document review authorizes implementation.

## 0. Authority, evidence and question

Use existing authority, do not recast it here:

- [PBK-001](../adr/ADR-PBK-001.md): Command / Fact / Projection and realtime firewall.
- [PBK-002](../adr/ADR-PBK-002.md): D11 terminal authority, D14.2 observation, D14.5 seek, D14.6 replacement, D14.7 pause, D14.11 processing, §20 U2 EOF policy.
- [PBK-003](../adr/ADR-PBK-003.md): stable Output Plugin and backend-neutral AudioOutput contract.
- [SongCore binding architecture](songcore-binding-architecture.md) and [C ABI](../../native/include/songcore.h): SongCore remains an independent media-to-PCM component.
- [DSP product model](dsp-product-model.md) and [playback execution model](playback-execution-model.md): processing and execution constraints.
- [#231 audit](https://github.com/jnhu76/qianqian/issues/231#issuecomment-6187764428) **together with its mandatory [erratum](https://github.com/jnhu76/qianqian/issues/231#issuecomment-6188419040)**; [#277](https://github.com/jnhu76/qianqian/issues/277) remains undecided.

The product-first question is **not** "How many error enums should survive the FFI?"
It is: who can prove a condition was handled, what makes continuing legal, which
unhandled failures affect another owner's contract, and which information must
remain available for diagnosis without becoming playback authority?

## 1. Four independent dimensions (candidate reasoning model)

A single scalar "music error" or `retryable: bool` cannot encode all four:

1. **Origin:** which operation/stage observed an anomaly (host read, demux,
   decode, seek, processing, output, session, App/host).
2. **Scope of invalidation:** which guarantee no longer holds (single
   operation, data fragment, current decoder continuation, episode, output
   mechanism, or ownership/lifecycle). Origin alone does **not** prove scope.
3. **Disposition:** handled locally, degraded under an *already authorized*
   contract, refused unchanged, escalated, or terminates under the existing
   authority. A disposition is **not** an additional D11 terminal variant.
4. **Diagnostic provenance:** typed codes and context sufficient for
   investigation. Diagnostics are neither recovery permission nor Facts.

**Decision rule (candidate):** the layer nearest the failure that **owns
the affected operation and can prove its continuation invariant** may handle
it locally. Otherwise it must report a truthful failure to its existing caller,
whose owner decides the next consequence. An unproven continuation must fail
closed. Success/error status, diagnostic severity and user-visible wording
must not be interchanged.


The analysis dimensions are **not mutually exclusive** and must not be
flattened into a universal disposition vocabulary: an operation can be
escalated to its caller, prevent episode establishment, and subsequently
encounter a teardown fail-stop. "Handled", "refused unchanged", "escalated",
"establishment failure", "episode failure" and "fail-stop" in this document
are ordinary descriptions at **different owners and times**, not a common
enum, ordering, transition graph, or proposed dispatcher. Existing
operation-specific typed contracts (notably `ProviderSeekOutcome`) remain
the only authorized representations. Never derive recovery permission from
a generic label.

## 2. Product-visible outcome candidates (not accepted #277 policy)

This is the **minimum candidate UX partition**; it intentionally does not
classify "bad media" from a generic decode failure.

| Product observation | Allowed behavior under *current* authority | Explicitly unearned behavior |
|---|---|---|
| Candidate cannot be opened/probed | Refuse Open, explain, preserve current live episode on preflight refusal | Invent Failed for a non-episode; silently move playlist cursor |
| Current episode suffers irreversible decode/read/seek failure | Commit truthful Failed; report; user may explicitly navigate | Automatically advance on Failed; label file corrupt from stage/message |
| Audio output aborts and current episode cannot continue | Existing D11 resolution; report device/output evidence | Turn a dead render leg into a fake Paused state; skip tracks to fix device |
| Pure operation refusal (e.g. proven pre-mutation seek rejection) | Continue prior valid playback; report command refusal | Turn refusal into an episode failure |
| Auxiliary artwork/lyrics/visualizer fails | Candidate: isolate optional feature, provided no audio/lifetime dependency was violated | Assume all present or future auxiliary failures are nonfatal |
| Irrecoverable processing or lifecycle defect | Existing processing failure / existing fail-stop rules | Unapproved DSP bypass; recovery after teardown violation |
| Normal EOF + required drain | Completed, existing App U2 Completed-only traversal | Treat Completed as file-integrity certification |

Actual user wording and controls are later UX decisions; the category does not
automatically authorize retry, seek, drop-packet, bypass, or next-track.


## 3. Stage-by-stage disposition matrix

**Status vocabulary in this table describes evidence, not a new product
state:** `CURRENT` means observed in production code or supported by the
accepted authority cited in the row; `NOT_EARNED` means a possible future
behavior requiring its own proof and, where necessary, authority amendment.
The invariant in each row is what a reviewer could **falsify** with a
counterexample. Source EOF may be normal under the current byte/decode
protocol without proving the underlying file was complete.

| Owner/stage and authority anchor | Status + conditions to continue (testable invariant) | Must escalate when | Diagnostic evidence, if actually captured |
|---|---|---|---|
| **Host byte I/O** — `qianqian-decode-songcore/src/lib.rs` `file_read/seek/size`; `songcore_ffmpeg.c` `io_read/io_seek` | **CURRENT:** successful read/seek preserves byte order/position; zero-byte read maps to source EOF under the existing API, **not** an integrity claim. **NOT_EARNED:** retry after I/O error; must prove unchanged cursor or exactly known progress, no duplicated/dropped bytes, bounded attempts, stop responsiveness | Read/seek/size error or unknown cursor/progress: report to SongCore as the current I/O contract permits; never silently convert read failure to EOF | Operation, original OS error if captured, validated offset, short-read/retry count (present callback currently loses OS detail) |
| **SongCore demux** — `songcore_ffmpeg.c` `decode_one_frame()`, `av_read_frame` branch | **CURRENT:** normal packet selection and FFmpeg internal progress; no generic corruption recovery. **NOT_EARNED:** resync; must prove correct stream identity and packet boundary, explicit treatment of lost media/time, consistent decoder continuation, bounded progress and stop observability | Demux cannot deliver a trustworthy continuation or returns non-EOF failure | Native FFmpeg code, stage, stream/packet and byte position **only if known** |
| **SongCore decoder / PCM conversion** — `decode_one_frame()`, `decode_status()`, `song_read_pcm()`; SongCore ABI | **CURRENT:** `EAGAIN` send/receive loop, valid PCM, format invariance, legal success/EOF, and latched fatal reported after already-produced frames. **NOT_EARNED:** drop malformed packet; requires authorized content-loss/gap semantics, sample/position and timestamp treatment, decoder state and stream identity proof, frame format, forward progress, and stop responsiveness. Later decodable frames alone do not prove legality | Decoder continuation invalid, conversion/format contradiction, unrecoverable native result; retain latched fatal for next read as today | Typed SongCore status, native code, produced frame count, whether fatal reporting was deferred |
| **Rust SongCore adapter** — `native/include/songcore.h` diagnostic lifetime; `ports.rs` `DecodedPcmStream`, D14.5 | **CURRENT:** `SONG_OK` → Frames; `SONG_EOF` → Eof; `ProviderSeekOutcome::RefusedUnchanged` only on proven pre-mutation refusal. Any diagnostic pointer is valid only **until the next SongCore call on the same handle**; bounded ownership transfer must finish inside that window | Unhandled status or invalid decoder continuation; destructive seek → existing failure path, no fake unchanged refusal | `song_status`; `song_error.native_code/message` **copied before the next same-handle call** if needed; no borrowed native pointer across async boundary |
| **Playback Session / decode worker / PcmEdge** — PBK-002 D11, D14.5, D14.11; `session.rs`, `completion.rs` | **CURRENT:** previously accepted frames and bounded edge behavior follow existing ordering; seek `RefusedUnchanged` permits old continuation; stop/EOF settlement follow frozen precedence. Invariant: no resumption of old PCM after destructive seek and no observer-dependent terminal creation | Endpoint read failure, destructive seek, processing failure or worker panic; publish mechanism evidence and let D11 owner settle | Actual worker origin and current episode-associated evidence; error text is presentation-only |
| **DSP processing** — PBK-002 D14.11; `session.rs` `processing.stage/poll_update`; `dsp-product-model.md` | **CURRENT:** successful staging and proven no-mutation validation rejection. **NOT_EARNED:** bypass on failed accepted processing; would need explicit signal/timeline, state-reset and lifecycle guarantees | Accepted processing fails and no authorized continuation exists | Applied stage/update, refusal vs failure, stage diagnostic; never rename a DSP failure as decode |
| **Output render / drain** — PBK-003; `wasapi.rs` `run_render_thread`; PBK-002 D11 | **CURRENT:** legal device/edge interaction, drain completion, stop-handling; `Aborted` alone is not device failure. **NOT_EARNED:** general device retry; needs stream/resource validity, correct PCM and position, bounded attempts, stop/teardown responsiveness | Output/stream cannot progress or correct drain cannot be established; publish stop/wake/drain evidence without waiting for log output | Backend and operation, HRESULT if captured, `LoopOutcome`, drain verdict, diagnostic text separate from the verdict |
| **Output volume control** — PBK-002 **D14.9**, `wasapi.rs` `SetAllVolumes` branch | **CURRENT:** ordinary non-device-invalidating apply failure retains **last successfully applied level**; remembers the attempted routed value to avoid repeating a failed COM call each iteration, and tries on a later routed value change. No episode failure from the ordinary apply error. Invariant: no invented "applied" readback or repeated failed application per quantum | **CURRENT:** `AUDCLNT_E_DEVICE_INVALIDATED` aborts through the existing device-failure path. No automatic recovery of dead render | Desired/routed versus last successfully applied (not a fabricated device readback), HRESULT, one diagnostic *attempt* per episode; see existing synchronous logging hazard §6 |
| **Playback Session completion** — PBK-002 D11/D14.2; `completion.rs` `resolve()`, `handle.rs` | **CURRENT:** at most one committed terminal outcome; resolve using all decisive worker/drain/stop-intent evidence, by semantic precedence. Pending observations do not create facts | Decisive evidence requires authoritative terminal commit; cannot defer it for a logging sink | Terminal result, origin diagnostic and decisive inputs where legally observable |
| **App / playlist / TUI / host** — PBK-002 §20 U2, D14.6; `player.rs` `replace_episode/poll_eof_policy` | **CURRENT:** probe-before-destruction refusal preserves old episode; `Completed` only triggers at most one auto-next attempt, committing cursor only on successful Open; explicit manual navigation remains available. Invariant: failed activation after old teardown is **not** equivalent to preflight refusal | Teardown violation → existing fail-stop; host error remains host failure; Failed is not an App-manufactured Completed | Open phase/result, old/new episode status only as supported, candidate and cursor commit, diagnostics without policy string parsing |

### NOT_EARNED recovery admissions (no experiment claimed in R0)

| Candidate | Minimum independently falsifiable continuation proof before approval |
|---|---|
| **Host retry** | Exact bytes/cursor before and after failure; no duplicate or lost bytes; finite tries; stop can interrupt/win; read error never silently treated as natural EOF |
| **Demux resynchronization** | Real next packet/stream boundary is identified; any skipped bytes/data are accounted for; decoder state is valid at the new point; downstream sample/timeline expectations are explicitly satisfied; finite progress and stop responsiveness |
| **Decode bad-packet skip** | Explicit authority for content loss and any silence/gap/position discontinuity; correct source format; no untrusted PCM passed through; decoder reset/continuation validity; finite forward progress and cancellation; prove on corrupt and healthy controls |
| **DSP bypass** | Authorized bypass semantics; correct post-processing PCM, signal history and timeline; valid publication/lifetime and teardown; no covert reclassification of failure |
| **Device restart/retry** | Fresh/valid device ownership and host resources; no old-stream use-after-release; correct PCM positioning/drain semantics; bounded response to stop and teardown |

These proofs are **preconditions**, not implementations or evidence of
capability. The present R0 review is allowed to leave every NOT_EARNED item
unimplemented, untested, and forbidden.

### Why these are not interchangeable

- **Normal control flow is not an error.** SongCore/FFmpeg EAGAIN during the
  send/receive protocol is already consumed by `decode_one_frame()` and should
  not be escalated or logged once per occurrence as a fault.
- **Decoded frames then deferred fatal is not recovery.** `song_read_pcm()`
  can publish produced PCM on this call and latch `fatal_error` /
  `last_native` for a later read. Do not discard valid delivered PCM or
  silently discard the latched error; the ABI's successful-call diagnostic
  clearing is contractual.
- **Seek already demonstrates the standard.** The current
  `ProviderSeekOutcome::{Applied, RefusedUnchanged, MutatedThenFailed}`
  classifies *whether old continuation is proven safe*, not merely the status
  name. Do not naively copy that enum into read/open/render APIs.
- **Aborted is not automatically "device failure".** The WASAPI
  `LoopOutcome::Aborted` can include stop or edge termination; Session
  uses stop intent and worker/drain evidence to distinguish outcomes.
- **Playback end is not an integrity oracle.** #231 observed a truncated MP3
  settling Completed. Neither Failed nor Completed proves the source file is
  corrupt/complete; an integrity contract would be separately earned.
- **Auxiliary failure isolation is a product candidate**, not a license
  to conceal a failure that broke an audio, ownership or cleanup invariant.

## 4. Mapping to today's code (facts versus candidate changes)

| Actual seam at BASE_SHA | Existing behavior / contractual fact | Consequence for R0 review |
|---|---|---|
| `native/src/songcore_ffmpeg.c`: `decode_status()` | EIO -> SONG_ERR_IO (103); ENOMEM -> OUT_OF_MEMORY (112); other errors -> generic DECODE_ERROR (108) | Native enum alone cannot prove damaged media; CORRUPT_DATA (107) is not produced in the decode loop |
| `native/src/songcore_ffmpeg.c`: `song_read_pcm()` | Partial frames may be returned before a latched fatal is reported on the next read; `ok(h)` clears diagnostic under ABI contract | **Do not "repair" this by deleting ok(h)**; investigate separate original-message retention only if earned |
| `crates/qianqian-decode-songcore/src/lib.rs`: `file_read/seek/size` | Rust I/O errors become -1; SongCore sees generic EIO | Loss of OS specificity is a bounded diagnostic candidate, not proof of media corruption |
| `crates/qianqian-decode-songcore/src/lib.rs`: `read_frames()`, `last_error()` | Non-success codes become `DecodeError { message }`; `native_code` not read | #231 L1/L2 candidate internal retention; adding fields alone **does not** earn App policy input |
| `crates/qianqian-audio-api/src/ports.rs`: `DecodeError` / `DecodeOpenError` | Public domain-facing errors contain messages only | Widen only for a concrete diagnostic or control consumer, evidence that local reporting is insufficient, and the minimum backend-neutral contract; a diagnostic-only change **must not** change control behavior |
| `crates/qianqian-playback/src/session.rs` and `completion.rs` | Decode/seek/processing evidence drives existing D11 resolver; no playlist awareness | Keep semantic precedence, first committed Fact, worker-exit and teardown ordering |
| `crates/qianqian-output-wasapi/src/wasapi.rs` | `LoopOutcome::Aborted { message }` is optionally printed **synchronously before** `render_input.stop()` and `drain.complete()`; volume-apply recoverable failure also has a synchronous `eprintln!` in the render loop | **Existing progress hazard**, not a new bug from this PR: a blocked stderr can delay wakeup, drain publication or render-loop progress. D11 semantics unchanged; §6 records the gap. |
| `crates/qianqian-playback/src/handle.rs` | Public terminal enum has only Completed/Stopped/Failed; `failure_diagnostic` is presentation-only | Do not branch on message or add Failed subtypes |
| `apps/headless/src/player.rs`: `poll_eof_policy()` | Only committed Completed drives auto-traversal; one attempt per episode, commit cursor on successful Open | #277 must independently amend §20 U2 before any Failed-driven transition |

**#231 erratum classifications, not all actionable bugs:**
L1/L2/L3/L7 = 4 confirmed information losses;
L4/L9/L11 = contractual abstractions;
L13 = contractual abstraction + no integrity authority;
L5/L8 = contractual abstraction + potential gaps;
L15 = potential gap;
L6/L10/L12/L14 = no defect (L14 has an accidental-string-signal note).
The 15 ledger entries are **not 15 implementation tickets**.

**Unsettled FLAC attribution:** the #231 erratum also records a
STREAMINFO-MD5-vs-decoded-samples signal suggesting a decoder-specific
difference. Its methodology and authority are still pending independent
adjudication. A FFmpeg 7.0.1 CLI comparison is not a like-for-like control
against Qianqian's pinned 9.0.1 closure. Do not label this file definitively
corrupt, definitively healthy, or the decoder definitively defective.


## 5. Information-preservation and collapse rules (candidate)

A shared error *result* may be smaller than the data available for diagnosis.
The justification is specific to the next owner, never the convenience of an
`Error` super-enum.

1. **Who consumes which distinction?** Identify the concrete diagnostic
   **or** control consumer and obligation; show why existing local
   diagnosis/observation is insufficient.
2. **What does the producer actually prove?** A generic
   `DECODE_ERROR` is not a media-corruption witness, nor is EIO proof
   that one file is the only affected resource.
3. **Can two failures share a caller result?** Yes if the caller has the
   same obligations; preserve only those independent distinctions that
   could alter those obligations. Diagnostic provenance may still be
   different and local.
4. **Is a wider public contract necessary?** Require the smallest
   backend-neutral fields and a real consumer. A **diagnostic-only**
   extension must demonstrate it does **not** change control semantics
   or act as implicit recovery permission. A **behavior-changing**
   extension requires separate owner/ADR authorization.
5. **Which distinctions must not collapse?** Seek
   `RefusedUnchanged` versus `MutatedThenFailed`; normal EOF versus
   unhandled read failure (while accepting EOF proves no integrity);
   stop-intent + worker/drain evidence versus an isolated Aborted;
   preflight Open refusal versus post-destruction activation failure
   versus teardown violation; accepted PCM with **pending fatal**
   versus genuinely recovered error.
6. **What does the FFI own?** `song_error` and its borrowed
   `message` are valid only **until the next SongCore call on the same
   handle**, not until the handle is destroyed. If needed, copy the
   bounded diagnostic while valid, then pass **owned** data to an
   asynchronous diagnostic consumer. If the copy/report fails, the
   original operation's typed success/failure still follows its
   contract; losing a log is permissible, losing the failed operation
   is not.

Keep mechanism errors scoped to their proper owner. Do not route all
FFmpeg/OS enums into Playback Session or UI; do not flatten
control-critical data into localized text. SongCore C ABI changes have
not been earned; local adapter retention from #231 remains a
candidate, not an implementation approval.


## 6. Diagnostic contract for future logging (candidate, no logger yet)

**No new logging mechanism is implemented or authorized here.** Logging
is evidence, not a Fact, a recovery command, or an owner of termination.
When actually captured, a local record might include component, operation
and stage; typed source status/native code; known offset/produced-frame
count/delayed-fatal context; existing episode context if necessary; and
an unstable message. There is no requirement to invent an Episode ID.

### Proposed critical-path firewall — beyond the steady audio quantum

The **whole failure publication and exit cone** is protected, not just
the per-PCM-block render loop:

- Never let a logging sink's latency/success influence the publication
  of failure evidence, `render_input.stop()`/wakeups, drain completion,
  D11 settlement, or resource release and thread/episode teardown.
- No synchronous sink under the settlement lock or while holding a
  resource needed for an owner to make terminal progress. An unavailable,
  backpressured, full or broken diagnostic consumer must not block
  producer publication or episode shutdown; the caller may **drop the
  diagnostic**, never the actual failure signal.
- Limit producer work and diagnostic payloads at the actual boundary:
  any optional snapshot/copy or handoff must be bounded/nonblocking;
  expensive formatting, disk I/O, network delivery and other blocking
  sink operations are outside the RT/failure-publication/teardown path.
  A log consumer is not an owner that must be joined or awaited before
  the required terminal evidence/discharge.
- **"Only one log per episode" bounds count, not output latency.**
  A single `eprintln!` to a stalled pipe can block indefinitely.
  Logging off, dropped or delayed cannot alter behavior or introduce
  another completion barrier. No wall-time bound is asserted by this
  design without an executable witness.
- Source/FFI borrowed diagnostic pointers are never handed to any
  asynchronous consumer. If useful, materialize an **owned, bounded**
  snapshot before the next SongCore operation on the same handle; if
  this optional capture fails, preserve the operation/terminal result
  regardless. Do not change `song_read_pcm()`'s `ok(h)` diagnostic
  clearing or latched-fatal reporting to accommodate logging.

**Known BASE_SHA progress hazards (NOT FIXED by this docs-only PR):**

1. `crates/qianqian-output-wasapi/src/wasapi.rs:257–273`
   (at frozen `524c6efe`) calls `eprintln!` for
   `LoopOutcome::Aborted` **before**
   `render_input.stop()` and `drain.complete()`.
   With `QIANQIAN_AUDIO_LOG` enabled, a blocked stderr can delay
   the producer wakeup, drain evidence and render-thread exit. This
   is an **existing code/progress risk**, not an assertion that
   the error class or D11 resolver misclassifies the episode.
2. The recoverable `SetAllVolumes` apply-error branch around
   `wasapi.rs:674–685` also issues a synchronous `eprintln!`
   from the render loop, despite the one-per-episode guard. The
   output volume itself holds the last successful level, but a stalled
   diagnostic sink can delay the next render iteration.

Before treating any future logging implementation as satisfying this
candidate firewall, explicitly demonstrate **negative controls**
(stderr sink blocked, consumer stopped, queue full and write failure)
while verifying stop/wake, drain/settlement, render progress and
teardown are independent. No such experiment is claimed here; a
separate, minimal corrective may be needed for today's opt-in logging.
Do not turn this paragraph into authorization for a broad logging
refactor.

Default diagnostics must avoid unnecessary private filenames,
absolute file paths and metadata; fuller exports require a separate
product/privacy decision. Do not record each normal FFmpeg EAGAIN,
every PCM block, or every repeated failure. Severity is independent
of disposition; even high-severity logging is best-effort. Do not
create a Logging Plugin, EventBus, K0 capability, new error registry,
or independently managed lifecycle without separate D13 evidence.

## 7. Adversarial examples / counterexample obligations

| Example | Expected reasoning | Unsafe shortcut |
|---|---|---|
| FFmpeg EAGAIN during receive/send | Local protocol progress, not a product error | Log/propagate as decode failure |
| One malformed packet, later frames potentially decodable | Continuation proof required before any skip-packet design | Treat codec error code as permission to skip |
| Affected FLAC with SongCore error but other decoder succeeds | Ambiguous; gather same-version/environment evidence | Call file corrupt or decoder defective from a single observer |
| Produced >0 PCM and latched fatal error | Return those frames under current ABI; later read reports fatal | Treat successful read as recovery or erase fatal |
| Destructive seek failure | D14.5 MutatedThenFailed, cannot resume old cursor | Interpret generic seek status as unchanged refusal |
| WASAPI stop-intent versus device failure | Resolver must use intent/worker/drain evidence | Classify every Aborted as device failure |
| MP3 truncation returned EOF | Existing Completed reports observed decode/drain; file integrity unknown | Manufacture a new D11 incomplete-file terminal |
| Candidate Open refused | No episode, cursor unchanged as required | Create D11 Failed or advance silently |
| Failed episode with a healthy next song | Manual navigation permitted; U2 still Completed-only | Auto-skip before #277 owner approval |
| Several consecutive failures / Repeat One | Future policy needs explicit boundedness and traversal proof | Unbounded automatic failure cascade |
| WASAPI abort while stderr is blocked | Producer wakeup and drain completion cannot depend on diagnostic sink availability; existing baseline has risk | "Only one error log" means bounded execution time |
| Recoverable output-volume COM apply error | D14.9 holds last successful level; device invalidation escalates; blocked stderr must not stall rendering | Convert every apply failure to Failed or assume one-time print cannot block |
| SongCore diagnostic borrowed across an async task | Copy bounded, owned evidence **before next same-handle SongCore call**, never wait for sink | Assume pointer survives as long as handle exists |

## 8. Decision gates and independent-review questions

### Gate R0 — design-only, submitted with this draft PR

- [ ] Independent reviewer confirms this is non-normative and cannot be
      mistaken for an ADR amendment or implementation authorization.
- [ ] Every current-behavior claim maps to code or accepted authority at
      the stated baseline, without treating diagnostic text as semantics.
- [ ] For each stage, confirm a CURRENT/NOT_EARNED status, authority anchor, and falsifiable continuation invariant; do not bless candidate recovery by analogy.
- [ ] Challenge whether a smaller (or no) shared error taxonomy suffices.
- [ ] Verify no new Plugin, EventBus, terminal variant, cross-platform
      backend leakage or extra episode identity is being proposed.
- [ ] Verify logging remains nonblocking throughout evidence publication, stop/wakeup, drain, settlement, and teardown, **not only** the audio quantum; audit the existing WASAPI stderr hazards.
- [ ] Verify SongCore borrowed diagnostics expire at the **next same-handle call** and async consumers can only receive owned data.
- [ ] Verify #277 owner policy is not silently resolved.

### Later gates — NOT performed or authorized by this PR

**R1 (mapping):** reconcile each #231 L1–L15 with these disposition
requirements. For each entry: current producer/caller, semantic consequence,
diagnostic loss, proof, and whether any actual consumer requires a field.
Explicitly allow justified abstraction/no-change.

**R2 (targeted evidence):** compare pinned FFmpeg 9.0.1 and independent
reference under as close to identical build/options/input as feasible;
reconcile STREAMINFO MD5 claim; deterministic source/read-failure probes;
positive/negative witnesses for SongCore status/native_code and deferred
fatal; test actual D11 ingress where needed. Mark unrun cells NOT_RUN.

**R3 (possible implementation decision):** choose the smallest earned
adapter/diagnostic correction if needed, preferring local retention.
Public Decode Port or observation expansion requires **a concrete
diagnostic or control consumer**, evidence that existing local routes
cannot serve it, and a minimum backend-neutral contract. Diagnostic-only
changes must be proven control-inert. Changes to recovery, continuation
or product behavior separately require the appropriate authority and
ADR review. Neither class of change is approved in R0.

**R4 (#277 separate):** owner decides whether Failed-driven App traversal
exists at all. Until explicitly amended, PBK-002 §20 U2's Completed-only
policy, one-attempt semantics, Open-commit discipline and no skip cascade
remain fully binding.

### Specific questions for Astra

1. Does this document improperly introduce a second authority, terminal
   fact, or taxonomy, despite its design-candidate label?
2. Which stage-local recovery condition lacks a testable continuation
   invariant? In particular, could "normal source EOF" hide truncation?
3. Are SongCore's independent component boundary and Qianqian product
   authority maintained?
4. Is diagnostic retention in the Rust adapter meaningful **without**
   requiring a new public DecodeError representation? What would consume it?
5. Does the WASAPI Aborted/stop-intent distinction survive the matrix?
6. Is the FFmpeg 7.0.1 vs pinned 9.0.1 and STREAMINFO MD5 uncertainty
   fairly represented?
7. Could "optional auxiliary failure" conflict with DSP or realtime
   lifecycle guarantees?
8. Which lines should be **deleted**, not turned into new Rust code?
9. If #277 remains conservative, is any new architecture or implementation
   actually necessary for correct playback today?
10. Do the known WASAPI stderr calls obstruct stop/drain/exit under blocked
    sink, even on the one-shot abort path? Is any suggested solution narrower
    than a logging subsystem?
11. Does each NOT_EARNED continuation claim define **content loss, position,
    decoder state, bounded progress and stop response** where relevant?
12. Can a diagnostic-only Port extension be justified by a real consumer
    without inventing a semantics-changing use case?

## R0 disposition

~~~text
STATUS = DESIGN_CANDIDATE / NOT_AUTHORITY
BASE_SHA = 524c6efeeabcfc94ceaf8e9cf1392e88861288c3
ISSUE_231 = RESEARCH_INPUT; NOT_CLOSED_BY_THIS_PR
ISSUE_277 = UNCHANGED / NO_FAILED_AUTO_ADVANCE
D11 = UNCHANGED
SONGCORE_ABI = UNCHANGED
RUST_PUBLIC_API = UNCHANGED
LOGGING = CONCEPTUAL_ONLY; BASELINE_STDERR_PROGRESS_HAZARD = UNRESOLVED
PRODUCTION = UNCHANGED
TESTS = UNCHANGED
OWNER_ADOPTION = PENDING
ASTRA_REVIEW = REVISE_AT_3c866cfa; RECHECK_REVISED_HEAD
~~~
