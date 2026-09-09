# Component Boundary Audit A0

> **STATUS: HISTORICAL / EXPERIMENTAL EVIDENCE**
>
> This document describes an earlier concrete Playback decomposition. It is **no longer current ARCH-004 authority**. Current Playback Foundations authority is `docs/adr/ADR-PBK-001.md` (ACCEPTED). Generic composition provenance and behavioral evidence remain usable; playback-specific ownership/granularity conclusions below are historical.

Design-gate evidence for **#53 COMPONENT-BOUNDARY-A0** (parent authority **#46 PLAYER-PLUGIN-ARCH-1**).

Status: **design audit, not implementation authorization.** Nothing here freezes a Rust API, a crate layout, or a dynamic-loading mechanism. Logical component boundaries are not crate/shared-lib/dynamic-lib boundaries.

BASE audited: `dae8dba` on clean `main`. Workspace tests green at audit time (2 passed).

Revision 2 (Corrective-1, 2026-09-06): applies the four corrections from human review round 1 on PR #66 — composition confluence split from domain continuity (§H); explicit ownership/lifetime for the RT data edge (§B.1, §B.3, §C, §E); CLAIMED protocol point-of-no-return distinguished from the physical emission boundary (§B.3, §F); Music component vs MusicKernel ownership distinction (§B.1, §I).

Revision 3 (Corrective-2, 2026-09-07): purifies §H.a per human review round 2 — frozen classifier for composition-owned vs domain-session truth; open Decoder handle / current source / playback state / recovery removed from §H.a; SinkSession + device session frozen as composition-owned via the bind-at-activation rule; clean-baseline definition forbids pre-loading domain state.

---

## Playback authority transition note (2026-09)

> **This document is historical evidence only. Its playback-specific ownership/granularity conclusions are not fresh implementation guidance, and no playback model described here — this document's #53 position or any successor — is current authority. Playback architecture has since been reopened from first principles; `ADR-PBK-001` is ACCEPTED and deliberately does not freeze either playback model.**

Still valid here (generic + behavioral evidence):

```text
FFmpeg dependency-closure evidence (#48)
PCM / realtime-path behavior evidence (submitted != rendered, RT island,
  commit/flush protocol, provider withdrawal ordering)
system-boundary / effect-classification reasoning
composition-kernel design provenance (#53 -> #67 -> #71 chain)
```

Playback architecture has since been reopened (2026-09 reset): neither the #53 position below nor the formerly accepted `MusicKernel`/`TransportKernel` split is current authority. Old playback conclusions — including this document's historical statement that a later accepted model superseded the #53 position — are historical records only; do **not** combine or extend either playback model when implementing new code.

---

## A. Reality audit

### A.1 Current repository facts

```text
BASE: dae8dba (main, clean)

crates/qianqian-core
  base.rs          empty placeholder (no kernel concepts yet)
  music.rs         MusicKernel with PlaybackState {Idle,Ready,Playing,Paused,Ended}; state only
  ports.rs         three EMPTY capability-port traits: Decoder, Processing, AudioOutput
                   (authorized capability names + dependency direction only)
  presentation.rs  PlayerView/PlayerAction, PlaybackState -> PlayerView mapping
crates/qianqian-runtime
  AppRuntime       constructor-only composition: MusicKernel + Option<Box<dyn AudioOutput>>
apps/headless      prints and constructs an empty runtime
```

`AppRuntime::new()`, `with_audio_output()`, and the direct `audio_output()` accessor are R0 bootstrap witnesses, not compatibility contracts.

### A.2 Historical evidence actually used (tag `playback-reference-v1`, frozen)

| Evidence | Fact used in this audit |
|---|---|
| `native/include/songcore.h` | SongCore ABI v1: one local file -> identity/metadata/artwork/stream info/source-rate Float32 interleaved PCM/seek/EOF; host-IO callbacks (`SongSource`); `SONG_ERR_STREAM_CHANGE` fail-closed; no FFmpeg type crosses the header; handles not internally thread-safe, distinct handles independent |
| `native/include/player_engine.h` | PlayerEngine C ABI: managed decode worker, polled coherent snapshot (media-time µs, `PE_QUALITY_CONFIRMED/ESTIMATED`), `submitted != rendered`, GAP has zero media duration, fail-closed seek, `pe_destroy` documented teardown order (stop publications -> join worker -> quiesce audio path -> close SongCore handle), open/stop recovery semantics |
| `native/src/player/commit_flush_handshake.hpp` | The #40/#42 commit-flush protocol: single-flight REQUESTED/CLAIMED/COMPLETED/CANCELLED; I3 (cancel-before-claim = never began), I4 (claimed can never be cancelled — no faked rollback of a possibly-started physical flush), I5 (no cross-request ACK/ABA) |
| `native/src/player/wasapi_renderer.hpp` | One backend-owned event-driven render thread over a mutex-free engine seam (`fill_output`/`advance_render` with padding-proven playout); renderer created after engine and destroyed BEFORE it; device failure degrades to bounded-retry with audible-position freezing as the honest "no output" signal |
| `native/src/player/pcm_ring.hpp`, `playback_timeline.hpp`, `wasapi_submit_accounting.hpp` | Engine-owned ring and single-owner timeline accounting (submitted vs rendered, device time vs media time) |
| `native/src/player/null_audio_backend.*` | Headless playback correctness without a real device is a proven mode |
| `native/ffmpeg/profiles/*.json`, `capabilities/*.json` | One FFmpeg dependency-closure authority with build profiles (`codec-base`, `songcore-test`); codec coverage is provider configuration, not a runtime layer |
| `apps/desktop/**` (Kotlin) | Proven UI seam: `PlayerPort`/`PlayerSnapshot` polled 5–10 Hz, `NativePlayerAdapter`, `FilePicker` living in the host app |

