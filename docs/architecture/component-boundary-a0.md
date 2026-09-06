# Component Boundary Audit A0

Design-gate evidence for **#53 COMPONENT-BOUNDARY-A0** (parent authority **#46 PLAYER-PLUGIN-ARCH-1**).

Status: **design audit, not implementation authorization.** Nothing here freezes a Rust API, a crate layout, or a dynamic-loading mechanism. Logical component boundaries are not crate/shared-lib/dynamic-lib boundaries.

BASE audited: `dae8dba` on clean `main`. Workspace tests green at audit time (2 passed).

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

Project memory corroboration: the timeline-ownership fix (splitting timeline accounting across submit/render sides was a real bug source) and the RT-island corrective (zero mutex on the render hot path) both argue for single-owner timeline + pre-bound data edges.

Historical implementations are evidence of *behavior and ownership truth*, not of the future component layout.

---

## B. Component boundary matrix (proposed components)

Three runtime components are proposed for the first Windows slice. Full matrix for each; every other candidate from the issue is dispositioned in §J.

### B.1 Music (player domain-kernel component)

| Field | Analysis |
|---|---|
| Component | `Music` — one component owning the player domain kernel (historical PlayerEngine semantics). |
| Owns | Playback state machine (EMPTY/READY/PLAYING/PAUSED/ENDED/ERROR); media-timeline truth (position/duration µs, CONFIRMED/ESTIMATED landing, GAP = zero media time); the active track session (including the open Decoder handle); decode worker thread; PCM ring; the RT-safe publication boundary (commit/flush); stop-recovery reopen policy. Future queue semantics stay here (domain kernel), not in a new component. |
| Requires | `Decoder` (open/probe/decode/seek/EOF, stream info, metadata) — cardinality 1. `PcmSink` from AudioOutput (bind/negotiate, RT fill seam, render evidence) — cardinality 1. Unsatisfied requirement ⇒ component stays inactive/degraded; it never crashes the root (see §K). |
| Provides | `PlaybackControl` (open/play/pause/stop/seek with frozen pe_* semantics); `PlaybackSnapshot` (polled coherent instant + track metadata view + diagnostics). `PlayerView`/`PlayerAction` (Presentation) is the *payload vocabulary* of these services, not a separate capability or component. |
| Operations / data edge | Control calls: synchronous, caller-serialized. Snapshot: concurrent poll. PCM: pre-bound RT data edge — sink render thread pulls blocks through the engine's mutex-free fill seam (realtime island; no Context/resolution per block). Render evidence (playout padding, underruns) returns over the same RT seam. Dependency loss arrives via kernel invalidation push, never as a direct reverse callback (see §D). |
| Observable contract | State; position/duration on the MEDIA timeline; landing quality; buffered/underrun diagnostics; typed errors with Decoder verdict attribution on open/seek failure. Frozen from `pe_snapshot`. |
| Commutativity | Multiple snapshot/control listeners commute → contribution-oriented `register -> token`. Control operations on one session do **not** commute (open/play/seek/stop order is the state machine). |
| Ordering | State-machine transitions; seek-landing vs ENDED (probes tolerate idle-in-band); commit/flush single-flight (REQUESTED/CLAIMED/COMPLETED/CANCELLED, I3/I4/I5). |
| Effects | Session open (Decoder handle): reversible via close, with teardown-access window. Ring/worker allocations: reversible, owned inside. Listener registration: reversible token. |
| System boundary | Audio actually rendered by the sink is outside rollback (submitted ≠ rendered; claimed physical flush is irreversible per I4). Host-IO side effects during decode belong to the host, not to Music. |
| Lifecycle dependency | Dependent of Decoder and of AudioOutput: must quiesce worker, park the pipeline, and close the handle (teardown access still valid) before either provider's final release. Historical proof: `pe_destroy` order; renderer destroyed before engine. |
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
| Requires | Nothing at the capability plane (the device is platform, not a capability). The data-plane pull from Music's fill seam is a **pre-bound edge established by the composition root**, not a requirement — if no Music is bound, the output never activates a session. This direction is what keeps the graph acyclic (§D). |
| Provides | `PcmSink`: bind/negotiate → sink session with RT-safe swap/flush (commit/flush handshake). `OutputDeviceDiscovery`: read-only endpoint enumeration, when a consumer needs it. Two capabilities, one provider. |
| Operations / data edge | Sink session: bind at composition/control time; per-block pull over the pre-bound RT edge; evidence counters observable; flush/swap via the single-flight handshake. |
| Observable contract | Bind success/failure + negotiated format; render evidence (underruns, playout); device list. |
| Commutativity | Discovery reads commute. Evidence listeners (future) would commute via tokens. Sink sessions: exactly one per composition (cardinality follows from Music's requirement). |
| Ordering | bind → negotiate → start; any graph/format swap is a single-flight commit/flush (I3/I4/I5). |
| Effects | Session open/close: reversible. Render thread spawn/join: reversible. Format renegotiation: transactional swap through the handshake. |
| System boundary | Once a flush is CLAIMED, the physical device action may start and can never be cancelled (I4) — that is the precise crossing point outside the recoverable boundary. Device state itself is outside runtime ownership. |
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
       |        PCM blocks,           RT fill seam|
     host IO     host IO payload      pull (pre-bound)
     (payload,       ^                        |
      data edge)     |                  render evidence
                  source file          (same RT seam)
```

- Capability edges: Music → Decoder, Music → AudioOutput. Decoder and AudioOutput require no capabilities. The graph is a DAG.
- Payload/data edges (not capabilities): file/IO callbacks into Decoder; PCM blocks from Decoder into Music's ring; RT fill-seam pull from AudioOutput into Music's ring.

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
| Per-block PCM fill through the RT seam | Single-owner sequential (not a shared-key problem) | Pre-bound edge; no cross-component same-key algebra per block |
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
| Seek | Transactional (fail-closed) | Music | Success = CONFIRMED landing; failure = ERROR, never a half-seek |
| Track open | Transactional (replace-with-fail-closed) | Music | READY @0 or ERROR; no rollback-to-previous-track promise |
| Stop-recovery reopen | Compensatable | Music | Deterministic rebuild to READY @0; reopen on failure |
| Output device switch (planned or loss) | Compensatable | Music + AudioOutput | Park at last CONFIRMED landing → rebind → resume by policy; cannot restore the un-played interim (silence is the honest outcome) |
| Claimed physical device flush | **Irreversible** | AudioOutput mechanism | I4: a claimed flush can never be cancelled; control must await the verdict |
| **Already-rendered audio** | **Outside recoverable boundary** | Physical world | `Everything is Plugin` ≠ `Everything is rollbackable`; submitted ≠ rendered is the accounting that keeps this truth visible |
| Host filesystem/network reads during decode | Outside boundary (host-owned) | Host via data edge | Runtime may observe failure, not roll it back |
| UI pixels already painted | Compensatable (repaint) | UiHost (future) | Never part of playback correctness |
| Library writes / playlist edits (future) | Compensatable (journal) | Library | Not in MVP; noted to preempt "everything is reversible" drift |

The system-boundary crossing point is **precise and historically encoded**: a flush becomes unrecoverable at CLAIMED (I4), and sound becomes unrecoverable at physical render. The architecture must never wrap either in an Effect closure and call the result rollback.

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

**G.2 AudioOutput / device replacement.** Output withdrawal (planned switch or device loss) → no new bind/resolution → Music invalidated on the PcmSink edge → Music parks the pipeline at the last CONFIRMED landing (media-time truth preserved by submitted ≠ rendered accounting; GAP = zero media time) → Music quiesces the RT edge via the commit/flush handshake → AudioOutput releases the device session → new output binds, format renegotiates (SRC parameters may change) → Music resumes by policy. The historical bounded-retry renderer is the degradation shape when no replacement exists: position freezes, state honest.

**G.3 AudioRuntime replacement (future).** The graph owner's withdrawal invalidates both Music's path and DSP node contributions; nodes tear down their contributions (tokens), Music parks, buffers are released last by the graph owner. Design-level only; triggers in §J.

**G.4 UiHost removal.** UiHost is a dependent, never a playback provider. Withdrawal invalidates nothing except its own registrations: listeners unregister by token (independent removal), playback continues untouched. Headless operation is the standing proof that no playback path requires UiHost.

**G.5 Root disposal.** Reverse of activation: dependents before providers — Music deactivates (stop publications → join worker → quiesce audio path → close handle; the documented `pe_destroy` order), then Decoder and AudioOutput release. On the output side the proven rule is renderer destroyed before engine.

## H. Confluence oracle

> After any legal load/unload/replacement history reaches quiescence, the observable runtime is equivalent to a clean construction of the final desired composition.

**Compared (public truth):**

1. Reachable capability set (PlaybackControl/PlaybackSnapshot present iff Music present; required bindings resolved).
2. Fiber/component lifecycle truth (active set == desired set; nothing pending/zombie).
3. Public service behavior probes: a fresh open→play→seek→pause→stop sequence on the settled graph behaves like the same sequence on a clean graph (same states, same error taxonomy).
4. Pipeline topology: MVP trivially `Music → ring → sink`; once DSP exists, the explicit ordered node list equals the desired order.
5. Contribution registries: listener sets equal by semantic identity (not token values); dispatch count == registered count.
6. Ownership counts: exactly one device session, one decode worker, one open handle per active Music session — the session-leak detector.
7. Position/state semantics: state equal; position within an honest equivalence band (±render quantum) because wall-clock differs; ENDED probes tolerate idle-in-band.

**Never compared:** allocator addresses, opaque token values, worker thread IDs, private generations, exact `buffered_frames`/`underrun_count` values (bands/invariants only).

**Histories:**

| # | History (→ settle) | Clean baseline | Extra assertion |
|---|---|---|---|
| H0 | Root without UiHost (headless, null output) | Same root | The confluence baseline itself; playback fully correct |
| H1 | open A, play → switch output X→Y → settle | clean open A on Y | State PLAYING; one device session; position in band |
| H2 | open A → replace decoder provider (same source) → settle | clean A on new provider | Recovery reopen per policy; old closure released (ownership counts) |
| H3 | Output flapped X→Y→X→… (N times) → settle on Y | clean Y | Exactly one session total — flap must not leak sessions |
| H4 | Decoder replaced while PLAYING / while PAUSED / after ENDED (three runs) | same clean target | All three settle to the same observable profile |
| H5 | Listeners added/removed in opposite orders → settle | clean set | Final dispatch set identical (commutativity proof) |
| H6 | UiHost removed mid-playback → settle | root without UiHost | Playback observables unchanged |
| H7 | (Future, DSP) insert EQ → switch output → remove EQ → replace decoder → settle | clean Music+Decoder B+Output C | Node list == desired order; no ghost nodes/taps |
| H8 | Root disposal from any quiescent state | n/a | Zero owned resources remain: device closed, worker joined, handle closed |

H0–H3, H5, H6, H8 are expressible with the MVP decomposition alone; H4 needs decoder replacement; H7 needs DSP. All are design-level oracles now; they become executable tests only when the kernel exists (§K.6).

## I. Proposed Windows MVP decomposition (smallest justified graph)

**Three components. Nothing else.**

```text
Profile: windows-mvp (desired composition)
  Music      requires Decoder, PcmSink
  Decoder    (FFmpeg closure provider; SongCore semantics behind the capability)
  AudioOutput providers PcmSink + OutputDeviceDiscovery
    - real provider: WASAPI shared-mode (historical renderer semantics)
    - headless provider: null backend (H0 baseline)
File input: CLI argument/path in headless (FilePicker deferred with UiHost)
```

Target functionality mapping: open local file (Decoder open + Music session), play/pause/seek/stop (PlaybackControl), position/duration + ENDED (PlaybackSnapshot media-time truth), basic metadata (probe result surfaced through Music's track view).

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
| Decode worker/ring as a separate "AudioRuntime" now | Deferred, not rejected | With zero DSP the "graph" is one edge; AudioRuntime is the future integration component for the ordered PCM graph. Triggers: (a) DSP nodes as independent components, (b) multiple sinks, (c) graph publication independent of domain state. Its required contract (pre-bound RT edges, RT-safe swap) is already frozen by this audit |
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

## K. Blocking unknowns

None blocks designing COMPOSITION-KERNEL-0. The following are **kernel-0 design inputs** that this audit deliberately leaves open; each is a policy/mechanism choice, not a boundary gap:

1. **Capability-contract shape in Rust** (trait objects vs typed ports; SongCore ABI v1 as *the* Decoder seam vs as one provider implementation behind it). The audit's position: the capability is the `Decoder` service definition; the frozen SongCore ABI is a preservation asset and a candidate implementation, not necessarily the kernel seam.
2. **Async invalidation mechanism** (callback vs channel vs poll) — push invalidation and the teardown-access window are frozen; the transport is implementation.
3. **Resume policy after provider replacement** — proposed default: pause at last CONFIRMED landing (deterministic, honest); must be fixed before confluence tests can assert H1/H2/H4 exactly.
4. **Device-surprise policy for MVP** — proposed: fail-closed into the G.2 invalidation sequence with bounded-retry degradation; automatic rebind deferred.
5. **Degraded/inactive semantics** — when a required capability is unsatisfied (root without output), the dependent is inactive, not crashed. This is a real Reconcile/Fiber requirement discovered by the audit; expressible with the five-primitive budget.
6. **How confluence oracles become executable** (what the kernel exposes for observing capability/fiber/ownership truth) — kernel-0 design work, properly sequenced after this gate.
7. **Realtime-island enforcement** — the pre-bound edge is designed; how the kernel *guarantees* no per-block resolution (type-system vs discipline vs audit) is kernel-0/implementation concern.

No audit finding requires a kernel primitive beyond `Context / Capability / Fiber / Effect / Reconcile`.

---

## Verdict

**PASS.** The music-player domain decomposes into three justified MVP components (Music, Decoder, AudioOutput) with an acyclic capability graph, frozen interaction algebra (commutative contributions vs explicitly ordered graphs), a precisely located irreversibility boundary (CLAIMED flush / physically rendered audio), dependency-directed lifecycle ordering with a teardown-access window, and a confluence oracle with concrete histories and public-truth comparisons.

PASS means: **enough boundary evidence exists to design `COMPOSITION-KERNEL-0`.** It does not authorize implementing the kernel, freezing a Rust API, or starting FFmpeg/WASAPI/PocketJS integration in this task. Kernel-0 must adopt the decisions frozen here (§B–§I) and resolve §K as design inputs.
