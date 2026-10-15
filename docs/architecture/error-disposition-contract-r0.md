# Error disposition contract — R0 design candidate (#231)

> **Truth class: DESIGN_CANDIDATE / NOT AUTHORITY / NOT IMPLEMENTED.**
> **Evidence anchor:** `main@524c6efeeabcfc94ceaf8e9cf1392e88861288c3` (2026-10-15).
> **Review status:** proposed for an independent Astra review; no owner adoption.
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

We use the following names *only as review annotations*, not proposed Rust
types or new runtime states:

| Annotation | Meaning | Permission boundary |
|---|---|---|
| HANDLED | Internal protocol condition or anomaly fully resolved without changing the promised result | Local owner proves invariants; no new Episode outcome |
| DEGRADED_CONTINUE | Reduced service with an explicit product/operation contract | **Candidate only** unless that degradation is already authorized |
| REFUSED_UNCHANGED | Operation rejected before the relevant prior state changes | Must prove unchanged continuation; not a generic fallback |
| ESCALATED | Callee cannot discharge caller's required guarantee | Report sufficient, accurate operation evidence; caller adjudicates |
| ESTABLISHMENT_FAILURE | No live episode was established | Never forge D11 Failed |
| EPISODE_FAILURE | Live episode cannot satisfy current playback contract | Playback Session alone commits D11 Fact under existing precedence |
| FAIL_STOP | Existing lifecycle/teardown guarantee violated | Respect existing latch; no attempt to disguise recovery |

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

Every "may continue" entry means **only if the specified proof obligation
holds**. It is not a declaration that present code implements that recovery.

| Owner / stage | Local handling and legal continuation | Mandatory escalation | Diagnostic evidence worth retaining |
|---|---|---|---|
| **Host source / byte I/O** (source provider) | Successful read/seek; normal source EOF according to its existing contract. A transient retry only if bounded and without changing cursor/data ordering, and explicitly supported | Persistent read/seek/size failure, unknowable cursor/data validity | Operation, OS error/code if available, safe byte offset, retry/short-read facts |
| **SongCore demux / container** | Normal internal demux protocol; successful resynchronization only if packet boundary, stream identity and continuation are actually proven | Cannot establish valid stream, required packet boundary, or continued demux | Container, stage, stream index, native code, known packet/byte location |
| **SongCore decoder / PCM conversion** | Internal EAGAIN send/receive coordination; decoded frames with correct format/layout; localized bad-packet recovery **only if separately proven and authorized** | Unusable decoder continuation, untrusted PCM, unsupported midstream format change, non-recoverable decode result | Codec, operation, native code, returned status, produced-frame count, delayed-fatal evidence |
| **Rust SongCore adapter** (FFI translation) | Exact SONG_OK / SONG_EOF; map the provider's **already proven** seek dispositions; copy diagnostics while handle still owns them | An unhandled native status or a destructive/unknown continuation | Typed song_status; native_code/message when available; original operation; mapped result |
| **Playback Session decode worker / PcmEdge** | Existing frames, bounded edge backpressure, stop and EOF obligations; D14.5 RefusedUnchanged remains nonterminal | Endpoint read error, destructive seek failure, processing failure, worker panic; publish existing failure evidence and settle under D11 | Existing worker origin, operation and observation snapshot; avoid fake media-local attribution |
| **DSP processing** (episode-owned stage) | Validated/rejected configuration with no illegal mutation; completed processing; any bypass **requires distinct authority/evidence first** | An accepted processing operation fails without a proven authorized continuation | Processor stage, update/apply context, rejection vs fatal distinction |
| **Output backend / render leg** (Output Plugin mechanism) | Legal device protocol and normal drain; retry only if bound, safe and established for this backend | Device/stream failure, terminal aborted render, failed mandatory tail observation; release/wake waiters per existing contracts | Concrete backend/operation, OS/HRESULT when captured, LoopOutcome and drain evidence |
| **Playback Session completion** (D11 authority) | Nonterminal command dispositions remain commands; settle exactly as currently authorized | Publish exactly one terminal Fact when decisive evidence exists; no observer-dependent commit | Fact, competing evidence relevant to precedence, existing stage diagnostic |
| **App / playlist / TUI / host** (product policy) | Safe Open refusal, normal Completed-only auto-next, manual user navigation, truthful presentation | Existing fail-stop for teardown; host failure stays host failure; do not invent playback Fact | User action and Open result, optional terminal diagnostic, navigation attempt/result |

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
| `crates/qianqian-audio-api/src/ports.rs`: `DecodeError` / `DecodeOpenError` | Public domain-facing errors contain messages only | Widen the contract only for a proven consumer and backend-neutral semantic need |
| `crates/qianqian-playback/src/session.rs` and `completion.rs` | Decode/seek/processing evidence drives existing D11 resolver; no playlist awareness | Keep semantic precedence, first committed Fact, worker-exit and teardown ordering |
| `crates/qianqian-output-wasapi/src/wasapi.rs` | `LoopOutcome::Aborted { message }` is optionally printed, then mapped to drain Aborted without message | Diagnostics may be lost (L7); not permission to relabel playback outcome |
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