Project memory corroboration: the timeline-ownership fix (splitting timeline accounting across submit/render sides was a real bug source) and the RT-island corrective (zero mutex on the render hot path) both argue for single-owner timeline + RT data edges established outside the audio path (binding ownership frozen in §B.3).

Historical implementations are evidence of *behavior and ownership truth*, not of the future component layout.

---

## B. Component boundary matrix (proposed components)

Three runtime components are proposed for the first Windows slice. Full matrix for each; every other candidate from the issue is dispositioned in §J.

### B.1 Music (player domain-kernel component)

| Field | Analysis |
|---|---|
| Component | `Music` — one runtime component. The component is the composition unit; internally it is **MusicKernel + cohesive MVP playback mechanisms** (ownership split below). |
| Owns | Playback state machine (EMPTY/READY/PLAYING/PAUSED/ENDED/ERROR); media-timeline truth (position/duration µs, CONFIRMED/ESTIMATED landing, GAP = zero media time); the active track session (including the open Decoder handle); decode worker thread; PCM ring; the RT-safe publication boundary (commit/flush); stop-recovery reopen policy. Future queue semantics stay here (domain kernel), not in a new component. **Ownership split (frozen):** the *Music component* owns the domain semantics **plus** these cohesive MVP playback mechanisms (session/worker/ring/timeline mechanism/RT publication), because the frozen PlayerEngine proves their cohesion and its public ABI states "the engine owns its decode worker". **`MusicKernel` owns only the domain semantics** — state-machine meaning, track/session/playback/ENDED meaning, timeline interpretation. Mechanisms must never be folded into `MusicKernel`; `struct MusicKernel { worker, ring, renderer_handle }` would contradict "domain kernels own domain semantics" (#46). Whether the mechanisms live as separate modules inside the component is a later implementation choice, not frozen here. |
| Requires | `Decoder` (open/probe/decode/seek/EOF, stream info, metadata) — cardinality 1. `PcmSink` from AudioOutput (bind/negotiate, RT fill endpoint, render evidence) — cardinality 1. Unsatisfied requirement ⇒ component stays inactive/degraded; it never crashes the root (see §K). |
| Provides | `PlaybackControl` (open/play/pause/stop/seek with frozen pe_* semantics); `PlaybackSnapshot` (polled coherent instant + track metadata view + diagnostics). `PlayerView`/`PlayerAction` (Presentation) is the *payload vocabulary* of these services, not a separate capability or component. |
| Operations / data edge | Control calls: synchronous, caller-serialized. Snapshot: concurrent poll. PCM: Music resolves the `PcmSink` capability, then **binds its own fill endpoint** — conceptually `PcmSink.bind(PcmSourceEndpoint) → SinkSession` (lifetime shape, not a Rust API freeze). The sink's RT thread pulls blocks only through the endpoint handed to that session (realtime island; no Context/resolution per block). **Music owns the SinkSession binding effect** — creation, quiesce, teardown — and the binding's teardown is explicit and precedes provider final release (§G.2). **Activation rule (frozen): the bind happens at Music activation when PcmSink is resolved — not at track open** — so SinkSession existence follows solely from the live binding (idle ≠ absent), making it composition-owned truth (§H.a); track open/close changes what flows through the session, never whether it exists. Render evidence returns over the same session. No composition-root pointer wiring exists or is permitted (§J). Dependency loss arrives via kernel invalidation push, never as a direct reverse callback (see §D). |
| Observable contract | State; position/duration on the MEDIA timeline; landing quality; buffered/underrun diagnostics; typed errors with Decoder verdict attribution on open/seek failure. Frozen from `pe_snapshot`. |
| Commutativity | Multiple snapshot/control listeners commute → contribution-oriented `register -> token`. Control operations on one session do **not** commute (open/play/seek/stop order is the state machine). |
| Ordering | State-machine transitions; seek-landing vs ENDED (probes tolerate idle-in-band); commit/flush single-flight (REQUESTED/CLAIMED/COMPLETED/CANCELLED, I3/I4/I5). |
| Effects | Session open (Decoder handle): reversible via close, with teardown-access window. Ring/worker allocations: reversible, owned inside. Listener registration: reversible token. |
| System boundary | Audio actually rendered by the sink is outside rollback (submitted ≠ rendered; claimed physical flush is irreversible per I4). Host-IO side effects during decode belong to the host, not to Music. |
| Lifecycle dependency | Dependent of Decoder and of AudioOutput: must quiesce the worker, park the pipeline, **tear down its SinkSession binding**, and close the handle (teardown access still valid) before either provider's final release. Historical proof: `pe_destroy` order; renderer destroyed before engine. |
| Granularity cost | Splitting Transport/Session/Timeline/Worker apart re-exposes the dual-timeline-accounting bug class over one coherent state machine; no split is justified at MVP. |

### B.2 Decoder (decode mechanism provider; historical SongCore)

| Field | Analysis |
|---|---|
| Component | `Decoder` — one provider owning the media dependency closure and per-song decode. |
| Owns | One FFmpeg closure (build profiles, #48 authority: Decoder/Processing share one closure); per-handle decode state (codec context, packet/frame state, EOF/seek state); probe result (identity, metadata, artwork, stream info); `STREAM_CHANGE` fail-closed policy. |
| Requires | Nothing. Host-IO callbacks (`SongSource`) are **payload on a data edge**, not a capability. |
| Provides | `Decoder` capability: `open(io) -> handle`; probe (stream info/metadata); decode → source-rate Float32 interleaved PCM; `seek`; EOF as a normal terminal; typed statuses. |
| Operations / data edge | Handle method calls; decoded PCM blocks flow directly into the consumer's ring (data edge). No events. |
| Observable contract | `song_status` codes; stream info (rate/layout/duration); metadata; PCM at source rate; last-error diagnostics. No FFmpeg type ever crosses the seam (proven ABI discipline). |
| Commutativity | Distinct handles are independent by construction (no shared state, no internal mutexes). Same-handle operations are sequential/ordered. |
| Ordering | Within-handle sequential semantics (decode/seek/EOF). Nothing shared-key across components. |
| Effects | Handle open/close: reversible. Per-handle allocations: reversible, owned inside handle lifetime. Provider activation (FFmpeg init): locally reversible. |
| System boundary | None at runtime beyond host-IO reads (host-owned effects). |
| Lifecycle dependency | Music must close handles and stop issuing calls before the provider's final release (teardown-access window), e.g. before a closure unload. |
| Granularity cost | Per-codec plugins multiply the closure authority for zero composability gain (codec selection is intra-provider dispatch). Splitting probe/metadata from decode breaks the proven one-file-one-snapshot contract. No split justified. |

### B.3 AudioOutput (physical output provider; historical WASAPI renderer / null backend)

| Field | Analysis |
|---|---|
| Component | `AudioOutput` — one component owning the device session and the render mechanism island. |
| Owns | Device session (endpoint activation, negotiated format, event-driven render thread); submit/playout accounting (padding-proven advance, underruns); format conversion to the device format (SRC — provisional placement, see §I); bounded-retry device-loss degradation. |
| Requires | Nothing at the capability plane (the device is platform, not a capability). The data-plane pull is **not root-wired and not a reverse requirement**: the edge exists only inside a `SinkSession` created by the dependent's explicit `PcmSink.bind(PcmSourceEndpoint)` on the resolved capability. AudioOutput holds only the endpoint handed to that session and can never reach Music as a capability — if no session is bound, the output never activates. This direction is what keeps the graph acyclic (§D). |
| Provides | `PcmSink`: `bind(PcmSourceEndpoint) → SinkSession` (format negotiated at bind; RT-safe swap/flush via the commit/flush handshake) — conceptual lifetime shape, not a Rust API freeze. `OutputDeviceDiscovery`: read-only endpoint enumeration, when a consumer needs it. Two capabilities, one provider. |
| Operations / data edge | SinkSession: created by the dependent's explicit bind at control time; per-block pull happens only through the session's endpoint (no control-plane work per block); evidence counters observable; flush/swap via the single-flight handshake. |
| Observable contract | Bind success/failure + negotiated format; render evidence (underruns, playout); device list. |
| Commutativity | Discovery reads commute. Evidence listeners (future) would commute via tokens. Sink sessions: exactly one per composition (cardinality follows from Music's requirement). |
| Ordering | bind → negotiate → start; any graph/format swap is a single-flight commit/flush (I3/I4/I5). |
| Effects | SinkSession binding: an effect **owned by Music** (the dependent) — explicit bind/teardown, and the teardown precedes provider final release. **Activation rule: bind at Music activation when PcmSink is resolved, not at track open** — the sink/device session's existence is therefore composition-owned (§H.a). Session resources and render thread inside AudioOutput: reversible, owned here. Format renegotiation: transactional swap through the handshake. |
| System boundary | Two distinct notions, kept separate (corrective P1-3): a **CLAIMED flush is the protocol point-of-no-return** — from CLAIMED, cancellation/rollback is no longer promised and control must await the definitive outcome (I4); the **physical render is the external emission boundary** — the actual crossing where sound leaves the recoverable system. The claimed flush stays classified Irreversible (rollback authority ends at CLAIMED; the flush may itself irreversibly discard device-buffer state), but the two notions must not be conflated: a claimed flush can complete without anything being emitted (e.g., muted device). Device state itself is outside runtime ownership. |
| Lifecycle dependency | Music (the PcmSink consumer) must park/quiesce its RT edge before sink-session release: renderer destroyed before engine (proven rule). |
| Granularity cost | DeviceSession/Renderer split forces the device handle or per-block calls across a seam inside one RT island — mechanism leakage for zero gain. Discovery-as-separate-component rejected (capability, not component). |

---

## C. Dependency graph (requires/provides)

```text
        Profile / desired composition (composition root)
                          |
       +------------------+------------------+
       | requires         | requires         |
       v                  v                  |
 +-----------+  Decoder  +---------+  PcmSink  +-------------+
 |  Decoder  |<----------|  Music  |---------->| AudioOutput |
 +-----------+  capability (domain  capability +-------------+
       ^         data edge: kernel)   data edge:  ^
       |        PCM blocks,           SinkSession |
     host IO     host IO payload      bound by    |
     (payload,       ^                Music       |
      data edge)     |                  render evidence
                  source file          (same session)
```

- Capability edges: Music → Decoder, Music → AudioOutput. Decoder and AudioOutput require no capabilities. The graph is a DAG.
- Payload/data edges (not capabilities): file/IO callbacks into Decoder; PCM blocks from Decoder into Music's ring; the RT pull from the sink thread into Music's fill endpoint — created only by Music's explicit `PcmSink.bind(...) → SinkSession` (established at Music activation, §H.a) and owned by Music as a binding effect. **A data edge is not a capability edge, but every data edge still has explicit ownership, provenance and teardown**; none is wired by the composition root behind the capability plane (§J).

## D. Cycle / integration audit

**D.1 The naive cycle that must not be built.** Device loss tempts a design where AudioOutput calls "music.onDeviceLost()". That edge (`Music requires PcmSink` + `AudioOutput requires Music`) is a genuine cycle. Disposition: **not a real cycle — it is dependency invalidation in the wrong direction.** Device unavailability is *provider withdrawal* (AudioOutput stops satisfying PcmSink) and must arrive through kernel invalidation push to the dependent (Music), or be degraded inside AudioOutput itself (historical: bounded-retry with audible-position freezing as the honest "no output" signal). No reverse requirement, no mediation component needed. This is the same direction as the proven commit/flush control-thread boundary.

**D.2 Future cycle candidate (recorded, non-blocking).** When Library and UiHost exist: `Library requires FilePicker (from UiHost)` while `UiHost requires collection queries (from Library)` would cycle. Disposition at that future audit: the picker result is **payload** (the user-selected paths flow as data into a scan operation); a scan flow can be mediated by an integration component if push-based progress UI is required. Not decided now; nothing in the MVP depends on it.

**D.3 No other cycles.** MediaKeys/Analyzer/UiHost are pure consumers of Music-provided services. Two implementations of one capability (WASAPI vs null output) compete at profile/composition level, not as runtime peers.

**Split-cost accounting.** No integration component is introduced in the MVP. The future `AudioRuntime` (§J) is the *only* anticipated mediation component — justified when DSP nodes or multiple sinks become independent components — and is deferred with explicit triggers rather than pre-built.

## E. Interaction algebra

Frozen rule: **commutative relation → may compose as independent effects; non-commutative relation → explicit dependency/order/integration structure.**

| Interaction | Class | Structure |
|---|---|---|
| Multiple snapshot/control listeners (UI, media keys, logging) | **Commutative**, independently removable | Contribution-oriented `register -> opaque token`; removal of one token cannot damage others |
| Control operations on one Music session (open/play/seek/stop) | **Ordered** | Music's state machine; single owner, explicit transitions (frozen pe_* semantics) |
| Replace/open track session | **Ordered, transactional fail-closed** | open stops everything, drops the previous handle, lands READY @0 or ERROR; it is replace-with-fail-closed, not rollback-to-previous (honest semantics from the frozen ABI) |
| Per-block PCM fill through the SinkSession | Single-owner sequential (not a shared-key problem) | Edge created by Music's explicit bind and owned by Music as a binding effect; no cross-component same-key algebra per block; no root wiring |
| **DSP chain: EQ → Compressor vs Compressor → EQ** | **Ordered (non-commutative)** | Explicit ordered processing graph owned by the PCM-path owner; node positions are declared in desired composition and published RT-safely via commit/flush. **Never** registration order, mount order, hash iteration, or discovery order |
| Resampler placement (before/after DSP) | **Ordered** | Format negotiation result; becomes an explicit graph node when a graph owner exists (§I.4) |
| Visualizer/Analyzer taps (future) | **Commutative** | Read-only observers of the PCM path; observation never mutates; token removal independent. This is the deliberate algebraic *contrast* with transforms: taps commute, transforms do not |
| Device enumeration | Commutative (read-only) | Pure query |
| Capability bindings at the composition root across distinct keys | Commutative | Key-local by construction; operation locality enforced (no hidden cross-key writes) |
| Same-capability provider competition (two Decoders, two Outputs) | Not a composition operation | Resolution policy: required-single cardinality; ambiguity is a composition error at reconcile, never a silent pick |
| Commit/flush swaps (format/device/graph) | Ordered, single-flight | REQUESTED/CLAIMED/COMPLETED/CANCELLED with I3/I4/I5 |
| Host IO callbacks | N/A — payload | Data edge owned by host |

**Artificial non-commutativity audit (API design).** Contribution tokens must be opaque — sequence numbers or indices would leak insertion order into observable behavior and make removal order observable. Snapshot diagnostics (`buffered_frames`, `underrun_count`) are read-only bands for display, never identity, and confluence must not overfit them. No unregister-by-position exists anywhere. R0's concrete `AppRuntime::audio_output()` accessor must remain a bootstrap witness — as a public seam it would let consumers bypass capability identity. The fill seam must not hand buffer ownership across the boundary ambiguously: the sink provides the block, the engine fills it.

## F. Effect / system boundary classification

| Effect / resource | Class | Owner | Notes |
|---|---|---|---|
| Capability binding at composition root | Reversible | Root/Reconcile | Key-local; `with_audio_output()` today is the R0 witness |
| Snapshot/control listener registration | Reversible | Registrant via token | Commutative removal |
| Decoder handle (open session) | Reversible with teardown-access window | Music (handle), Decoder (mechanism) | `song_close` must remain valid during Decoder withdrawal (§G) |
| PCM ring, decode worker thread | Reversible | Music | Spawn/join, alloc/free owned inside Music |
| AudioOutput device session + render thread | Reversible open; transactional swap | AudioOutput | Format/device change goes through single-flight handshake |
| SinkSession binding (Music's RT data edge) | Reversible | **Music** (binding effect) | Created by Music's explicit `PcmSink.bind(...)`; torn down by Music, before provider final release (§G.2); never root-wired |
| Seek | Transactional (fail-closed) | Music | Success = CONFIRMED landing; failure = ERROR, never a half-seek |
| Track open | Transactional (replace-with-fail-closed) | Music | READY @0 or ERROR; no rollback-to-previous-track promise |
| Stop-recovery reopen | Compensatable | Music | Deterministic rebuild to READY @0; reopen on failure |
| Output device switch (planned or loss) | Compensatable | Music + AudioOutput | Park at last CONFIRMED landing → rebind → resume by policy; cannot restore the un-played interim (silence is the honest outcome) |
| Claimed flush — protocol point-of-no-return | **Irreversible** | AudioOutput mechanism | From CLAIMED, cancellation/rollback is no longer promised and control must await the definitive outcome (I4). Distinct from the external emission boundary below: a claimed flush may complete without audible emission |
| **Physically rendered audio — external emission boundary** | **Outside recoverable boundary** | Physical world | The actual crossing into the external world; `Everything is Plugin` ≠ `Everything is rollbackable`; submitted ≠ rendered is the accounting that keeps this truth visible |
| Host filesystem/network reads during decode | Outside boundary (host-owned) | Host via data edge | Runtime may observe failure, not roll it back |
| UI pixels already painted | Compensatable (repaint) | UiHost (future) | Never part of playback correctness |
| Library writes / playlist edits (future) | Compensatable (journal) | Library | Not in MVP; noted to preempt "everything is reversible" drift |

Two boundaries must never be conflated (corrective P1-3): **CLAIMED is the protocol point-of-no-return** — the runtime loses rollback/cancellation authority and must await the outcome (I4) — while the **physical render is the external emission boundary**, where sound actually leaves the recoverable system. The claimed flush remains an irreversible effect because rollback authority ends at CLAIMED (and a flush may irreversibly discard device-buffer state), but "past the point of no return" and "emitted into the world" are different facts. The architecture must never wrap either in an Effect closure and call the result rollback.

## G. Lifecycle ordering (provider disappearance)

Semantic sequence (frozen):

```text
provider begins withdrawal
        ↓ provider stops satisfying new resolution
        ↓ dependents are invalidated (push, not poll-and-fail)
        ↓ dependents deactivate / park
        ↓ dependents finish teardown while teardown access stays valid
        ↓ provider finally releases bindings/resources
```

**G.1 Decoder withdrawal/replacement during playback.** No new `open` resolves → Music is invalidated → Music quiesces the decode worker and closes the open handle (`song_close` — teardown access still valid) → only then does the Decoder provider release the FFmpeg closure. On replacement, Reconcile rebinds and Music recovers via the proven stop-recovery reopen (same source, per policy). Failure to keep the teardown-access window = use-after-unload; this is a kernel-0 invariant, not a convention.

**G.2 AudioOutput / device replacement.** Output withdrawal (planned switch or device loss) → no new bind/resolution → Music invalidated on the PcmSink edge → Music parks the pipeline at the last CONFIRMED landing (media-time truth preserved by submitted ≠ rendered accounting; GAP = zero media time) → Music quiesces the RT edge via the commit/flush handshake and **tears down its SinkSession binding (the effect Music owns)** → AudioOutput releases the device session → Music re-binds by presenting its fill endpoint to the new capability; format renegotiates (SRC parameters may change) → Music resumes by policy. The historical bounded-retry renderer is the degradation shape when no replacement exists: position freezes, state honest.

**G.3 AudioRuntime replacement (future).** The graph owner's withdrawal invalidates both Music's path and DSP node contributions; nodes tear down their contributions (tokens), Music parks, buffers are released last by the graph owner. Design-level only; triggers in §J.

**G.4 UiHost removal.** UiHost is a dependent, never a playback provider. Withdrawal invalidates nothing except its own registrations: listeners unregister by token (independent removal), playback continues untouched. Headless operation is the standing proof that no playback path requires UiHost.

**G.5 Root disposal.** Reverse of activation: dependents before providers — Music deactivates (stop publications → join worker → quiesce audio path, including tearing down its SinkSession binding → close handle; the documented `pe_destroy` order), then Decoder and AudioOutput release. On the output side the proven rule is renderer destroyed before engine.

## H. Confluence oracle

The oracle has **two layers that must never be conflated** (corrective P0-1, purified by corrective-2). The final plugin graph determines *composition truth*; it does not by itself determine historical media position, playback state, or any track/session resource. A domain session fact survives a mutation history only if an explicit continuity policy preserves it — and that policy (§K.3) is not frozen yet.

**Classifier (frozen, corrective-2).** A resource or fact belongs to §H.a composition truth **iff a fresh construction of the desired composition, prior to any user/domain action, would deterministically exhibit it**. If its existence or value depends on a domain session — which source is open, whether anything is playing, where the checkpoint is — it belongs to §H.b. No exceptions, including ownership counts. This is the audit's answer to "which state belongs to the composition calculus, and which to product/domain semantics".

### H.a Composition confluence

> After any legal load/unload/replacement history reaches quiescence, the composition truth — capability/Context reachability, Fiber/component lifecycle, composition-owned effects/bindings, and pipeline topology — is observationally equivalent to a clean construction of the final desired composition.

**Compared (composition-owned truth only):**

1. Capability reachability and bindings: PlaybackControl/PlaybackSnapshot present iff Music present; Decoder/PcmSink resolved and bound per the desired composition; required-single cardinality.
2. Fiber/component lifecycle truth: active set == desired set; nothing pending/zombie.
3. Composition-owned effects/bindings: capability bindings; contribution/token registries (listener sets equal by semantic identity, not token values; dispatch count == registered count).
4. Composition-owned data-edge bindings — **SinkSession and device session**. Frozen activation rule: **Music ACTIVE + PcmSink resolved ⇒ Music binds immediately**; an *idle* session (device running, nothing flowing) exists without any track. SinkSession existence therefore follows solely from the live Music↔PcmSink binding, never from track state. Evidence: the frozen renderer is created after the engine and destroyed before it — it spans the component lifetime, not the track session, and `pe_open` never creates or destroys render machinery. Consequence: exactly one live SinkSession and one device session per live Music↔AudioOutput binding, idle or not — this is the session-leak detector (H3). A future "lazy/on-demand sink activation" would be a change of this frozen rule requiring its own audit, and would move SinkSession to §H.b.
5. Pipeline topology: MVP trivially `Music → SinkSession → device`; once DSP exists, the explicit ordered node list equals the desired order.
6. Ghost absence: no bindings, contributions, or sessions beyond the desired set.

**Never compared in §H.a:** open Decoder handles, current source, PlaybackState, media position, CONFIRMED checkpoints, ENDED/reopen/recovery state — by the classifier these are domain-session truth and live in §H.b. Also never compared: allocator addresses, opaque token values, worker thread IDs, private generations, exact `buffered_frames`/`underrun_count` values (bands/invariants only).

### H.b Domain continuity / recovery (separate oracle)

**Domain-session truth (compared only here):** current source; Decoder open-handle existence/count; PlaybackState; media position; CONFIRMED checkpoint; ENDED semantics; reopen/recovery state; desired play/pause intent.

Playback state and media position are **domain session facts, not composition facts**. A clean rebuild of the final graph does not magically reconstruct a historical playing position. Continuity is judged only when an explicit policy says what must survive, in one of two legal forms:

1. **Checkpoint/apply:** capture an explicit domain checkpoint on the pre-mutation graph — source, last CONFIRMED landing, desired play/pause intent — apply the same checkpoint to both the settled graph and a clean build of the final composition, and compare the resulting playback semantics (handle existence, state-machine outcomes, landing quality, media-time behavior, error taxonomy).
2. **Behavioral probes:** run identical post-settle probe sequences (fresh open→play→seek→pause→stop) on both graphs and compare outcomes — same states, same error taxonomy.

**Clean baseline definition (frozen).** A clean baseline is a fresh construction of the final desired composition with **no domain session**: no open track, no Decoder handle, EMPTY state, idle SinkSession. Domain state enters a comparison only by applying the same explicit checkpoint/intent to **both** sides. No history may pre-load its clean baseline with historical domain state to make confluence pass.

Position/PLAYING equality after a replacement history is asserted **only** under a frozen continuity/resume policy (§K.3: proposed default — pause at last CONFIRMED landing). Until that policy is frozen, no history below claims position or PLAYING equality as a confluence conclusion; histories assert composition confluence (H.a) unconditionally and continuity (H.b) only as policy-conditional probes.

### H.c Histories

| # | History (→ settle) | Clean baseline | Composition assertions (§H.a, unconditional) | Continuity probes (§H.b, policy-conditional) |
|---|---|---|---|---|
| H0 | Root without UiHost (headless, null output) | Same root | The composition baseline itself: full capability/fiber/ownership truth without UI | n/a |
| H1 | open A, play → switch output X→Y → settle | **fresh build on Y — no track, no handle, idle SinkSession** | Capability set equal; exactly one live SinkSession + one device session on Y; lifecycle truth clean. Note: the settled history may legitimately end with a live domain session; that difference is §H.b material by design, never a confluence failure | Apply identical checkpoint/intent (open A at landing L, intent = play) to **both** graphs, then compare handle existence, state, landing quality; position-band equality only once §K.3 is frozen |
| H2 | open A → replace decoder provider (same source) → settle | **fresh build with new provider — no track** | Old closure released; bindings clean; counts composition-determined only | Recovery reopen compared via checkpoint/apply once the reopen policy is frozen |
| H3 | Output flapped X→Y→X→… (N times) → settle on Y — **no track open; pure composition churn** | clean Y (idle) | Exactly one live SinkSession + one device session total — flap must not leak sessions or leave ghost SinkSessions | n/a |
| H4 | Decoder replaced while PLAYING / while PAUSED / after ENDED (three runs) | same clean target (no track) | All three runs settle to identical composition truth | Continuity compared only via identical applied checkpoint/intent per run — never raw historical position equality |
| H5 | Listeners added/removed in opposite orders → settle | clean set | Final dispatch set identical (commutativity proof) | n/a |
| H6 | UiHost removed mid-playback → settle | root without UiHost (no track) | Playback capability/fiber truth unchanged by removal of a pure consumer | Domain-session stability: removing the UI must not perturb the playing session (compared pre/post removal, same graph, same checkpoint) |
| H7 | (Future, DSP) insert EQ → switch output → remove EQ → replace decoder → settle | clean Music+Decoder B+Output C (no track) | Node list == desired order; no ghost nodes/taps | Per policy once frozen |
| H8 | Root disposal from any quiescent state | n/a | Composition-owned resources released: device session closed, SinkSession torn down, worker joined | Domain-session resources released too: handle closed, track session gone (zero owned resources remain overall) |

H0–H3, H5, H6, H8 are expressible with the MVP decomposition alone; H4 needs decoder replacement; H7 needs DSP. All are design-level oracles now; they become executable tests only when the kernel exists (§K.6).

## I. Proposed Windows MVP decomposition (smallest justified graph)

**Three components. Nothing else.**

```text
Profile: windows-mvp (desired composition)
  Music      requires Decoder, PcmSink; binds its fill endpoint -> SinkSession
  Decoder    (FFmpeg closure provider; SongCore semantics behind the capability)
  AudioOutput providers PcmSink + OutputDeviceDiscovery
    - real provider: WASAPI shared-mode (historical renderer semantics)
    - headless provider: null backend (H0 baseline)
File input: CLI argument/path in headless (FilePicker deferred with UiHost)
```

Target functionality mapping: open local file (Decoder open + Music session), play/pause/seek/stop (PlaybackControl), position/duration + ENDED (PlaybackSnapshot media-time truth), basic metadata (probe result surfaced through Music's track view).

**Component vs kernel (frozen distinction, corrective P1-4):** the runtime unit is the **Music component**. It internally owns the domain semantics *and* the cohesive MVP playback mechanisms (session, decode worker, PCM ring, timeline mechanism, RT publication) because the frozen PlayerEngine proves that cohesion. **`MusicKernel` remains only the domain-semantic authority** — state-machine meaning, track/session/playback/ENDED meaning, timeline interpretation. This audit does not authorize a `MusicKernel` that owns threads, rings, or renderer handles; whether the mechanisms live as separate modules inside the Music component is a later implementation choice, not frozen here.

**Explicit answers:**

- **Is Decoder one plugin?** Yes. Codec-family selection is *not* another layer — it is intra-provider dispatch plus a build-time closure profile (#48: one FFmpeg closure authority shared with future Processing).
- **Is AudioOutput one plugin?** Yes. **DeviceDiscovery and DeviceSession stay separate? No** — discovery is a second capability of the same provider when a consumer needs it; session+renderer are one RT mechanism island and are not split.
- **Is Processing a component?** Not in the MVP. The Processing seam is a *frozen decision* (ordered graph, explicit positions, RT-safe publication), with no owner component until nodes exist. DSP stages (EQ/Compressor/Gain) are **not** independent plugins; when justified they become nodes contributed at explicit positions of an ordered graph.
- **Is Presentation a plugin?** No — it is the payload vocabulary (view/action/snapshot types) of Music's services; the stateless mapping needs no lifetime.
- **Is UiHost a plugin?** Yes when it exists — one component owning the platform UI mechanism, consuming only Presentation vocabulary, replaceable per platform (PocketJS/KuiklyUI), absent from the headless MVP slice.
- **Premature splits rejected:** see §J.

**Provisional decision — SRC placement:** format negotiation happens at bind; both declared formats are part of the binding contract; the sink owns conversion to the device format in the MVP. When DSP/a graph owner exists, SRC becomes an explicit graph node placed topologically correctly (SRC before vs after EQ is ordered topology) and moves into the graph owner's negotiation. Both sides declaring their format at bind is what makes this relocation possible without breaking the seam.

## J. Rejected alternatives

| Candidate / split | Verdict | Reason |
|---|---|---|
| MediaSource as a plugin | Rejected | Bytes/IO callbacks are **payload on a data edge**, not a capability; a "provide bytes" component adds a fiber with no reachability content |
| Per-codec Decoder plugins (MP3/FLAC/… plugins) | Rejected | Splits one FFmpeg dependency-closure authority into many; codec selection is intra-provider dispatch; zero composability gain |
| Probe/metadata split from decode | Rejected | Breaks the proven one-file-one-coherent-snapshot contract; forces two handles over one file |
| Transport vs Timeline/Session split | Rejected | Re-creates the dual-timeline-accounting bug class; one coherent state machine owns both |
| Decode worker/ring as a separate "AudioRuntime" now | Deferred, not rejected | With zero DSP the "graph" is one edge; AudioRuntime is the future integration component for the ordered PCM graph. Triggers: (a) DSP nodes as independent components, (b) multiple sinks, (c) graph publication independent of domain state. Its required contract (explicitly owned, teardown-able RT edges per §B.3; RT-safe swap) is already frozen by this audit |
| Resampler as a user-composable plugin | Rejected | SRC placement is derived from format negotiation (mechanism), not user composition; forcing it into the plugin set creates fake freedom with real ordering hazards |
| EQ/Gain/Compressor as independent runtime plugins now | Rejected for MVP | Not in target functionality; when they exist they are ordered graph nodes, and their algebra (non-commutative) is already frozen — building them as order-free plugins now would be wrong *by construction* |
| Mixer | Rejected for MVP | Single source; same-key contribution case (float-addition associativity caveat recorded); revisit with multi-source |
| DeviceDiscovery / DeviceSession / Renderer as separate components | Rejected | One RT island (device+thread+handshake); discovery is a capability of the same provider; splitting forces mechanism across a seam |
| Presentation as a fiber | Rejected | Stateless contract vocabulary; no owned state, no lifecycle |
| UiHost now | Deferred | Boundary justified (owns UI mechanism, replaceable); not needed by the headless Windows slice; hard rule recorded: UI never participates in realtime correctness |
| Library, MediaKeys, FilePicker, Analyzer/Visualizer now | Rejected/deferred | Not in target functionality. Recorded shapes: MediaKeys = consumer of PlaybackControl; Analyzer = commutative read-only taps; FilePicker = UiHost-provided capability; Library = future cycle case (§D.2) |
| "AudioOutput requires Music" (device-loss callback) | Rejected | Invented cycle; invalidation direction already solves it (§D.1) |
| Opaque sequence numbers as contribution tokens | Rejected | Leaks insertion order; creates artificial non-commutativity (§E) |
| `AppRuntime::audio_output()` concrete accessor as the seam | Rejected as a contract | Bootstrap witness only; bypasses capability identity |
| Composition root wiring the RT data edge directly (`root: sink.source = music.fill_endpoint`) | Rejected | A privileged bypass of the capability plane with no ownership, provenance or teardown; the edge exists only as the SinkSession created by the dependent's explicit `PcmSink.bind(...)` (§B.3, §C) |

## K. Blocking unknowns

None blocks designing COMPOSITION-KERNEL-0. The following are **kernel-0 design inputs** that this audit deliberately leaves open; each is a policy/mechanism choice, not a boundary gap:

1. **Capability-contract shape in Rust** (trait objects vs typed ports; SongCore ABI v1 as *the* Decoder seam vs as one provider implementation behind it). The audit's position: the capability is the `Decoder` service definition; the frozen SongCore ABI is a preservation asset and a candidate implementation, not necessarily the kernel seam.
2. **Async invalidation mechanism** (callback vs channel vs poll) — push invalidation and the teardown-access window are frozen; the transport is implementation.
3. **Resume/continuity policy after provider replacement** — proposed default: pause at last CONFIRMED landing (deterministic, honest). Until frozen, the continuity probes of §H.b remain policy-conditional; only the §H.a composition-confluence assertions are unconditional.
4. **Device-surprise policy for MVP** — proposed: fail-closed into the G.2 invalidation sequence with bounded-retry degradation; automatic rebind deferred.
5. **Degraded/inactive semantics** — when a required capability is unsatisfied (root without output), the dependent is inactive, not crashed. This is a real Reconcile/Fiber requirement discovered by the audit; expressible with the five-primitive budget.
6. **How confluence oracles become executable** (what the kernel exposes for observing capability/fiber/ownership truth) — kernel-0 design work, properly sequenced after this gate.
7. **Realtime-island enforcement** — the explicitly bound RT data edge (SinkSession, §B.3) is designed; how the kernel *guarantees* no per-block resolution (type-system vs discipline vs audit) is kernel-0/implementation concern.

No audit finding requires a kernel primitive beyond `Context / Capability / Fiber / Effect / Reconcile`.

---

## Verdict

Human review round 1 (PR #66): **PASS_WITH_CORRECTIVES** — four corrections. Revision 2 applied all four:

1. **P0-1** Composition confluence split from domain continuity/recovery (§H two oracles).
2. **P1-2** The RT data edge has explicit ownership/lifetime — `PcmSink.bind(PcmSourceEndpoint) → SinkSession`, owned by Music as a binding effect; composition-root wiring rejected (§J).
3. **P1-3** CLAIMED is the protocol point-of-no-return, distinct from the physical-render external emission boundary (§B.3, §F).
4. **P1-4** Music component vs MusicKernel: the component owns domain semantics plus cohesive MVP mechanisms; `MusicKernel` owns only domain semantics (§B.1, §I).

Human review round 2 (PR #66): **PASS_WITH_ONE_CORRECTIVE** — §H.a still mixed domain-session truth into composition confluence (e.g. "one open handle per active Music session": handle existence depends on track state, not on the final plugin graph). Revision 3 (this revision) applies **corrective-2**:

- **Frozen classifier (§H):** a fact is composition truth iff a fresh construction of the desired composition, prior to any user/domain action, would deterministically exhibit it; everything track/session-dependent belongs to domain continuity.
- **§H.a purified:** open Decoder handle, current source, PlaybackState, position, checkpoints, and recovery state removed; §H.a now compares only capability reachability/bindings, Fiber lifecycle, composition-owned effects/bindings, composition-owned data-edge bindings, ordered topology, and ghost absence.
- **SinkSession / device session classified, not vague:** frozen activation rule — bind happens at Music activation when PcmSink is resolved, not at track open; session existence follows solely from the live Music↔PcmSink binding (idle ≠ absent), grounded in the frozen renderer lifecycle evidence. They therefore stay in §H.a; a future lazy-activation rule change would require its own audit and move them to §H.b (§B.1, §B.3, §H.a.4).
- **Clean baseline definition (§H.b):** fresh construction with no domain session; domain state enters comparisons only by applying the same checkpoint to both sides. H1/H2/H3/H4/H6/H7/H8 rewritten so no baseline smuggles historical domain state.

Proposed verdict: **PASS**, pending confirmation of corrective-2. After confirmation, #53 may close and authorize opening the **COMPOSITION-KERNEL-0 design issue** (design only, still not implementation).

PASS still means only: **enough boundary evidence exists to design `COMPOSITION-KERNEL-0`.** It does not authorize implementing the kernel, freezing a Rust API, or starting FFmpeg/WASAPI/PocketJS integration in this task. Kernel-0 must adopt the decisions frozen here (§B–§I) and resolve §K as design inputs.
