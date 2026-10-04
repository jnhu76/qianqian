# Playback execution model

```text
STATUS = CANDIDATE / NOT FROZEN
OWNER = #198
IMPLEMENTATION_UMBRELLA = #201
STAGE = #208 (Stage 5 explicitness; next independent review #209)
BASE_MAIN_SHA = b72cab4d647edba5ecb255050f9434a13e38c2d0
STAGE_4_MERGE_SHA = b72cab4d647edba5ecb255050f9434a13e38c2d0
PRODUCTION_BEHAVIOR_CHANGE = NONE
C1 = COMPLETE / MERGED (#204 / #205)
C2 = COMPLETE / MERGED (#206 / #213)
D1_D6_STATUS = CANDIDATE / PENDING STAGE-6 ACCEPTANCE (#209)
```

This is the canonical candidate cross-protocol execution authority and reading
entry point. [#198][198] owns architecture diagnosis/decisions; [#201][201] owns
sequencing. [#207][207] / [#214][214] produced the Stage-4 candidate;
[#208][208] makes that merged subject explicit without changing execution.
C1 [#205][205] merged at `029b0a4d7587ec39ef3eb768466ab0d2ce6ca16f`;
C2 [#213][213] merged at `cc87368d4dee74b977e08a87de2cba15a3187b32`,
with #206 closed. Both realizations are present at the exact base above.
Old issue status snapshots and historical implementation proposals are provenance,
not the current resume point.

**D1–D6 remain CANDIDATE.** Inherited statements retain only the force and scope
of their linked accepted authority. Current realization describes code, not a
new protocol definition. Stage 5 neither accepts these decisions nor freezes
architecture; those gates belong to #209 and #210 respectively.

The execution shape is state owners, resource-confined workers, typed bounded
seams, local serialization/program order, designated semantic authorities and
acknowledgement/join boundaries. A stateful struct is not thereby an actor,
Plugin or independent execution owner.

## 1. Scope / authority hierarchy

**A single execution reading entry point is not a single authority for every
playback protocol.** This document's proposed ownership is execution identity,
attachment, mutable-state ownership, waiting, ordering, failure responsibility,
quiescence scope, and bounds scope. It explains how existing protocols execute;
it does not replace their predicates.

| Concern | Owning authority | Role here |
| --- | --- | --- |
| Composition admission, Fiber lifecycle, activation, discharge | [K0 design][k0-design] §F–G, §L; [K0 implementation ADR][k0-impl] | Explain relation only; no new primitive or lifecycle |
| Projection/read-side firewall, Fact authority, realtime publication/reclamation | [PBK-001][pbk1] §1–2, §6 | Apply/link; do not redefine the constitution |
| Session ownership, terminal, Seek, Pause/Resume, Position, Open/replacement | [PBK-002][pbk2] D6, D11, D14.2–D14.8 | Link protocol truth; inherit whole-composition establishment from D14.6 |
| Output Plugin and concrete backend boundary | [PBK-003][pbk3] §4–5, §8 | Explain render execution; backend remains Output's owned mechanism |
| DSP Desired/Accepted/Applied/Transitioning/Settled | [DSP product model][dsp] §7.3; PBK-002 D14.11 for lifecycle/placement/admission | Explain control recording versus worker execution |
| Evidence, acceptance, commit, observation vocabulary | [Temporal semantics][temporal] §2–6 | Reuse vocabulary; protocol owners retain predicates |
| PCM transport | PBK-002 D8, D14.5/D14.11; PBK-003 §5; [edge][edge] and [render ports][ports] as implementation evidence | Explain ownership, bounds and waiting; do not promote implementation constants to protocol authority |
| Execution identity/attachment, ownership, waiting, ordering, failure, quiescence and bounds scope | **This candidate, D1–D6** | Proposed cross-protocol authority; requires review before freeze |
| Host-input failure/result settlement | This candidate D3/D5; C2 realizes it | Invocation-local admission/failure/acknowledgement/seal only; never D11 truth |

| Decision | Status | Scope / review gate |
| --- | --- | --- |
| D1 | CANDIDATE | Episode identity/attachment; fresh-core precondition, not structural enforcement |
| D2 | CANDIDATE | Cross-protocol result representation/consumption; inherited D14.6 establishment semantics remain accepted |
| D3 | CANDIDATE | Machine input failure and result settlement; C2 realization does not accept policy |
| D4 | CANDIDATE | Cross-protocol work-fate reading; each linked protocol retains its predicates |
| D5 | CANDIDATE | Shutdown/quiescence scopes; no process-wide shutdown contract |
| D6 | CANDIDATE | Local bounds/liveness scope; no global budget/deadline |

## 2. Reading views + Fact lens

Composition, Execution, PCM Data, and Observation are **reading views**, not
mandatory runtime layers. K0 admits Plugins and orders relation discharge;
Session owns an episode's relations; prebound typed PCM flows directly from
decode through processing/edge to output; observers read derived visibility.
K0 is absent from the per-block data path. LiveProcessing and PcmEdge remain
owned resources, and the WASAPI mechanism remains owned by Output.

Preserve the existing Fact lens:

```text
Command
  ↓
Acceptance / linearization
  ↓
Evidence
  ↓
Commit
  ↓
Semantic Fact / truth
  ↓
Observation
```

This is a reading sequence, not a requirement for a new object at every arrow.
Seek has a protocol-state commit, not a new Seek Fact. DSP setter success records
Desired; semantic Accepted is later worker validation/compilation. Pause intent
plus current engagement/tail evidence produces a read-side Paused projection;
there is no Resumed Fact. D11 terminal outcome is the currently designated
episode Fact. Neither `observe()` nor `wait_terminal()` establishes that truth.
See temporal §6 and the protocol owners above.

There is no universal mailbox order. Relevant orders are the host's serialized
replacement calls, completion/control lock holds, decode/render program order,
FIFO frames, first-wins evidence, and explicit acknowledgement/join edges.
Source, Position and DSP diagnostic reads need not form one cross-cell snapshot.

## 3. Identity / lifetime / attachment

### D1 — Episode attachment (CANDIDATE)

**One fresh `PlaybackSessionHandle`/completion core is attached to one playback
episode's establishment attempt. A clone references that same episode. Retry,
restart, repeat and replacement require a fresh core, fresh Session spec, and
fresh establishment attempt.** A failed establishment does not authorize reuse
of the attempted core.

Authority source: PBK-002 D11 designates the Session semantic role for **one
episode**, not Fiber identity by definition; D14.6 establishes fresh compositions.
Code evidence: [handle][handle] `PlaybackSessionHandle` documentation explicitly
forbids reuse for restart/retry; `new()` creates a completion core, while `Clone`
shares it. [Session factories][session] capture that core. [Reference replacement][player]
`replace_episode`/`retire_old_episode`, with [RealEpisodeSource][entry] `start`,
construct a new root, handle and spec after old-side clearance. Semantic scope:
playback-domain attachment, not a constraint on every K0 component.

| Identity | Meaning; relation to the episode |
| --- | --- |
| Playback episode | One Session semantic authority and its resource lifetime; core is its retained command/read reference |
| K0 component identity | Reusable registered `ComponentSpec`; a name/spec is not an episode |
| K0 Fiber identity | One mounted composition instance; generic lifecycle can activate it more than once |
| Generic K0 remount/reactivation | K0 design §F.1/§L and [kernel][kernel] `activate_fiber`/`mount_fiber` support fresh revisions and dependency-driven reactivation; this does not authorize playback-core reuse |
| Thread identity | Decode/render execution context; not semantic episode identity |
| Backend/device identity | Output-owned mechanism/device selection; not episode or Plugin identity |
| Resource identity | Endpoint, edge, processor, stream, gate; subordinate lifetime resources, not an additional execution generation |

**Can one completion core establish twice?** Generic APIs technically permit
invoking a captured production factory again or constructing multiple specs
over the same handle. Production factories do not structurally enforce one-shot
use. Under this proposed playback attachment contract, that use is **unsupported
by the playback domain**, not supported and not structurally forbidden. No
supported product path needs it. Test-only factories using `Option::take()`
guards in `session.rs` establish a fixture's one-shot condition, not a production
guarantee. K0's own internal slot/generation representation remains K0-local;
this decision introduces no playback generation, epoch, registry or ActorRef.

The application assembly must respect this precondition, including when using
generic reactivation APIs. This is a reviewable clarification of the documented
handle lifetime, not a proposal to change K0's supported semantics. Whether to
enforce the precondition structurally remains an unchosen implementation
hardening question outside Stage 5. #208 only clarifies the candidate domain
precondition; it authorizes no executable enforcement.

### D2 — Whole fresh-composition establishment (CANDIDATE; inherited D14.6)

**PBK-002 D14.6 already defines the authoritative new-episode activation result
`Activated` over the WHOLE fresh composition.** Required providers, dependency
resolution and Session activation all belong to that result. This candidate does not
reopen or redefine it.

Generic K0 operation success is not playback `Activated`. `FiberState::Active`,
`source_format.is_some()`, `activation_error.is_none()`, `revise_desired() == Ok`,
absence of diagnostics, “not terminal”, and “playing” are not independently
establishment truth. `CompositionSnapshot` and Fiber-state observations cannot
decide whether a host needs terminal waiting or resource retirement (PBK-001
§2.3; PBK-002 D14.6).

**Proposed host representation family**, without prescribing a new API:

```text
ESTABLISHED

NOT_ESTABLISHED {
    diagnostic: optional
}
```

This represents the existing D14.6 meaning; it is not a new establishment
definition: `ESTABLISHED` represents D14.6 `Activated`; `NOT_ESTABLISHED` represents
an attempt that did not reach that result. The assembly boundary must carry the
authoritative completed attempt classification for the whole fresh composition
to its host. An optional diagnostic may identify admission, provider, dependency
or Session activation failure, but is non-authoritative presentation text.
Classification is independent of diagnostic presence: `NOT_ESTABLISHED` without
a diagnostic is valid, and absence of a diagnostic cannot imply `ESTABLISHED`.
The current [machine presentation][machine] `activation_failure_report` already
accepts `None`. The disposition of attempted resources remains separately
governed by authoritative disposal;
`NOT_ESTABLISHED` never implies they have already discharged. Activation failure
is not an episode `Failed` Fact.

`ESTABLISHED` means the fresh playback execution world and Session terminal
authority exist, so `wait_terminal()` is semantically meaningful. The outcome
may already have committed before the host receives this result. An immediate
`Completed` or runtime `Failed` does not retroactively make an established world
unestablished. There is no required “currently playing” interval.

**Machine and reference hosts must consume the same whole-attempt result.**
Machine: established → wait terminal, dispose, settle invocation; unestablished
→ dispose the attempted composition and report activation failure without
waiting for a nonexistent terminal authority. Reference: probe first; old-side
clearance uses absent root or `Discharged`; only then create/establish fresh
world. On success install that episode; on failure perform D14.6 failure-clean
disposal or retain the violated root and fail-stop. Neither host derives the
choice from a read-side snapshot. C1 realizes representation and placement;
the candidate does not itself prescribe an API spelling.

C1 provenance: [#204][204] / [#205][205] record the removed Fiber/source/diagnostic
reconstruction. Current establishment correctness follows the operation result
below; those historical paths are not current evidence.

### C1 merged realization (#204 / #205; no semantic amendment)

Machine and reference now call the same headless `assembly::establish` factory.
It owns fresh-root creation, selection/registration of the canonical Decode and
Output providers, Session construction, and the fixed three-member desired
composition. Its inputs are only the source, processing configuration and
initial output level: callers cannot supply a root, Plugin spec or desired list.
The private assembly routine constructs exactly the Decode/Output/Session
entries. Topology expansion requires review of this authority boundary rather
than silently extending what the Session result certifies. The deterministic
scope oracle reproduces an extra failing desired Plugin under generic K0, then
asserts exact membership of the canonical assembly; the projection appears only
in that test oracle, never in production classification.

The factory drives that entire composition synchronously, then consumes the
`EstablishmentAttempt` paired with `playback_session_spec_with_establishment`.
`EstablishmentResult::Established` represents the existing D14.6 `Activated`;
`NotEstablished { diagnostic }` represents its absence. Representation !=
semantics; Projection != authority.

The Session activation operation alone writes that return slot, after all its
acquisition steps and inverse registrations succeed. In this supported fresh
wiring, K0 can enter Session activation only after both required providers have
activated and committed bindings. Provider failure or unresolved dependency
therefore leaves the attempt `NotEstablished { diagnostic: None }`; Session
failure records `NotEstablished` with presentation text. Source publication,
render open, generic admission success and terminal timing cannot produce or
revoke `Established`. Registration or desired-admission refusal produces the
same `NotEstablished` classification for both hosts, before Session activation.
The machine's admission report/exit class travels separately as presentation
metadata; it cannot select establishment or terminal waiting. Optional diagnostic
text is non-authoritative. The root still requires authoritative disposal on
failure, including admission refusal.

Placement comparison: a generic K0 result would unnecessarily widen the kernel;
a handle observation would repeat the forbidden read-side reconstruction; an
app-local result cannot receive the private Session activation return without a
cross-crate seam. The paired constructor/consume-only attempt is that minimal
seam. The application factory structurally fixes the whole composition to the
canonical three-Plugin wiring. Fresh-core/single-attempt remains a precondition; no
reattachment enforcement, second lifecycle or terminal rule is added. The result
survives immediate D11 settlement and disposal. C2 realizes only the machine-host
settlement in §8; the overall execution architecture remains
CANDIDATE / NOT FROZEN.

## 4. Owner / writer inventory

An entry can be a semantic owner, execution context or owned communication
resource; these are not additional Plugins. Session core and SessionCompletion
are the semantic role and its realization, **not two terminal authorities**.
All “current” entries map to production at the audited SHA; D3/D5 remain
candidate cross-cutting policy, realized by merged C2.

Here, **owner** names lifetime/teardown responsibility; **writer** names the
path that mutates a particular record; **serializer** names the local lock or
program order that orders those mutations; **execution context** names where
that path runs; **semantic authority** names the role designated by the owning
protocol to establish truth. **Observers** consume truth/evidence/visibility.
These dimensions can align without being interchangeable.

Owner ≠ writer; writer ≠ serializer; serializer ≠ semantic authority.
Plugin ≠ execution owner; execution owner ≠ thread; thread ≠ semantic authority.
Resource ownership does not designate a Fact authority.

### 4.1 App / host

| Attribute | Current realization / candidate obligation |
| --- | --- |
| Identity; lifetime; lifetime owner | One QianqianApp composition root per attempted episode; machine invocation or longer-lived ReferencePlayer owns roots and host collections |
| Execution context; state; writers | Serialized caller/control thread; host methods write current root/handle, playlist/navigation/volume, desired input and failure-clean disposition; generic K0 alone writes admitted desired/catalog/Fiber state behind the root |
| Observers | The reference shell reads navigation/host feedback; machine caller reads final result; composition snapshots are diagnostic readers only. |
| Inputs; outputs | User commands/probe/source and committed terminal observations → fresh assembly/replacement, disposal, status and exit result |
| Waiting seams; serialization | Synchronous probe/activation, terminal condvar and disposal joins; mutable host API/program order serializes Open and replacement |
| Semantic authority; failure responsibility | PBK-002 D14.6 App Open/replacement and U2 policy; no D11 authority. Owns assembly/admission/probe failures, fail-stop retention and proposed D3 result |
| Stop/cancellation; termination ack; resources | Stop old unsettled episode then wait/dispose; authoritative disposition `Discharged` or violation. Owns root/host collections; reader termination is separately scoped |

Evidence: entry `start_episode`, `RealEpisodeSource`; player `replace_episode`,
`retire_old_episode`, `failure_clean_start`, `quit`; [App][app] and kernel disposal.

### 4.2 Playback Session semantic core

| Attribute | Current realization / candidate obligation |
| --- | --- |
| Identity; lifetime; lifetime owner | D1 one fresh handle/core for one attempt/episode; App owns attachment/root; retained clones extend storage lifetime |
| Execution context; state; writers | No dedicated Session thread. Completion state/cells hold mutable episode intent/evidence/truth; methods write them on command callers, decode exit/failure paths and render evidence callbacks; no dedicated Session thread |
| Observers | Retained handle callers observe committed terminal and derived read side; assembly consumes establishment separately. |
| Inputs; outputs | Commands + designated worker/drain/gate evidence → command history, seek decisions, immutable terminal Fact and read-side visibility |
| Waiting seams; serialization | Completion mutex/condvar; atomic predicate/commit holds and worker/render program order, not mailbox order |
| Semantic authority; failure responsibility | D11 single episode terminal authority; routes mechanism failures into D11 predicates, activation failure remains distinct |
| Stop/cancellation; termination ack; resources | Stop/teardown release routed waiting; terminal and worker joins are separate acknowledgements. Owns episode relation lifetime for endpoint/worker/edge/render, not provider mechanisms |

Evidence: handle/session/completion. Ownership authority: PBK-002 D6/D11/D14.2.

### 4.3 SessionCompletion realization

| Attribute | Current realization / candidate obligation |
| --- | --- |
| Identity; lifetime; lifetime owner | Shared completion Arc of D1 core; handles/activation closures/callbacks hold references, evidence callbacks use weak core references |
| Execution context; state; writers | CompletionState under mutex: stop/pause, activation/source evidence, first failure/terminal, worker/gate/cut fields, bound stop target. Session activation caller writes activation/source evidence and binds stop target; command callers write intent/routing; worker/render callbacks publish their designated evidence |
| Observers | Handle observers/waiters; worker reads seek/ending evidence, render receives routed gate state; none gains a second terminal authority. |
| Inputs; outputs | Typed calls/evidence → gated routing, seek slot/release, notifications, observation; no PCM dispatch |
| Waiting seams; serialization | `publish_evidence` resolves/commits under one hold; `wait_terminal` only condvar-waits. Seek rechecks episode/slot conditions under completion+slot lock; the earlier edge sample does not establish joint eligibility at plant (temporal §6.1) |
| Semantic authority; failure responsibility | Implements Session authority, never an independent Fact owner; first worker failure diagnostic retained. Exact resolver precedence is current implementation detail subject to D11's external propositions, not a frozen universal arrival order |
| Stop/cancellation; termination ack; resources | Monotone Stop and teardown routing suppression; while unsettled, worker-gone published before stranded-slot abort. Postterminal publication is suppressed. Holds gate/drain/slot/Position/control cells and potentially retained stopped edge; no thread to join itself |

Evidence: completion `publish_evidence`, `wait_terminal`, `request_seek`,
`worker_exited`, `resolve`; ports synchronous observers outside signal locks.

### 4.4 Decode worker

| Attribute | Current realization / candidate obligation |
| --- | --- |
| Identity; lifetime; lifetime owner | One spawned worker of established Session; Session's registered inverse owns its JoinHandle and endpoint relation |
| Execution context; state; writers | Worker thread exclusively owns decoder endpoint, processing instances, staging/remainder, local pending seek; decoder reads/seeks and LiveProcessing progression confined to it |
| Observers | Session observes worker evidence; render consumes edge PCM; teardown joins worker return. No public decode-cursor reader. |
| Inputs; outputs | Source frames, seek pickup, latest DSP Desired → processed PCM, actual seek landing, failure/EOF and worker-gone evidence |
| Waiting seams; serialization | Blocking provider reads/seeks; edge capacity 2 ms retries; gate/cut/release evidence waits. Provider seek → classification → history/edge invalidation → landing/cut commit → release consumption → new production |
| Semantic authority; failure responsibility | Mechanism executor, not independent D11 authority; reports decode/processing/panic evidence to Session before edge failure propagation |
| Stop/cancellation; termination ack; resources | Edge stop/episode-ending predicates; catch-unwind exit funnel calls worker-exit publication then stranded abort (already-terminal publication may be skipped); join in inverse acknowledges actual return. Endpoint/processing/staging drop with worker |

Evidence: session `decode_worker`, `write_observing_seek`; completion
worker/failure methods. PBK-002 D14.5/D14.11 own ordering and seek/DSP interaction.

### 4.5 LiveProcessing / ProcessingControl

| Attribute | Current realization / candidate obligation |
| --- | --- |
| Identity; lifetime; lifetime owner | Episode-owned processor/control resource; Session binds it, decode worker owns live engine; handle may retain control after engine ends |
| Execution context; state; writers | Construction/Session factory records initial Desired and clears pending through `establish`; setters write Desired/pending/intrinsic-refusal diagnostics under control lock; activation caller binds by consuming pending and reading Desired in one hold; decode worker consumes pending and can write compile-refusal diagnostics under that lock, and alone writes accepted engine, old/new transition, elapsed frames and scratch |
| Observers | Command callers read refusal diagnostics; worker reads pending configuration. Product sound-state/read-model identity is not provided. |
| Inputs; outputs | Full desired config + episode format + staging block → worker Accepted/Applied processing and refusal diagnostic; no separate thread/mailbox |
| Waiting seams; serialization | Single latest pending slot; pickup deferred during active transition; whole-block boundary and decode program order govern application |
| Semantic authority; failure responsibility | DSP §7.3 configuration/transition semantics; no terminal authority; processing error reports Session failure evidence |
| Stop/cancellation; termination ack; resources | No independent cancel/join: ending/worker drop disposes engine; Applied Seek rebuilds fresh accepted target. Owns processor histories/transition scratch; no K0 identity |

Evidence: live `ProcessingControl`, `LiveProcessing`, `poll_update`, `stage`,
`invalidate_signal_history`; DSP §7.3 and PBK-002 D14.11.

### 4.6 PcmEdge producer / consumer

| Attribute | Current realization / candidate obligation |
| --- | --- |
| Identity; lifetime; lifetime owner | One prebound episode PCM resource; Session owns relation, shared references may outlive joined workers |
| Execution context; state; writers | No thread of its own; decode writes samples/write cursor/buffered count and EOF/fail/Applied-seek purge; render `read_frames` writes read cursor/buffered count; backend abort, command Stop and Session inverse call edge stop. One edge mutex serializes these writers; none gains semantic authority |
| Observers | Worker samples edge terminal; render consumes frames; only tests read occupancy; no application correctness via occupancy. |
| Inputs; outputs | Typed frames/close/stop/fail/purge → FIFO frames, `PcmPull::Eof` or `PcmPull::Stopped`; Failed/Stopped edge states both stop pulls, while Session retains the failure origin |
| Waiting seams; serialization | Data/space condvars; bounded ring; first terminal wins; read checks Stopped/Failed before buffered drain; EOF drains |
| Semantic authority; failure responsibility | Mechanism observation, not semantic truth; failure/EOF participates through Session evidence, not edge state alone |
| Stop/cancellation; termination ack; resources | Stop wakes data/space; no thread acknowledgement in edge terminal flag. Holds ring allocation until references drop |

Evidence: edge; session prebinds producer/consumer, PBK-002 D8/D14.5.

### 4.7 Render worker / device seam

| Attribute | Current realization / candidate obligation |
| --- | --- |
| Identity; lifetime; lifetime owner | One backend render stream per current Session relation; Output owns backend allocation mechanism, Session owns returned stream lifetime |
| Execution context; state; writers | WASAPI render thread owns COM/device/event objects, handed-off basis, tail probes and volume application; Session command/teardown paths and worker-side Session seek decisions route gate intent/release; render consumes release and produces engagement/position/drain evidence |
| Observers | Session receives drain/gate evidence; read side samples Position; stream owner observes join, not acoustic silence. |
| Inputs; outputs | Prebound PCM/gate/level → device submissions, actual queued-tail evidence, Position estimate, drain/failure callbacks |
| Waiting seams; serialization | Loop-top gate before buffer reservation; PCM pull may block after a device reservation, but no reservation spans a gate park; event/native waits; single render writer for Position; continuing-seek slot clearance waits for release consumption |
| Semantic authority; failure responsibility | Backend mechanism evidence only; Session D11 commits terminal. Output owns open failure/timeout cleanup before stream handoff; runtime failure published via owner callback |
| Stop/cancellation; termination ack; resources | Session releases gate before stop/join; open-abort closes gate before joining. Drain evidence is not join; returned stream join acknowledges thread/resources. Concrete backend is not another Plugin |

Evidence: ports, wasapi `open_stream`, `run_render_thread`, `WasapiStream`,
`open_and_run`, open-abort; PBK-003 §5/§8. Windows runtime/device evidence was
not executed by this documentation audit.

### 4.8 Machine stdin reader

| Attribute | Current realization / candidate obligation |
| --- | --- |
| Identity; lifetime; lifetime owner | Named `qianqian-stdin` thread per machine transport; host creates it, currently may detach; retained handle fixes its episode scope |
| Execution context; state; writers | Reader thread owns stdin lock/current line; host observes spawn failure; reader and owner share bounded first-failure bookkeeping and owner seal (C2) |
| Observers | Host observes admitted first failure via seal; external pipe reads emitted reports. No production observation/join of reader return. |
| Inputs; outputs | Before seal, stdin lines → commands/status/diagnostics; EOF normal closure; admitted spawn/read/caught-panic failures recorded then Stop routed if established/unsettled. After seal, no reader dispatch/report/output |
| Waiting seams; serialization | Blocking stdin and output I/O, local line order; no unconditional join or global input bound. D3 orders seal against admitted dispatch/output/failure response; OS stdin read is outside that ordering |
| Semantic authority; failure responsibility | Host infrastructure/result only; no D11 authority. Reader reports read/caught-panic before seal; spawning host records spawn failure synchronously; late failures discarded without output |
| Stop/cancellation; termination ack; resources | EOF/failure/return ends reader; seal closes admission but does not cancel blocked stdin. A returned post-seal read result exits/releases local resources; an unlocked preclosure open check can still precede physical read-start after seal. Actual return/join would acknowledge end; retained handles/stdin lock may survive host return |

Evidence: entry `machine_transport`/`finish_episode`; [machine input][machine-input]
admission, failure and acknowledgement operations; [oracles][machine-input-tests].
C2 realizes candidate D3/D5; it does not freeze the execution architecture.

### 4.9 Observation / read-side consumers

| Attribute | Current realization / candidate obligation |
| --- | --- |
| Identity; lifetime; lifetime owner | Host/UI/status readers, not automatically Plugins; caller owns subscription/poll lifetime and retained references |
| Execution context; state; writers | Caller contexts build display/navigation projections; completion/Position/control producers own underlying truth/evidence |
| Observers | User/UI/automation consumes rendered visibility; committed Facts remain owned by the producer, not the presenter. |
| Inputs; outputs | Pure `observe`, terminal wait, source/position/diagnostic reads → display/status and separately authorized App policy decisions from committed terminal Facts |
| Waiting seams; serialization | `wait_terminal` condvar does not resolve outcome; observations can be stale/cross-cell; no universal snapshot freshness |
| Semantic authority; failure responsibility | No playback authority. PBK-001 firewall forbids projection-based correctness decisions; App U2 may consume already committed Completed |
| Stop/cancellation; termination ack; resources | Reader cessation releases references, not episode resources by itself. Host reader failures belong to that host; no observer-demand terminal commitment |

Evidence: handle/completion observation; player `completed_fact`/`poll_eof_policy`;
entry status. PBK-001 §2.3 and temporal §6 own the firewall/vocabulary.

### 4.10 Canonical assembly boundary

| Attribute | Current realization |
| --- | --- |
| Identity; lifetime; lifetime owner | One synchronous `assembly::establish` call/attempt; machine or reference host owns the returned root and episode handle |
| Execution context; owned mutable state; writers | Serialized calling thread; assembly creates the root/catalog/three desired entries; assembly invokes generic K0 operations (K0 writes catalog/desired/Fiber state); only paired Session activation writes its attempt slot, or assembly returns admission refusal |
| Observers | Machine consumes `Episode.establishment`; reference consumes `StartAttempt.establishment`; diagnostics/source/terminal readers do not classify the attempt |
| Inputs; outputs | Source, processing config, initial level → root, handle, `EstablishmentResult`, separate admission presentation |
| Waiting seams; serialization | Synchronous K0 registration/revision/activation and provider opens; ordinary caller order; consume-only `EstablishmentAttempt::finish` after settle |
| Semantic authority; failure responsibility | PBK-002 D14.6 owns whole-establishment meaning; assembly fixes certification scope, covers registration/admission/provider/dependency/Session acquisition failures |
| Stop/cancellation; termination acknowledgement; resources | Failed attempt still requires host-owned disposal; the returned Established variant certifies establishment, not worker termination. The call can instead return NotEstablished. Fresh-core/single-attempt is a precondition. Owns construction, transfers root/handles to host |

Evidence: [assembly][assembly] `establish`, `assemble`, `admission_refused`;
[establishment][establishment] `EstablishmentAttempt`; Session `record_establishment`.
No independent Plugin/thread/result authority is introduced by this boundary.

### 4.11 HostInput / host-result record

| Attribute | Current realization |
| --- | --- |
| Identity; lifetime; lifetime owner | One `HostInput` shared record per established machine invocation; host owns settlement, detached reader references may extend storage lifetime |
| Execution context; owned mutable state; writers | Host/reader contexts; mutex protects admission-closed, one active operation, optional first failure. Reader operation writes admission/failure/ack; host writes closure and reads seal; spawning host reports Spawn synchronously without a reader |
| Observers | Invocation owner reads sealed failure; reader checks admission. No observation consumer commits a playback outcome |
| Inputs; outputs | Reader effects or infrastructure failures plus owner seal call → effect permit/completion ack and fixed first-failure classification; final exit also consumes genuine terminal/disposal presentation independently |
| Waiting seams; serialization | `admit`, failure record, `Operation::drop`, `seal` use one bookkeeping mutex; completion condvar releases it while owner drains active work. Neither blocked read nor reader return carries an operation debt |
| Semantic authority; failure responsibility | D3/D5 candidate host-result scope only; HostFailure is invocation-local infrastructure truth, never D11. Reports first admitted cause before Stop response |
| Stop/cancellation; termination acknowledgement; resources | Closure rejects fresh effects; drain acknowledges admitted operation, then seal fixes failure record. Does not cancel/join stdin. Owns only fixed bookkeeping/condvar, not edge/device/Session lifecycle |

Evidence: [machine input][machine-input] `HostInput`, `Operation`, `start_reader`;
[entry][entry] `machine_transport_with_reader`, `finish_episode`.

## 5. Communication / waiting seams

This is the actual waiting map at the audited base. It creates no generic
mailbox/global command queue. Protocol truth belongs to §1's owners; the policies
here inventory their current realization. Bounds are logical, not allocator bytes.
Exact capacities, slice durations, private helpers, lock/condvar layout and
concrete OS-thread layout below describe **current realization or tuning**.
This classification does not weaken accepted placement obligations: PBK-002
D14.11 freezes decode-worker staging as the current processing minimum.
Inherited protocol predicates, placement and scoped acknowledgements retain
their linked authority; changing those obligations requires that authority's
review, even when ordinary private mechanisms can be replaced.

| Actual seam / producer → consumer | Buffer / order | Blocking and wake / acknowledgement | Ending / capacity policy | Semantic role versus mechanism-only role; primary code |
| --- | --- | --- | --- | --- |
| PcmEdge / decode → render | 8192-frame ring × source channels; mutex FIFO prefix writes | Empty/Open consumer condvar; full producer capacity wait in 2 ms slices; data/space/terminal notifications. No ownership of another lock while waiting | Full **BLOCK** through retry; terminal first-wins; EOF drains whole frames; Failed/Stopped abandon buffered reads; Applied seek purges once on worker path | PCM transport, never Fact dispatch; Failed edge pulls return Stopped, Session failure evidence classifies truth. [edge][edge] `write_some`, `read_frames`, `wait_for_space` |
| Completion state / command + owner evidence paths → Session decision + handle readers | Fixed evidence/intent fields and one outcome; completion-lock order | `publish_evidence` commits if decisive and notifies terminal condvar; `wait_terminal` releases lock while waiting | Outcome **FIRST-WINS**; postterminal evidence **INERT**; no deadline while evidence absent | Realizes D11 settlement; pure wait is not commit. [completion][completion] `publish_evidence`, `resolve`, `wait_terminal` |
| Seek slot / handle → decode worker | One logical in-flight seek across slot, worker local and release; no queue/coalescing | `take_seek_command` takes target; worker observes between bounded write slices; no slot condvar | Busy/ineligible **REFUSE**; worker exit aborts stranded slot; one continuing cut must resolve before next | D14.5 admission/execution, not success receipt. Completion `request_seek`, `take_seek_command`; [session][session] `write_observing_seek` |
| Seek park/release / Session → render; render evidence → Session | Fixed seek hold, current park/tail latches, one release payload; local gate/leg order | Park event + tail probes; gate notify + 10 ms slice; worker polls cut/release in 2 ms slices; render takes payload on its path and applies rebase before next submission | Applied waits for cut conjunction; ending aborts; continuing worker frees slot after release consumed; no cut timeout | D14.5 protocol evidence/ack; park ≠ pause Fact, release ≠ join. Completion `seek_cutover_decision`; [ports][ports] `release_seek_hold`, `park_loop_top` |
| Pause gate / Session routed command → render | Idempotent pause flag; completion→gate routing order; engagement-scoped evidence | Loop-top park before device reservation; notify/slice; synchronous event callback outside gate locks | Resume/Stop/teardown release; open-abort permanently closes gate; late park request **INERT** once closed | D14.7 engagement/tail evidence supports Paused Projection, no Resumed truth. Ports `set_paused`, `pause_park`, `close_and_release` |
| DSP pending / product-control setter → decode worker | One whole pending config; control-lock record; **LATEST-WINS** | No condvar; worker takes once at fresh staging pickup, after processed remainder/seek obligations; no pickup during transition | Intrinsic/compile **REFUSE** keeps old continuation; pending may be superseded/unapplied at ending | Desired record ≠ Accepted ≠ Applied. [live][live] `update_desired`, `take_pending`, `poll_update`, `accept` |
| DSP transition / worker → same worker | One old/new pair plus one-block scratch; exclusive worker program order | No inter-thread wait; processed-frame progression; worker edge backpressure may suspend further processing | Complete-in-flight during continuing PCM; Applied seek rebuilds accepted target; ending/EOF drops incomplete work | DSP §7.3 sample-driven sound semantics, no publication epoch. Live `stage`, `invalidate_signal_history` |
| Drain signal + gate event callbacks / render → Session | One drain verdict **FIRST-WINS**; fixed current gate fields, no event queue | Synchronous observer before first `complete` returns, outside signal locks; terminal condvar notification through owner publication | Later drain calls inert; terminal suppresses later evidence; callback is not worker termination acknowledgement | Backend evidence feeds Session decision; Output never commits D11 independently. Ports `DrainSignal::complete`, `RenderGate::emit`; completion weak observers |
| Position + output level / render → read side; App → render | One Position cell/single writer; one routed level; no delivery queue | Pure atomic sample/load; rebase on render path; level apply at loop top | Unknown landing withdraws Position; terminal suppresses product derivation; latest routed level observed when leg progresses | Evidence/desired value, no acoustic claim or applied-level receipt. Ports `PositionEvidence`, `OutputLevel`; WASAPI `steady_loop` |
| HostInput admission/failure/ack/seal / reader or spawning host → invocation owner | One active effect, one fixed first cause; bookkeeping-lock order, not physical-event timestamp | Permit guard holds no mutex; Drop clears active + notifies; seal closes admission then condvar-waits for ack; no domain/I/O inside lock | **FIRST-WINS** failure; admission closed → **INERT** fresh work; admitted effects finish before seal; active output can delay seal | D3/D5 candidate host-result cut; not playback Fact or reader join. [machine input][machine-input] `admit`, `fail_with_response`, `Operation::drop`, `seal` |
| Blocking stdin / OS → reader | One reusable String, input-sized/unbounded line; local read/line order | `read_line` holds stdin lock and can block forever; owns no admitted-operation debt; no unconditional join | EOF normal; read error/panic tries effect admission; late read result exits on rejection; a preclosure open check can permit read-start after seal; unread input may be abandoned | Physical input wait is outside result serialization. Machine input `read_input`, `spawn_reader` |
| Worker joins / Session inverse → decode/render thread | One JoinHandle each; render inverse registered first, decode last | Ordinary blocking join after edge stop; gate release before render join. No mutex needed by joined work held across join | Thread return acknowledges termination; failure verdict remains distinct; native calls may prevent progress | D6 lifetime relations/K0 discharge, not terminal/acoustic truth. Session activation inverses; WASAPI `stop_and_join`; [open-abort][open-abort] |
| Device open/native waits / Output caller ↔ render; worker ↔ decoder | One open verdict slot; device-owned buffer; one endpoint | Open condvar up to 10 s then abort/join outside verdict lock; event wait 100 ms; native decoder/device calls may block | Failed open cleans before handoff; runtime abort stops edge + publishes drain; drain loop 5 s cap, not whole-operation deadline | PBK-003 realization; platform-specific calls/constants do not define generic semantics. [wasapi][wasapi] `open_stream`, `run_render_thread`, `drain_to_zero`; ports decode/output contracts |

Completion→seek-slot and completion→gate are directional routing acquisitions;
edge checks occur outside the completion hold. Gate/drain callbacks release
signal locks before entering Session. Host bookkeeping is never nested across
Stop, terminal wait, root disposal or blocking I/O. Independent local lock orders
are not a global execution order; observe and command callers can interleave.

## 6. Admission / in-flight / work fate — D4 (CANDIDATE)

Inherited protocol-specific semantics, with D1 attachment and D3 host settlement
proposed above. “Must settle” means the stated protocol obligation, not a common
command-debt object or a requirement that every intent become successfully
applied. Ending below means the relevant protocol's stop/teardown/terminal
conditions; it is not a second episode lifecycle enum.

| Work | Ingress and record point | Acceptance/linearization; busy policy | Ending/close policy and fate | Authority/code evidence |
| --- | --- | --- | --- | --- |
| Seek | `handle.request_seek`; command slot under completion/slot locks | Separate Open observation, then atomic episode/free-slot recheck + internal plant/reset/hold. Plant is semantic Accepted iff actual joint D14.5 eligibility holds there; a non-Open plant is **REFUSED/INERT**, never Accepted (temporal §6.1) | Never-Accepted ending-raced records cause no provider seek/purge/rebase and are abandoned/exit-cleaned. Accepted seek resolves by provider refusal preservation, cut commit/release consumption, or abort on episode ending. No promise of successful cut after Stop. Refused remainder must resume exactly; Applied discards old remainder/edge/history. No coalescing. Only ending may abandon an Applied pending cut; stranded slot is aborted on worker exit | PBK-002 D14.5; [completion][completion] `request_seek`, `seek_cutover_decision`, `abort_stranded_seek`; [session][session] `decode_worker`, `write_observing_seek` |
| Pause | `request_pause`; intent history under completion lock | Record/route intent atomically relative to stop/teardown; gate engages at render loop top. Repeated intent is idempotent, not a queued debt | Ending leaves history but suppresses new routing; teardown releases gate. No required Paused projection before ending, no join implied. Pause preserves processing history/resources | PBK-002 D14.7; completion `request_pause`, `release_pause_gate`; [ports][ports] gate |
| Resume | `request_resume`; clear intent under completion lock | Release routed gate; no Resumed Fact or acceptance acknowledgement debt | Release remains useful for wake/teardown; terminal history cannot restart execution. No coalesced mailbox or mandatory observer update | PBK-002 D14.7; completion `request_resume` |
| Stop | `request_stop`; monotone intent under completion lock | Infallible/idempotent record, then edge/gate response; late Stop cannot relabel terminal | Recorded intent may survive only as history. If still unsettled it participates in D11's decisive predicate; Stop must not be confused with terminal/join acknowledgement. EOF already reached remains drain-to-Completed absent failure | PBK-002 D11/D14.4/D14.7; completion `request_stop`, `resolve` |
| DSP Desired update | Setter composes/validates under `ProcessingControl` lock, commits Desired/pending | Setter success records coherent intrinsically valid Desired, **not** semantic Accepted; invalid request refuses unchanged | Desired is retained control history; no duty that each value is accepted/applied. Late retained setters may change local history without an active episode result | DSP §7.3; [live][live] `ProcessingControl` setters |
| DSP pending update | One pending complete config | **LATEST-WINS** before pickup; worker pickup validates/compiles for episode format to become Accepted | May be superseded or remain unapplied at ending; no successful-settlement debt or forced synthetic PCM. Refusal diagnostic may survive as history | DSP §7.3; live `take_pending`, `poll_update`, `accept` |
| Active DSP transition | Worker creates accepted target transition, starts Applied on a whole staging block | One transition; complete-in-flight before another pickup during continuing processing; sample-driven progression | Normal newer Desired does not preempt it. Applied Seek invalidates/rebuilds accepted target fresh; Refused Seek preserves it. Ending/EOF may drop an incomplete transition; current R0 has no artificial output-tail duty | DSP §7.3; PBK-002 D14.11; live `stage`, `invalidate_signal_history` |
| PCM buffered frames | Producer writes bounded edge FIFO; render pulls | Prefix write until capacity, **BLOCK** by producer wait/retry on full, not latest-wins | EOF preserves buffered drain; stopped/failed edge refuses reads before buffer drain, so frames may be abandoned. With the leg parked, provider Applied precedes history/edge purge; production then waits for the cut's tail-quiescence/commit and release consumption. Tail-quiescence is a commit condition, not a pre-purge condition. Refused Seek preserves frames exactly | PBK-002 D14.5; PBK-003 §5; [edge][edge] `write_some`, `read_frames`, `invalidate` |
| Late retained-handle commands | Same episode's retained core/control cells | Existing protocol methods only, no new attachment | Stop/Pause/Resume history inert for terminal semantics; Seek refuses. DSP/level histories may still mutate; no resurrection or successor binding. Terminal alone does not prove all worker code has stopped; joins do | PBK-002 D14.2/D14.4/D14.7/D14.9/D14.11; [handle][handle], completion/live |
| Stdin unread input | OS stdin/line reader, no playback record until parsed command dispatched | Reader's local line order only; not a universal mailbox; no bounded input admission promised. D3 admission closure rejects new command/failure-report/status-output effects before its drain/seal | EOF closes normally; unread input may be abandoned. After seal, the reader only exits/releases local resources on wake, without dispatch or output; no duty to consume every line. Infrastructure failures recorded before the seal enter the host result and require record-then-Stop for an established unsettled episode | D3 candidate; [entry][entry] `machine_transport` |

Refusal and abort are different from a successful semantic result. In particular,
an accepted seek awaiting device-tail quiescence can remain pending indefinitely
without new tail evidence, failure, Stop or teardown (PBK-002 D14.5). No new
timeout is introduced to make the table look uniformly settled.

## 7. Ordering / linearization / commit

The vocabulary in [temporal semantics][temporal] §2–6 classifies existing
operations. **Offered** means an invocation/request exists; **admitted** means its
local protocol permits work; **recorded** means state was written. A lock
**linearization** point need not be semantic **Accepted**, **Applied**, or
**committed**. Execution can produce mechanism **evidence**, while only the owning
protocol establishes semantic truth. Fact/projection publication and consumer
**observation** are different again. An **acknowledgement** closes its named duty;
**quiescence** needs the relevant execution/lifetime evidence in §9. These labels
introduce no fields, sequence numbers, event log or global total order.

| Protocol / offered → admitted or recorded | Linearization / executed work | Evidence → Applied / commit / truth | Projected / observed → acknowledged / quiesced | Owning authority / current code |
| --- | --- | --- | --- | --- |
| C1 establishment: host calls fresh canonical assembly; generic registration/desired admission may refuse | Synchronous required-provider binding precedes Session activation; `record_establishment` writes the operation return after acquisition/inverse registration succeeds; `attempt.finish` consumes it after K0 settle | Whole fresh-composition `Activated` is carried as Established; provider/dependency absence leaves NotEstablished; admission refusal returns it explicitly. Source/render-open/diagnostic observations alone cannot establish it | Hosts select wait or attempted-root disposal from same result; terminal may already be committed when result is consumed. No worker join from establishment | PBK-002 D14.6; [assembly][assembly], [establishment][establishment], Session `activate_established_with_spawn`, `record_establishment` |
| D11 terminal: evidence publication (not a new command); Stop intent may have been recorded | Session-owned `publish_evidence` mutates/evaluates `resolve` and memoizes under one completion hold; decisive publication serializes with Stop | Worker failure or worker-terminal/drain conjunction selects immutable Completed/Stopped/Failed under D11; backend evidence producer gains no Fact authority | Notify after commit; `observe`/`wait_terminal` only read. Terminal return is acknowledgement of truth visibility, not worker/render quiescence | PBK-002 D11/D14.3; completion `publish_evidence`, `resolve`; Session exit funnel, drain observer |
| Seek: offered target → preliminary eligibility; second completion hold plants one in-flight slot, resets operation evidence and routes hold | Recording serializes with Session ending/worker exit, but its separate edge sample does not certify Accepted. Temporal §6.1 maps plant to Accepted only when actual joint eligibility holds; non-Open plant is Refused/Inert. Worker waits for actionable park and checks ending/Open before provider seek, then calls provider before any invalidation. Refusal finishes preserved remainder; Applied discards history/remainder and purges once | Landing plus current park/tail evidence and worker program order feed D14.5 cut decision; one atomic Committed/Aborted/Pending sample. Cut commit is protocol state, no Seek Fact | Committed release routes actual landing; leg consumes/rebases before further submission; continuing worker frees slot after consumption. Position sample/reported target is not commit authority; no seek join | PBK-002 D14.5; completion `request_seek`, `seek_cutover_decision`; Session `decode_worker`; ports gate, WASAPI loop |
| Pause / Resume: caller offers intent → completion-lock record/route | Pause routes only while protocol eligible; render engages at loop top before reservation; Resume clears intent/releases gate | Current engagement and tail evidence support PBK-002 Paused derivation, never an intent-time commit or terminal Fact; disengagement proves only park ended | `paused()` is Projection; Resume is Command only. Park/tail acknowledgement is narrower than resource join; bounded prefetch can continue while paused | PBK-002 D14.7/.8; completion `request_pause`, `request_resume`, `apply_gate_event`; ports `park_loop_top` |
| DSP update: setter offers field/config → compose + intrinsic validation + Desired/pending record in one control hold | Recording linearizes racing setters; after remainder/seek handling and prior transition settlement, worker takes latest pending and compiles outside control lock for episode format | Worker compile gives semantic Accepted; next whole unprocessed block gives Applied/Transitioning; sample progression reaches Settled. Setter `Ok` is not Accepted. No new playback Fact or durability promise | Refusal diagnostics are separate reads; external sound/applied-config identity is not exposed. Ordinary updates complete-in-flight while processing continues; ending/Seek follows §6. No per-setter completion debt | DSP §7.3; temporal §6.5; live `update_desired`, `accept`, `poll_update`, `stage`; Session fresh-block pickup |
| C2 host settlement: physical read outside admission; returned command/report/failure offers effect → `admit` sets active if open | Failure record `get_or_insert` under host mutex precedes unlocked Stop response; `Operation::drop` clears active and notifies. After terminal + disposal, owner closes admission, waits ack, then reads first failure under mutex (final seal) | HostInput first cause contributes invocation failure independently of genuine D11 outcome. Admission closure ≠ final seal; admitted operation retains recording/effect rights during drain. No playback Fact is committed by host failure | Final owner reports/exit policy use sealed failure; fresh reader effects rejected after closure, post-seal read result inert. An unlocked preclosure read-open check may still lead to a physical read starting after seal; seal acknowledges admitted effects, not physical input/thread/process termination | D3/D5 CANDIDATE realized by C2; machine input `HostInput`, `Operation`; entry `finish_episode`; machine `machine_exit_code` |

There is no shared acceptance/commit instant across these rows. In particular,
temporal §6.5 and DSP §7.3's specific worker Accepted boundary govern DSP; generic
command-record/linearization wording must not collapse that later stage into
setter success. No publication implies persistence, pipe delivery or audibility.

## 8. Failure / cancellation / host-result settlement

### D3 — Machine input responsibility (CANDIDATE)

Authority source: #198/#201/#202 assign host-input responsibility and require a
deterministic settlement boundary; PBK-002 D11 owns episode truth. Code evidence:
[entry][entry] `machine_transport`, `finish_episode`, the named `qianqian-stdin`
thread, and [machine][machine]'s pinned observable reports/exit-code contract.
Semantic scope: **one machine invocation's host result and reader admission**,
not a general shutdown authority or new Session terminal kind.

| Input event | Proposed responsibility and consequence |
| --- | --- |
| EOF | Normal closure of the host input. No implicit Stop, episode failure, or requirement to finish playback early |
| Reader spawn failure | Machine host infrastructure failure; record on the invocation before any failure response |
| Read failure | Reader reports host infrastructure failure before its failure response/exit; not EOF |
| Reader thread panic | Host infrastructure failure when an unwind panic is caught/reported at the reader boundary; detaching an unobserved panicking thread is insufficient |

**Machine host failure is not `EpisodeTerminalOutcome::Failed`.** For a stdin
infrastructure failure admitted before seal, the host **must first record the
failure, then route the existing `request_stop()` to an established, unsettled
episode**, and consume the real D11 terminal outcome and authoritative disposal
verdict. A terminal race may make this idempotent Stop inert; it cannot relabel
the outcome. An unestablished attempt is disposed without terminal waiting.
This response is mandatory, so losing the command-producing reader does not
leave the host relying only on natural completion. It adds no timeout or new
terminal predicate. Session alone decides Completed/Stopped/Failed under D11;
host failure never manufactures decoder/render evidence.

### 8.1 HOST_RESULT_SETTLEMENT_BOUNDARY

The proposed exact settlement event is **the invocation owner sealing its host
result after obtaining the authoritative establishment classification, the
terminal outcome when established, and the attempted root's disposal verdict**.
It seals once and captures any host-input failure already recorded for that
invocation. This event precedes final result reporting and host-function return.
Neither terminal commit, `wait_terminal()` return, root disposal alone, stdout
flush, reader exit nor process exit is that event.

Failure recording and this seal must serialize on one ordinary invocation-local
record operation. Before seal, the first recorded stdin infrastructure failure
is retained (FIRST-WINS); further failures need not accumulate. The same ordering
must cover reader command dispatch, status/diagnostic output, and the mandatory
failure response: an admitted operation finishes before the seal, or loses
admission and performs none of those effects. A pre-seal check followed by
post-seal dispatch/output is forbidden. Recording an admitted failure precedes
its Stop response, and that response finishes before seal. Blocking OS stdin
reads remain outside this ordering; they need not return for the owner to seal.

The synchronization that decides reader admission, records host failure, and
seals the host result may protect only invocation-local bookkeeping. It **MUST
NOT remain held** while invoking playback `request_stop()`, `wait_terminal()`,
root `dispose()`, or potentially blocking reader/output I/O; release it before
those calls. An admitted operation's completion must be acknowledged separately
before seal. The seal waits for that acknowledgement, not for a mutex held
across domain/blocking calls.

At seal:

```text
activation/disposal/episode result remains independently classified
recorded stdin host failure adds a non-success host result
reader command / failure-report / status-output admission closes
sealed result cannot subsequently change
```

Thus **a spawn/read/reported-panic failure recorded before seal requires nonzero
host exit**, even when the episode naturally Completed. A race with Completed
is resolved by failure-record versus seal order, not by terminal-commit order.
A race with Stopped or Failed uses the same rule; the immutable episode outcome
stays Stopped or Failed. A failure whose physical occurrence predates the seal
but whose report loses that ordering race is excluded: this is a recorded-failure
cut, not a claim to observe every real-time occurrence. Spawn failure reporting
is synchronous on the invocation owner, so cannot be deferred past its seal.

After settlement, no reader command, failure report, status or diagnostic output
is accepted. A late failure is discarded locally, without reporting or output;
it cannot reopen or change this invocation's result. Panic-abort/process death and
failures of unrelated host code are outside this recoverable reader policy.
Merged C2 realizes these candidate rules as described below.

### Detached reader and deterministic exit

The reader may remain blocked in stdin after settlement; there is no unconditional
join. On return from a read after seal, it **admits no invocation effects and exits,
releasing reader-local resources**. Returned input/read errors are discarded;
it does not dispatch old-handle commands, format status, report failure or emit
diagnostics. An `is_open()` check that passed before closure can still be
followed by the physical read starting after seal: that unlocked check and the
OS read are not atomic with admission closure. Seal neither cancels nor prevents
that already-permitted read; after it returns, effect admission rejects its
result and the reader exits without another loop read. An otherwise valid old-handle command is still refused by this
closed host-input boundary. Generic retained-handle semantics in D4 remain
separate from machine reader admission.

Only the invocation owner emits the final settlement reports after seal, using
the sealed result and captured diagnostics. This preserves an observable cut:
reader commands and reader-owned output cannot cross into final reporting.
Completing an admitted pre-seal output operation can delay seal if output blocks;
no deadline is promised. Output here means the producer's output/flush operation,
not a guarantee that an external pipe consumer has received the bytes. Reader
termination itself remains unacknowledged without a join, and process termination
may abandon the blocked read.

This policy obtains a deterministic **result and reader-admission cut**, not
deterministic scheduling or a global deadline: the owner combines authoritative
episode/disposal results with the pre-seal recorded failure, closes reader
admission and seals, then reports/returns independently of a blocked stdin read.
No global ledger service, new playback lifecycle, or command-debt abstraction is
needed. “Host-result ledger”
is explanatory language for an invocation-local result record, not a new runtime
subsystem.

### 8.2 C2 merged realization (#206 / #213; D3/D5 CANDIDATE)

[Machine input][machine-input] keeps an ordinary invocation-local `HostInput`:
one admission-closed bit, one active-operation bit, and one optional fixed-size
`HostFailure::{Spawn, Read, Panic}`. There is one reader. The spawning host
records spawn failure synchronously when no reader exists. EOF returns normally.
Read errors and unwind panics enter the same host failure path; neither publishes
playback evidence or an episode outcome. The first admitted failure is retained;
static presentation text is rendered by the invocation owner after seal.

The concrete ordering points are:

| Point | Production realization |
| --- | --- |
| Reader-operation admission | `HostInput::admit` sets `operation_active` under the invocation mutex, only while admission is open |
| Failure recording | `Operation::fail_with_response` installs the first failure under that mutex; then releases it before checking the D11 terminal Fact and routing existing Stop if unsettled |
| Completion acknowledgement | `Operation::drop` clears the active bit and notifies the completion condition variable; the operation carries an obligation, never a mutex guard |
| Admission closure | After terminal wait and root disposal, `HostInput::seal` closes admission under the mutex; an already-admitted operation retains its right to record/finish |
| Final result seal | After the active operation acknowledges, `seal` reads the immutable first failure under the same mutex; this is the result linearization point |

Closing admission before draining prevents fresh operations from overtaking the
acknowledgement wait. No new failure or operation can enter during this drain;
an already-admitted operation may still record its failure before final seal.
The condition-variable wait releases the mutex. No host bookkeeping mutex spans
Stop, terminal wait, disposal, stdin reads, status formatting or output/flush.
A physically blocked OS read holds no active-operation obligation and is not
joined. On wake, command/report/failure admission is rejected after closure.
The command/output panic catcher runs **inside** its admitted operation so
unwinding cannot acknowledge before the panic record/Stop response. The outer
reader boundary also catches startup/read unwinds. Panic-abort remains outside
this recovery policy.

**Replaceable mechanism, not architectural semantics:** before spawning, the
process host configures Rust's panic hook once: suppress automatic hook diagnostics only on its uniquely owned
`qianqian-stdin` thread, whose catchers explicitly record the host failure;
delegate the prior hook for other thread names. Thread name plus process-wide hook
is the current implementation filter, not a frozen identity or required design. This ordinary runtime diagnostic filter carries no invocation ledger, playback handle or semantic
authority and holds no host bookkeeping lock. The wrapper is left installed after return because detached
readers may outlive the invocation; future process hook configuration is not governed by this local
policy. The semantic obligation is pre-seal admitted reader panic classification
and no post-seal reader invocation output, irrespective of replacement mechanism. Subprocess oracles use real `panic!` and capture stderr before/after seal and from an unrelated
thread; merely bypassing the hook with `resume_unwind` is insufficient.

[Entry][entry] obtains the real terminal and disposal results before sealing,
then emits final owner reports and calls `machine_exit_code`. Pre-seal host
failure contributes exit 1 independently of Completed/Stopped/Failed. Natural
Completed and runtime Failed retain their D11 truth and independent diagnostics;
post-seal failure cannot relabel the result. C1 establishment and other playback
protocols are unchanged. [Deterministic oracles][machine-input-tests] exercise
real Session/PCM/terminal/disposal paths with test Decode/Output mechanisms,
pre/post-admission cuts, acknowledgement waits and a physically blocked reader
thread. This is host-boundary evidence, not physical-device or process-quiescence
evidence. D3/D5 and the overall execution architecture remain CANDIDATE / NOT FROZEN.

### 8.3 Failure / cancellation domains

YES in the Fact column means **the designated Session authority may commit**
from that cause's evidence, not that the detecting unit may publish a terminal.
Host exit is presentation policy in [machine][machine], separate from semantic
classification. Cleanup may itself expose a K0 violation, never relabel a Fact.

| Cause / owner and truth class | May commit playback Fact? | May affect host exit? | Response / cleanup responsibility | Cannot relabel / owning authority and code |
| --- | --- | --- | --- | --- |
| Registration/desired admission refusal / K0 operation + assembly NotEstablished result | NO: no established episode | YES: registration 1/composition usage refusal 2; disposal presentation independent | Host disposes attempted root without terminal wait; reference failure-clean or fail-stop from verdict | Not D11 Failed; generic K0 success is not Established. D14.6; assembly `admission_refused`, entry unestablished branch, player `failure_clean_start` |
| Required provider activation failure or unresolved dependency / provider/K0 mechanism + whole-attempt absence | NO: Session activation not established | YES: failed establishment presentation | K0 unwinds acquired provider effects as needed; host disposes whole attempt | No Failed from Fiber Failed/Pending or missing diagnostic. K0 §F–G, D14.6; kernel `activate_fiber`, assembly `attempt.finish` |
| Session acquisition/processing compile/render open/decode spawn failure / Session activation diagnostic + NotEstablished | NO: activation failure stays distinct | YES: failed establishment | RAII and registered partial effects unwind; Output owns failed open before handoff; host disposes attempt | Published source/Position or opened render does not establish episode. D14.6/.11; Session `activate*`, `record_establishment`, Output `open_stream` |
| Runtime decode/provider seek MutatedThenFailed/caught decode panic / worker evidence to Session | YES: Session D11 Failed path | YES: real Failed reports exit 1 | Worker publishes failure before edge fail, exits; Session joins/retires resources | Not host infrastructure failure or proven inert refusal. D11/D14.5; Session `decode_worker`, completion `decode_failed` |
| Runtime processing failure / worker processing-origin evidence | YES: existing Session D11 Failed class | YES | Worker processing-failure publication then edge fail; no bypass/resurrection, normal inverse cleanup | Not decode-origin diagnostic or new terminal variant. D14.11, DSP §7.3; completion `processing_failed`, live `stage` |
| Live DSP validation/compile refusal / control diagnostic | NO from refusal alone | NO in current machine reader (typed DSP updates not wired there) | Preserve old engine; refuse coherent config, report diagnostic, no episode teardown required | Not Accepted, applied sound or processing Failed. DSP §7.3; live `update_desired`, `accept` |
| Runtime device/output abort or caught render panic / Output mechanism evidence to Session | YES: Session resolver may classify Failed or Stopped according to decisive evidence/recorded Stop; detector never commits itself | YES if real Failed or disposal unsuccessful | Render exits/stops input, publishes Aborted; decode exits; Session gate release + joins | Not unconditional Failed solely from drain Aborted, no acoustic claim. D11, PBK-003; WASAPI `run_render_thread`, completion `resolve` |
| User Stop / Session command history | YES: only real D11 Stopped/other winning predicate, not command itself | YES conditionally: real Stopped/Completed with quiet disposal succeeds; Failed/unclean disposal fails | Release gates and edge; owner consumes terminal then disposes | Cannot rewrite already decisive/committed Completed/Failed; not cancellation acknowledgement. D14.4; completion `request_stop`, machine `episode_exit_code` |
| Stdin spawn failure / spawning host infrastructure cause | NO: never forged playback Failed | YES: admitted host cause forces exit 1 | No reader exists; synchronously record first cause, existing Stop if unsettled, genuine terminal/disposal then seal | Cannot relabel natural Completed/Stopped/Failed. D3/D5 candidate; machine input `start_reader`, `failure` |
| Stdin read error / admitted reader infrastructure cause | NO | YES if admitted/recorded before final seal | Reader effect records before unlocked Stop, acknowledges and exits; host owns terminal/disposal/seal | Not EOF or playback diagnosis; physical occurrence before cut alone insufficient. D3; `read_input`, `Operation::fail` |
| Caught reader unwind panic / admitted host infrastructure cause | NO | YES if admitted before seal | Inside admitted output/dispatch catch, record/Stop before Drop ack; read/outer catches must acquire admission. Postclosure panic report inert; hook filter currently suppresses automatic reader diagnostics | No universal panic recovery/panic-abort guarantee; hook/name mechanism is replaceable. D3; `line`, `start_reader`, `install_reader_panic_hook` |
| Teardown/disposal violation / K0 authoritative verdict | NO new playback Fact from violation; existing D11 remains immutable | YES: non-success disposal report/exit | K0 latches open relation and blocks provider release; reference retains violated root/fail-stops. Machine consumes verdict/report and returns non-success; no new recovery | Not failed activation, clean quiescence or late terminal relabel. K0 §G.6; kernel unwind/dispose, player `latch_fail_stop`, entry disposal report |
| Late retained-handle command / caller intent or control history | NO new terminal or successor episode | NO by itself | Stop/Pause/Resume inert for terminal; Seek refuses; DSP/level may retain local history (§6); owner still joins resources | Cannot resurrect episode or certify all code stopped; terminal ≠ joins. D14.2/.4/.7/.11; handle/completion/live |
| Postclosure/post-seal host-input event / physically late reader event, rejected effect | NO | NO for sealed invocation | Reject command/report/failure admission; reader exits/drops local resources on wake | Cannot emit invocation reports, route old-handle commands or mutate sealed result. D3/D5; `admit`, `read_input`, `seal` |

C2's host response uses `observe().terminal_outcome` only as a **committed Fact
read**; it does not use `paused()`, Position, FiberState or diagnostics as a
correctness predicate. A racing terminal makes existing Stop semantically inert.
EOF creates no host failure; host failure and playback failure remain separate
even when both contribute non-success presentation.

## 9. Shutdown / quiescence / resource milestones — D5 (CANDIDATE)

This milestone model names observations and obligations, not new production
states. Inherited authority: D11, PBK-002 D6/D14.6, K0 §G.6, PBK-003 §5.
Code mapping: [session][session] activation relations/inverses, [completion][completion]
worker/evidence paths, [WASAPI][wasapi] stream stop/join, [kernel][kernel] unwind.
D3/D5's candidate host milestones are realized by merged C2.

| Milestone | Scope / owner | What it proves | What it does not prove | What becomes impossible / what remains possible |
| --- | --- | --- | --- | --- |
| Stop requested | Episode command / Session completion | Monotone intent recorded; response routed by protocol | Terminal Stopped, worker exit, drain or join | No relabeling of an already committed terminal; workers/render may still run, native waits and joins may remain. |
| Terminal committed | Episode semantic truth / Session | One immutable D11 outcome; decisive predicate evaluated without observer demand | Decode/render joined, edge memory reclaimed, audible completion | No later terminal relabeling; already-buffered/fetched PCM, render execution and teardown may remain until scoped stop/join. |
| Decode worker exited | Worker evidence / decode exit funnel | No further decode-loop iterations; while unsettled, worker-gone is published before stranded-seek cleanup. After terminal commit, publication may be suppressed | OS thread already returned, join completed, render stopped; worker-gone is not an unconditional postterminal acknowledgement | No new decode-loop work; exit funnel/destructors/thread return and render work may remain. Evidence can be suppressed postterminal. |
| Decode worker joined | Session relation / Session inverse | Thread returned; endpoint and worker-owned processing/staging resources dropped on this path | Render/device quiescence, stdin exit, success rather than failure | No future decode-thread work; render/device drain/stop/join and retained history cells may remain. |
| Render stop requested | Backend relation / Output-owned mechanism, Session requests stop | Stop invoked; eventual mechanism acknowledgement still needed | A Stop request alone does not prove device/worker returned; terminal drain evidence is distinct | No impossibility claim from stop request alone; render can still be draining or blocked until acknowledgement. |
| Render joined | Backend relation / stream `stop_and_join` | Render thread returned; backend-owned resources released on the exercised path | Physical acoustic silence on every device, reader exit, successful episode outcome | No future work from this render thread; retained storage, host reader and process work can remain. |
| K0 relation obligations discharged | Composition / K0 records inverse and teardown verdicts | Registered obligations discharged in lifecycle order; no provider final release past violated dependents | Universal thread discovery, stdin termination, correctness of an untested backend's discharge claim | No undischarged registered relation under conforming component contract; host/stdin and retained control allocations may remain. |
| Root disposed | Whole attempted composition / App calls K0 | **Only `DisposeVerdict::Discharged`** supports clean disposal; `TeardownViolated` retains violated world and fail-stops replacement | Clean exit from a mere `dispose()` call or quiet snapshot; host/process quiescence | Clean Discharged forbids remaining mounted obligations in that attempted root; violation instead keeps world open. Host seal/final I/O may remain. |
| Host input admission closed | Invocation effects / host `HostInput::seal` | `admission_closed = true` under bookkeeping lock rejects new effects | Final sealed result, finished admitted effects, cancelled/joined stdin | No fresh reader effect admission; a preclosure admitted effect/failure response may still finish/record before seal; blocked stdin may remain. |
| Host admitted operation drained | Invocation effects / `Operation::drop` + owner wait | Active bit cleared under mutex; condition-variable wait finishes after ack | Stdin returned; final owner reports completed; failure physical-event timestamp order | No admitted operation remains to act for this invocation; final seal/read and owner reporting may remain; stdin can remain blocked. |
| Host result sealed | Invocation result / machine host | D3 pre-seal recorded-failure cut and final classification fixed; reader command/failure-report/status-output admission closed | Reader ended, final owner reports delivered, process-wide quiescence | No reader command/report/failure or result relabeling; final owner output, detached blocked read and later local cleanup can remain. |
| Host function returned | Invocation caller boundary / host | Control returned with the sealed result (candidate D3) | Detached reader ended, all process threads joined | No further code in this host call; detached reader storage/I/O and other process work can remain. |
| Stdin reader ended | Host-input worker / reader, acknowledgement if owned/observed | No further reader work after actual return; observed join would acknowledge it | Episode completion, all host tasks ended; production currently has no such observation | No work from this returned reader; other invocation/process/episode resources may remain. |
| Process exited | Process / entrypoint + OS | Process execution ceased; OS reclaims process resources | Graceful protocol discharge, successful joins, delivery of final output | No process execution; OS resource reclamation is not graceful discharge evidence. |

**Episode quiescence ≠ host quiescence ≠ process quiescence.** For the current
established episode, an episode-quiescence claim requires decode and render
termination acknowledgements/joins and their resource release, plus discharged
Session relations and terminal settlement when decisive. Proof is scoped to these
registered obligations and conforming port contracts; it does not discover hidden
backend tasks or independently validate every platform. Claims about supported
playback are conditional on D1's proposed domain precondition becoming accepted:
reattachment is outside supported behavior under that precondition, but current
runtime does not structurally prevent it. D1 is not a structural quiescence proof.
Stop or terminal alone is insufficient. Retained handles may keep command/history
cells and even the bound stopped edge allocated;
quiescence does not mean every allocation has been reclaimed.

Current relation ordering: register render inverse first, decode inverse last;
LIFO unwind stops the edge and joins decode, then permanently suppresses further
episode gate routing/releases the render gate and stops/joins render. K0 blocks
provider final release until dependent obligations discharge. Failed stream-open
before handoff is Output's responsibility: [open-abort][open-abort] closes/releases
the gate, stops input, then joins, with no open-verdict lock held. These are
different ownership paths, not a second global shutdown mechanism.

A full host-quiescence claim would additionally require all invocation-owned
work (including reader and pending I/O) to have ended or been acknowledged.
D3 deliberately permits return without that claim. Process-wide quiescence
requires accounting for every process execution context, not only playback/K0.
Neither K0 `Discharged` nor host-result settlement proves the reader ended.
PBK-001's wider shutdown/unwind gaps are not silently closed by this machine-only
candidate policy.

## 10. Bounds / backpressure / liveness assumptions — D6 (CANDIDATE)

These are audited realization bounds at the stated SHA, not a global allocation
budget. Owners retain protocol policies in the authority table. Logical element
counts do not imply an allocator's exact byte capacity.

Ring-resident and already-fetched, not-yet-submitted source PCM are separate
reservoirs. A read frees ring space before submission; refill can leave a full
ring plus an outstanding fetched chunk. Each pull is bounded by resident frames
and its destination frame capacity, independently of the ring occupancy bound.
There is no aggregate single-ring-capacity guarantee (temporal §6.6), and these
source-side bounds do not bound already-submitted downstream/device/acoustic PCM.

| Structure | Bound | Overflow/busy policy | Waiting semantics | Owner / primary evidence |
| --- | --- | --- | --- | --- |
| PcmEdge | 8192 frames × episode channels; one fixed ring | Prefix write, then **BLOCK** via capacity retry; EOF drains, stopped/failed **DROP** remaining read eligibility | Consumer condvar may wait indefinitely while empty/Open; producer waits in 2 ms slices | Session lifetime, producer/consumer cursors under edge lock; session constants, edge |
| Seek slot + worker local pending | One reserved record across slot/pickup/release acknowledgement, not one per location; reservation alone is not semantic Accepted | **REFUSE** busy/ineligible; ending aborts | Worker/gate evidence slices; Applied cut may await tail indefinitely; next slot only after release consumption (or refusal/abort finish) | Completion admission + decode worker execution; completion/session |
| DSP pending | One complete config | **LATEST-WINS** before pickup; intrinsic or worker validation **REFUSE** unchanged accepted state | Pickup at whole unprocessed staging boundary, deferred during transition/seek handling | ProcessingControl / live |
| Active DSP transition | One old/new pair, scratch up to current staging block; frame count finite for episode rate | Complete-in-flight for ordinary updates; pending retains latest; Applied Seek resets, ending drops | Progress measured by processed samples, not wall-clock deadline | Decode-confined LiveProcessing; live `Transition`/`stage` |
| Staging buffer | 1024 frames × episode channels | Reused block; decoder must respect provided slice | Decode/native call may block; no end-to-end timeout | Decode worker / session |
| Remainder | At most unfinished suffix of one processed staging block | Preserve on RefusedUnchanged, discard on Applied/ending; no growing queue | May wait for edge capacity/park/seek resolution | Decode worker / session `write_observing_seek` |
| Local execution evidence | Fixed fields/cells: first terminal outcome, first worker failure, worker terminal, drain verdict, gate/cut evidence, source/position and last DSP refusal | Terminal/drain/failure **FIRST-WINS** as applicable; operation evidence reset for next cut; diagnostic replace; postterminal semantic writes **INERT** | Completion/gate/drain notifications and lock holds; observation pure | Completion, ports, PositionPublisher, ProcessingControl |
| Gate intent/release and stream-open verdict | Fixed intent flags + at most one seek release; one open verdict | Gate close makes later park requests **INERT**; release consumed once; verdict one attempt | Park slices 10 ms; open wait limit 10 s in WASAPI, abort then joins | RenderGate / Output mechanism, ports/wasapi/open-abort |
| Host first-failure record | One `Option<HostFailure>` with three fixed-size cause variants | **FIRST-WINS** admitted cause, further records do not grow; fresh postclosure failure **INERT** | Read by final seal after active effect ack; static report text rendered by owner | Machine HostInput / `fail_with_response`, `seal` |
| Host active effect | One boolean / one live `Operation`; one reader, spawn failure has none | No queue: new effects rejected after closure; active effect **BLOCK**s seal until ack | Condvar releases host mutex; blocked stdin itself carries no active obligation; output/flush may block active effect indefinitely | Machine HostInput / `admit`, `Operation::drop`, `seal` |


“Fixed evidence fields” bounds cardinality, **not diagnostic string bytes**.
Channel/rate-dependent processor histories, current configs and EQ band arrays
are bounded within a valid episode format/algorithm configuration; this is not
a global constant independent of format. Transition duration is currently 50 ms
of samples (minimum one frame), an implementation tuning value rather than new
authority. Decode/remainder processing is off the device submission loop.

No global fixed-memory guarantee covers stdin line size, playlist/path
collections, directory enumeration, metadata/artwork, diagnostics, native/backend
allocations, allocator overhead or other host collections. Their lack of a global
budget is not an architecture defect under the current authority. Retained
handles can prolong allocation lifetime. This inventory does not promise that
all host/native allocation is realtime-safe or bounded by the PCM ring.

**A bounded polling/wait slice is not an end-to-end deadline.** The 2 ms decode
slice, 10 ms gate slice, 100 ms WASAPI event wait, 10 s open-verdict wait and 5 s
drain loop cap are local mechanisms. OS scheduling, mutex acquisition, stdin,
decoder/native calls, device progress and joins can extend or prevent overall
completion. Open timeout is followed by abort/join, so it does not bound total
`open_stream` duration. Drain caps do not bound a blocking native API call.
Paused or seek-parked execution can intentionally wait for Resume/Stop/teardown
or tail evidence. No global operation deadline, fairness theorem or cross-platform
device-progress guarantee follows from these constants.


No guarantee makes blocked stdin return, joins it unconditionally, or bounds
native/device progress on every target. Closing reader admission removes future
invocation effects, not the physical wait or memory retained by its references.
PcmEdge/staging bounds do not bound backend device-buffer size or input media.

## 11. Observation boundary

PBK-001 §2.3 owns the firewall; PBK-002 D14.2/.7/.8 and temporal §6 own the
truth classes. Execution explains producers/readers, without promoting derived
visibility to authority.

| Read / result | Truth class / producing path | Legitimate consumer use / limitation |
| --- | --- | --- |
| `EstablishmentResult` | D14.6 authority-owned completed operation result, C1 transport | Machine/reference select wait versus failed-attempt disposal. Not a handle projection or new Fact; diagnostics optional |
| `observe().terminal_outcome`, `wait_terminal` | Already committed D11 Fact from Session publication | Consume terminal and App U2 policy; pending None means no committed terminal only. Wait/read never triggers settlement |
| Stop/pause/processing Desired/output level | Command/product-control history | Display requested values; no successful application, Paused, acoustic or successor-episode assertion |
| `paused()` / pause engagement | Derived Projection / underlying mechanism evidence | Product display only; no resume legality, resource release or control skip from Projection. No Resumed Projection |
| Position | One pure sample of render-published device-consumption evidence; withdrawn after terminal/activation failure | Display source-relative estimate; no decode/DSP-progress reconstruction, freshness/acoustic guarantee, or seek commit authority |
| Source format / source duration | Activation-relayed mechanism evidence; optional duration stays unknown | Display format/duration; not source identity or establishment truth. Duration may remain after terminal |
| Activation/failure/DSP-refusal diagnostic | Presentation or mechanism diagnostic separate from result/Fact | Explain refusal/failure; string existence/content is not classification. Source/Position/refusal reads do not make one universal snapshot |
| `DisposeOutcome.verdict` versus `snapshot` | K0 operation authority versus derived composition diagnostics | Authoritative verdict controls reference replacement/retention; snapshot supports warnings. Machine exit helpers presently consume `snapshot.quiet` as presentation input, never as establishment/terminal/next-root authorization |
| Sealed HostFailure / final exit | C2 invocation result versus presentation table | Cause can require nonzero exit while terminal remains Completed/Stopped/Failed. Exit, logs and missing diagnostics cannot reconstruct a playback Fact |

The completion record's locked fields coexist at one instant; Position and DSP
refusal use separate cells/locks and have no universal cross-cell freshness
contract. Retained observers keep references, not terminal/teardown authority.
Current callbacks publish bounded owner evidence; they are not a generic observer
fan-out in the PCM path. [Audio observation design][observation-design] remains
DRAFT/#187; its future PCM consumers and #188 UI read model are separately gated.
No observer-demand settlement, new Fact bus or cross-episode identity is earned
by this documentation. PBK-001 P1–P5 remains the trigger if future overlapping
published execution views require retirement/reclamation.

## 12. Minimum-core traceability — 20/20

The fixed identifiers/questions are from #198. All closure evidence needed to
inspect this core is in repository files below; old ZIPs, research reports and
chat history are not prerequisites. “Candidate” means a reviewable answer here,
not acceptance of D1–D6. C1/C2 correctives are merged; remaining decisions are
Stage-6 acceptance and optional future mechanisms, not unresolved runtime gaps. Code symbols are search targets, not a
promise that private representation remains frozen.

| Question | Owning authority | Primary code evidence | Current closure / remaining gap |
| --- | --- | --- | --- |
| EA-A01 — execution identity | D1 candidate; PBK-002 D11/D14.6 | [handle][handle] `new`/Clone; [assembly][assembly] `establish`; [session][session] `playback_session_spec_with_establishment`; entry `RealEpisodeSource::start`; kernel `activate_fiber` | Fresh episode core, shared clones; repeat attachment unsupported, not structurally forbidden |
| EA-B01 — mutable state and writers | D1/D4/§4; protocol owners | [completion][completion] state; [live][live] control/engine; [edge][edge] cursors; [wasapi][wasapi] locals; [machine input][machine-input] state (§4) | Named writer/lock/thread ownership; no inferred global state owner |
| EA-B02 — owner versus serializer versus authority | PBK-002 D6/D11; DSP §7.3; §4 | session inverses; completion `publish_evidence`; live `stage` | Session lifetime ownership, locks/program order serialization, designated semantic authority kept distinct |
| EA-C01 — command admission and ending fate | PBK-002 D14.4/.5/.7; DSP §7.3; D3/D4 | completion `request_*`; live setters/`poll_update`; machine input `HostInput`/`Operation`; entry reader | §6 and temporal §6.1 distinguish Accepted from non-Open Refused/Inert records; completion split-record/exit oracle + session real-worker/control oracle pin fate. D3/D4 remain candidate |
| EA-D01 — communication model | PBK-002 D8/D14.5; PBK-003 §5; D6 | edge; ports gate/drain; completion seek slot; live pending | Typed FIFO/cells/callbacks, no generic mailbox or PCM event bus |
| EA-E01 — local serialization | Temporal §3/§6; protocol owners; §4 | completion lock holds; live control lock; session worker; wasapi loop; machine input admission/record/ack/seal (§5/§7) | Local serial points identified; no universal command order |
| EA-E02 — acceptance/linearization | PBK-002 D14.5/.7; DSP §7.3; D2/D3 | completion `request_seek`/pause/stop; live setters/`accept`; player `replace_episode` | Seek plant is Accepted only with actual D14.5 eligibility there (temporal §6.1); exact ending witness and Open worker control distinguish it. Setter record differs from DSP Accepted; C1 establishment and C2 admission/ack/seal retain their boundaries (§7) |
| EA-E04 — in-flight ownership and debt | PBK-002 D14.5; DSP §7.3; D4 | seek slot/pending/release; live `Transition`; session remainder | Fate differs per protocol; no universal successful-application debt |
| EA-F01 — ordering | Temporal §3/§6; PBK-002 D14.5/.6; DSP §7.3 | session seek→invalidate→commit→release; player retirement; kernel unwind; machine input close→ack→seal (§7/§9) | Program/lock/FIFO/acknowledgement orders explicit; no global clock/order |
| EA-H01 — commit authority | PBK-001 §2.3; PBK-002 D11/D14.6 | completion `publish_evidence`/`resolve`; player replacement | Session commits terminal; App owns replacement; C1 carries whole-attempt Activated |
| EA-H02 — evidence/commit/visibility stages | Temporal §2–6; DSP §7.3 | completion evidence/observe; ports callbacks; live setter/pickup/stage | Fact lens preserved; no new Fact predicates or acceptance shortcut |
| EA-I01 — projection firewall | PBK-001 §2.3; PBK-002 D14.6/.8 | entry `start_episode`; handle `observe`; completion position gate | C1 consumes the Session activation attempt result; Fiber projection cannot classify establishment |
| EA-J01 — lifecycle relation | K0 §F–G; PBK-002 D6/D14.1; D1/D5 | kernel activate/unwind; session relations; completion core | One K0 lifecycle plus terminal Fact/ordinary resources; no second episode enum |
| EA-K01 — failure responsibility | PBK-002 D11/D14.6; PBK-003 §5; D3 | completion failure methods; session catch-unwind; wasapi/open-abort; machine input `HostInput`/`Operation`; entry reader | Failure domains separated; D3 requires record then existing Stop if established/unsettled, without forging Failed; merged C2 #213 realization (§8) |
| EA-K02 — failure domains | Same owners; D2/D3/D5 | player `failure_clean_start`/fail-stop; completion first failure; entry `finish_episode`; machine input `fail_with_response`, `seal` (§8) | Host failure cannot forge Failed; violation retains composition; no invented recovery |
| EA-O01 — boundedness | D6; DSP §7.3; protocol owners | session constants/staging/remainder; edge ring; live slots/transition; completion fields; HostInput `State` (§10) | Logical local bounds inventoried; diagnostic/native/host bytes not globally budgeted |
| EA-O02 — backpressure/busy policy | PBK-002 D14.5; DSP §7.3; D4/D6 | edge `write_some`/wait; completion seek refuse; live latest pending | BLOCK/REFUSE/LATEST-WINS/FIRST-WINS/DROP/INERT scoped to actual structures |
| EA-Q01 — shutdown order | K0 §G.6; PBK-002 D6/D14.6; PBK-003 §5; D5 | session inverse registration; kernel `run_unwind`; player retirement; open-abort | Episode relation ordering explicit; merged C2 realizes candidate host closure/ack/seal; detached reader separately scoped |
| EA-Q02 — quiescence proof | Same owners; D5 | decode/render JoinHandles; dispose verdict; completion terminal | Terminal ≠ joins; Discharged ≠ stdin end; host result ≠ process quiescence |
| EA-R01 — resource lifetime | PBK-002 D6; PBK-003 §5; D1/D5/§4 | session endpoint/stream effects; wasapi RAII; core retained Arcs; App roots | Allocation versus relation ownership explicit; retained history/storage can outlive execution |

## 13. Guarantee / assumption / non-guarantee index

| Class | Claim / scope | Source |
| --- | --- | --- |
| GUARANTEE (inherited) | Projection/read-side cannot authorize control/lifetime/semantic decisions; K0/PCM and Fact-authority firewalls remain intact | PBK-001 §1–2; PBK-002 D8/D11 |
| GUARANTEE (inherited) | Single designated D11 terminal authority; autonomous immutable commit, wait/observe pure | PBK-001 §2.3; PBK-002 D11 |
| GUARANTEE (inherited) | Whole fresh-composition Activated and old-side authoritative clearance; failure-clean disposal/fail-stop | PBK-002 D14.6 |
| GUARANTEE (inherited) | Seek mutation classification, protocol-local admission/cut predicate, refusal preservation, ending abort scope | PBK-002 D14.5 |
| GUARANTEE (inherited) | DSP latest pending/complete-in-flight within processing, worker Accepted versus Desired, Seek history semantics | DSP §7.3; PBK-002 D14.11 |
| GUARANTEE (inherited) | Composition inverse/discharge ordering blocks provider release past violation | K0 §G.6; realization kernel/session |
| ASSUMPTION | A claimed backend discharge/format/read contract is honored by its implementation; tests must target the actual boundary | PBK-003 §5; decode/output ports; not established by this docs audit |
| ASSUMPTION | OS schedules runnable work, native calls/device progress and blocking I/O eventually return where liveness is claimed | D6; concrete native/backend paths |
| ASSUMPTION (candidate precondition) | Assembly attaches one fresh core/spec to only one attempt | D1; current handle contract |
| NON-GUARANTEE | No general Actor runtime, universal mailbox ordering, playback-wide generation/epoch or new global registry | K0/PBK-002 current model; D1/§1 |
| NON-GUARANTEE | No global fixed-memory budget or global operation deadline; local bounds/slices are narrower | D6 |
| NON-GUARANTEE | Terminal Fact implies neither resource quiescence nor worker joins; Discharged implies neither host nor process quiescence | D5; D11/K0 scopes |
| NON-GUARANTEE | Source/Position/refusal observations have no universal cross-cell atomicity/freshness/acoustic guarantee | PBK-001; PBK-002 D14.8; temporal |
| NON-GUARANTEE | No unconditional blocked-stdin join or external output-delivery deadline; reader dispatch/report/output is forbidden after seal | D3 candidate |
| OPEN / CANDIDATE DECISION | D1 attachment clarification; optional structural enforcement unchosen | D1; no executable enforcement in this stage |
| OPEN / CANDIDATE DECISION | D2 common host representation and consumption of existing Activated; C1 merged #205; cross-protocol D2 acceptance still belongs to #209 | D2; #201 sequencing |
| OPEN / CANDIDATE DECISION | D3 recorded-failure cut, mandatory record-then-Stop and closed post-seal reader admission/output; C2 realization; decision remains candidate for architecture review | D3; #202 scope |
| OPEN / CANDIDATE DECISION | D4–D6 cross-protocol fate, quiescence and bounds scopes await independent review/freeze; inherited rules already retain their authority | This candidate |
| CURRENT REALIZATION (candidate policy) | One fixed-size first host cause; close→admitted-effect ack→seal; pre-seal failure contributes exit 1; no host mutex across playback/I/O | C2 #213; D3/§5/§7/§8 |
| CURRENT REALIZATION (replaceable mechanism) | Owned-reader automatic panic diagnostics filtered by thread-name/global hook; catchers implement host classification. Neither name nor hook is architectural identity/semantics | machine input `install_reader_panic_hook`; §8.2 |
| NON-GUARANTEE | Host result sealed ≠ blocked stdin returned/reader joined/host output delivered/process exited; no reader effects after seal | D3/D5; §8–10 |

No new runtime gap or accepted-authority contradiction is claimed by this audit.
The two campaign differentials C1/F1 and C2/F2 are corrected by merged #205
and #213 respectively; the candidate decisions await #209 acceptance. A concrete
counterexample must be classified as model/spec mismatch, implementation defect,
authority gap, or refinement/oracle gap before changing production or authority.
Generic K0 capability is not evidence of supported playback-core reattachment;
implementation constants are not new accepted global budgets. Historical OPEN
notes are read with their later accepted narrow amendments, not used to reopen
already-owned D14.6 or DSP §7.3 semantics.

### Review / validation handoff

The Stage-4 merge and this Stage-5 explicitness subject retain all fixed 20
questions in §12. C1/C2 are merged; D1–D6 remain CANDIDATE. Stage-5 review
checks wording, authority routing and the docs/comment-only diff; it is **not
#209 acceptance** and does not freeze architecture.

Before merge, #208's PR records `STAGE_5_BASE_SHA`, `STAGE_5_PR_HEAD_SHA`,
`REVIEW_SUBJECT_CANDIDATE_HEAD_SHA`, `EXECUTION_MODEL_BLOB_SHA` and the linked
authority blob identities. The PR head is an immutable **candidate**; it is not
necessarily the future merged main SHA. After owner review/merge, #208/#201
must bind `REVIEW_SUBJECT_MAIN_SHA` to the actual post-merge main and record
the execution-model blob present there. #209 must review that exact merged
subject rather than a later moving main. This document starts no Stage-6 work.

The former Stage-4 P3 routing/comment debt is addressed against the linked
authority and current realization. Optional structural one-shot attachment,
DSP applied/read-model identity and generic process shutdown remain unchosen
future work; Stage 5 authorizes none of them.

**Before Stage 7 architecture freeze begins, the separate execution-validation
owner #212 and its versioned contract must already be ready** (#198/#201 Stage 6.5). #212 freezes the contract in
`playback-execution-validation.md`; this candidate does not create that
future artifact. The contract covers:
architecture version under test; required platforms; deterministic/failure
oracles; mandatory versus earned stress/performance/model checking; PASS / FAIL /
INCONCLUSIVE; counterexample taxonomy; implementation defect versus authority
gap; and the authority-gap reopen protocol. This document does not execute that
campaign or weaken its gate. Validation after freeze precedes resuming dependent
#187/#188 work. No Windows/device/audible result is inferred from generic tests.

[198]: https://github.com/jnhu76/qianqian/issues/198
[201]: https://github.com/jnhu76/qianqian/issues/201
[202]: https://github.com/jnhu76/qianqian/issues/202
[pbk1]: ../adr/ADR-PBK-001.md
[pbk2]: ../adr/ADR-PBK-002.md
[pbk3]: ../adr/ADR-PBK-003.md
[k0-design]: composition-kernel-0-design.md
[k0-impl]: composition-kernel-0-implementation-adr.md
[temporal]: playback-temporal-semantics.md
[dsp]: dsp-product-model.md
[app]: ../../crates/qianqian-app/src/lib.rs
[entry]: ../../apps/headless/src/entry.rs
[machine]: ../../apps/headless/src/machine.rs
[player]: ../../apps/headless/src/player.rs
[handle]: ../../crates/qianqian-playback/src/handle.rs
[session]: ../../crates/qianqian-playback/src/session.rs
[completion]: ../../crates/qianqian-playback/src/completion.rs
[edge]: ../../crates/qianqian-playback/src/edge.rs
[live]: ../../crates/qianqian-playback/src/live.rs
[ports]: ../../crates/qianqian-audio-api/src/ports.rs
[kernel]: ../../crates/qianqian-composition/src/kernel.rs
[wasapi]: ../../crates/qianqian-output-wasapi/src/wasapi.rs
[open-abort]: ../../crates/qianqian-output-wasapi/src/open_abort.rs

[machine-input]: ../../apps/headless/src/machine_input.rs
[machine-input-tests]: ../../apps/headless/src/machine_input/tests.rs
[204]: https://github.com/jnhu76/qianqian/issues/204
[205]: https://github.com/jnhu76/qianqian/pull/205
[206]: https://github.com/jnhu76/qianqian/issues/206
[207]: https://github.com/jnhu76/qianqian/issues/207
[208]: https://github.com/jnhu76/qianqian/issues/208
[214]: https://github.com/jnhu76/qianqian/pull/214
[213]: https://github.com/jnhu76/qianqian/pull/213
[assembly]: ../../apps/headless/src/assembly.rs
[establishment]: ../../crates/qianqian-playback/src/establishment.rs
[observation-design]: audio-observation-plane.md