When designing a boundary, answer in order:

1. **Which next-owner obligation would become impossible if the field
   disappeared?** If none, preserve it locally for diagnosis if useful;
   do not expand a general public API by symmetry.
2. **Does the source evidence actually prove that category?** No
   `generic DECODE_ERROR => media-corrupt` or `EIO => this file is bad`
   leap. Native FFmpeg codes can be ambiguous across stages.
3. **Can two errors share a control result?** Yes, if they impose the
   same caller-visible obligations **and** no needed discriminator is
   destroyed. They may still have different local diagnostics.
4. **Is a new type/field needed beyond the boundary?** Only if a
   concrete consumer and false-merge counterexample earn it.
5. **Does any new recovery require ADR authority?** Do not smuggle
   changed semantics through a diagnostic code path.

The three paths remain separate in responsibility, **not necessarily three
implemented channels**:

~~~text
Local stage -- handled --> existing success/operation result
           -- unhandled --> existing typed operation failure --> owner decision
           -- diagnostic --> best-effort observation/logging (never an authority)
~~~

Do not route all FFmpeg/OS enums into Playback Session or UI, and do not
flatten control-critical distinctions into localized strings. SongCore C ABI
changes are **not earned** by current evidence; internal adapter retention
(option B in #231) remains a targeted candidate, not a selected PR implementation.

## 6. Diagnostic contract for future logging (candidate, no logger yet)

Future logs are **evidence**, not Facts and not recovery commands.
A useful local record may contain, only where known:

- component/operation/stage; typed source error status and optional native
  error code; whether error was handled, refused or escalated;
- caller-safe context (e.g. packet/byte position, produced PCM frame count,
  delayed-fatal occurrence); linkage to the **existing** episode record if
  necessary (no new Generation/Epoch identity);
- an unstable human-readable diagnostic separate from all control decisions.

Logging constraints to retain from the outset:

- Best effort: disabled, dropped, failed or reworded logs **never** change
  operation results, D11 settlement, data-plane behavior, or teardown.
- No file/network I/O, unbounded queues/locks, repeated formatting or
  expensive allocations on the WASAPI render quantum / realtime hot path.
  Avoid logging every normal FFmpeg EAGAIN or every PCM block.
- Prefer one bounded failure summary and optional aggregate counts to an
  unbounded stream of repetitive faults; a queue's drop count is diagnostic
  only. No latency or memory bound is asserted without measurement.
- Treat file paths, filenames, directory locations and media tags as potentially
  sensitive: minimize defaults; any fuller user-requested diagnostic export
  must be a separate product/privacy choice.
- Do not make a new Logging Plugin/Capability/EventBus. Any eventual mechanism
  must first demonstrate a consumer, performance envelope, ownership and
  need under PBK-002 D13 and the existing realtime firewall.

**Severity != disposition.** An error log does not require stopping
playback, and a fail-stop does not depend on successful logging.

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

## 8. Decision gates and independent-review questions

### Gate R0 — design-only, submitted with this draft PR

- [ ] Independent reviewer confirms this is non-normative and cannot be
      mistaken for an ADR amendment or implementation authorization.
- [ ] Every current-behavior claim maps to code or accepted authority at
      the stated baseline, without treating diagnostic text as semantics.
- [ ] Review stage-local continuation proof versus failure escalation.
- [ ] Challenge whether a smaller (or no) shared error taxonomy suffices.
- [ ] Verify no new Plugin, EventBus, terminal variant, cross-platform
      backend leakage or extra episode identity is being proposed.
- [ ] Verify logging remains bounded/best-effort and outside correctness.
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
adapter/diagnostic correction if needed; no public Decode Port or observation
expansion until a concrete semantics-changing consumer and authority approve
it. Any changed handling/recovery requires separate evidence and ADR review.

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

## R0 disposition

~~~text
STATUS = DESIGN_CANDIDATE / NOT_AUTHORITY
BASE_SHA = 524c6efeeabcfc94ceaf8e9cf1392e88861288c3
ISSUE_231 = RESEARCH_INPUT; NOT_CLOSED_BY_THIS_PR
ISSUE_277 = UNCHANGED / NO_FAILED_AUTO_ADVANCE
D11 = UNCHANGED
SONGCORE_ABI = UNCHANGED
RUST_PUBLIC_API = UNCHANGED
LOGGING = CONCEPTUAL_ONLY
PRODUCTION = UNCHANGED
TESTS = UNCHANGED
OWNER_ADOPTION = PENDING
~~~
