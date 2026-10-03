# Playback execution model

```text
STATUS = CANDIDATE / NOT FROZEN
OWNER = #198
IMPLEMENTATION_UMBRELLA = #201
STAGE = #204 (C1 implementation; Stage-1 candidate #202/#203)
LIVE_MAIN_SHA = eb7993f82aa366764dcb4d0c62431e928af2892f
PRODUCTION_BEHAVIOR_CHANGE = C1_ONLY (#204)
C1_STARTED = YES
C2_STARTED = NO
```

This is the candidate cross-protocol execution architecture authority and
reading entry point. [#198][198] owns the diagnosis and gaps, [#201][201]
owns sequencing and freeze gates. [#202][202] supplied the Stage-1 candidate,
merged by #203; #204 implements only C1 against the main SHA above. The original
Stage-1 audit targeted `0377305c76476594d72a00ad8214c2892dd39ed9`;
its execution-relevant baseline was unchanged by the documentation merge.

**D1–D6 are proposed decisions for review, not accepted architecture.**
Statements marked inherited keep the force and scope of their linked accepted
authority. Statements marked current realization describe production evidence,
not an additional protocol definition. In particular, D2 inherits an existing
meaning; D3 proposes host-result policy that production does not yet implement.

The direction is state-machine owners, resource-confined workers, typed bounded
communication seams, local serialization/program order, explicit semantic
authority, and acknowledgement/quiescence boundaries. There is no proposed
general Actor runtime. A stateful struct is not thereby an actor, Plugin, or
independent execution owner.

## 1. Authority ownership and reading views

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

## 2. D1 — Episode execution identity and attachment (PROPOSED)

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
enforce the precondition structurally is a later explicitness choice, not Stage 1.

## 3. D2 — Whole fresh-composition establishment (inherited meaning; PROPOSED consumption)

**PBK-002 D14.6 already defines the authoritative new-episode activation result
`Activated` over the WHOLE fresh composition.** Required providers, dependency
resolution and Session activation all belong to that result. Stage 1 does not
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
→ dispose the attempted composition, seal/report activation failure without
waiting for a nonexistent terminal authority. Reference: probe first; old-side
clearance uses absent root or `Discharged`; only then create/establish fresh
world. On success install that episode; on failure perform D14.6 failure-clean
disposal or retain the violated root and fail-stop. Neither host derives the
choice from a read-side snapshot. Representation and placement are for C1;
the candidate does not itself prescribe an API spelling.

Stage-1 baseline evidence: [entry][entry] `start_episode` computed `activated`
from Session Fiber Active; this was the **C1/F1 differential**, corrected below. [player][player]
`StartAttempt`/`replace_episode` used refusal plus source-format/activation-error
evidence from synchronous reference assembly. That conjunction was realization
evidence, not permanent authority or a second establishment definition. C1 below
exposes/consumes D14.6's whole result in both assemblies. No additional runtime
defect is asserted by this comparison.

### C1 production representation (#204; no semantic amendment)

Machine and reference now call the same headless `assembly::establish` helper.
It drives the fresh Decode/Output/Session desired composition synchronously,
then consumes the `EstablishmentAttempt` paired with
`playback_session_spec_with_establishment`. `EstablishmentResult::Established`
represents the existing D14.6 `Activated`; `NotEstablished { diagnostic }`
represents its absence. Representation != semantics; Projection != authority.

The Session activation operation alone writes that return slot, after all its
acquisition steps and inverse registrations succeed. In this supported fresh
wiring, K0 can enter Session activation only after both required providers have
activated and committed bindings. Provider failure or unresolved dependency
therefore leaves the attempt `NotEstablished { diagnostic: None }`; Session
failure records `NotEstablished` with presentation text. Source publication,
render open, generic admission success and terminal timing cannot produce or
revoke `Established`. Assembly admission refusal is handled separately as an
unsuccessful attempt. The root still requires authoritative disposal on failure.

Placement comparison: a generic K0 result would unnecessarily widen the kernel;
a handle observation would repeat the forbidden read-side reconstruction; an
app-local result cannot receive the private Session activation return without a
cross-crate seam. The paired constructor/consume-only attempt is that minimal
seam. It certifies the required episode in the current three-Plugin wiring, not
arbitrary extra Plugins. Fresh-core/single-attempt remains a precondition; no
reattachment enforcement, second lifecycle or terminal rule is added. The result
survives immediate D11 settlement and disposal. C2 remains unimplemented and the
overall execution architecture remains CANDIDATE / NOT FROZEN.

## 4. D3 — Machine input and host-result settlement (PROPOSED)

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

### 4.1 HOST_RESULT_SETTLEMENT_BOUNDARY

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
No Stage-1 claim is made that production currently enforces these rules.

### 4.2 Detached reader and deterministic exit

The reader may remain blocked in stdin after settlement; there is no unconditional
join. On its next wake/run after seal, it **only exits and releases reader-local
resources**. Returned input or read errors are discarded; it does not dispatch
old-handle commands, format status, report failure, emit diagnostics, or start
another read. An otherwise valid old-handle command is still refused by this
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
subsystem. C2 must realize this ordering and failure reporting; representation
is deliberately unimplemented here.

Current realization: spawn's `Result<JoinHandle<_>>` is retained only as `_control`,
read errors break like EOF, panic is unobserved, and `finish_episode` has no input
failure record/seal. This is the known **C2/F2 differential**. The later C2 oracle
must distinguish pre-seal versus post-seal recording, including each terminal
race, verify record-before-Stop responsibility, and forbid reader dispatch/output
after seal; it must not assert “every reader failure ever produces nonzero”.

## 5. D4 — Outstanding work fate

Inherited protocol-specific semantics, with D1 attachment and D3 host settlement
proposed above. “Must settle” means the stated protocol obligation, not a common
command-debt object or a requirement that every intent become successfully
applied. Ending below means the relevant protocol's stop/teardown/terminal
conditions; it is not a second episode lifecycle enum.

| Work | Ingress and record point | Acceptance/linearization; busy policy | Ending/close policy and fate | Authority/code evidence |
| --- | --- | --- | --- | --- |
| Seek | `handle.request_seek`; command slot under completion/slot locks | Atomic second eligibility check + plant one slot + reset cut evidence/route hold; **REFUSE** if ineligible or one in flight | Accepted seek resolves by refusal preservation, cut commit/release consumption, or abort on episode ending. No promise of successful cut after Stop. Refused remainder must resume exactly; Applied discards old remainder/edge/history. No coalescing. Only ending may abandon an Applied pending cut; stranded slot is aborted on worker exit | PBK-002 D14.5; [completion][completion] `request_seek`, `seek_cutover_decision`, `abort_stranded_seek`; [session][session] `decode_worker`, `write_observing_seek` |
| Pause | `request_pause`; intent history under completion lock | Record/route intent atomically relative to stop/teardown; gate engages at render loop top. Repeated intent is idempotent, not a queued debt | Ending leaves history but suppresses new routing; teardown releases gate. No required Paused projection before ending, no join implied. Pause preserves processing history/resources | PBK-002 D14.7; completion `request_pause`, `release_pause_gate`; [ports][ports] gate |
| Resume | `request_resume`; clear intent under completion lock | Release routed gate; no Resumed Fact or acceptance acknowledgement debt | Release remains useful for wake/teardown; terminal history cannot restart execution. No coalesced mailbox or mandatory observer update | PBK-002 D14.7; completion `request_resume` |
| Stop | `request_stop`; monotone intent under completion lock | Infallible/idempotent record, then edge/gate response; late Stop cannot relabel terminal | Recorded intent may survive only as history. If still unsettled it participates in D11's decisive predicate; Stop must not be confused with terminal/join acknowledgement. EOF already reached remains drain-to-Completed absent failure | PBK-002 D11/D14.4/D14.7; completion `request_stop`, `resolve` |
| DSP Desired update | Setter composes/validates under `ProcessingControl` lock, commits Desired/pending | Setter success records coherent intrinsically valid Desired, **not** semantic Accepted; invalid request refuses unchanged | Desired is retained control history; no duty that each value is accepted/applied. Late retained setters may change local history without an active episode result | DSP §7.3; [live][live] `ProcessingControl` setters |
| DSP pending update | One pending complete config | **LATEST-WINS** before pickup; worker pickup validates/compiles for episode format to become Accepted | May be superseded or remain unapplied at ending; no successful-settlement debt or forced synthetic PCM. Refusal diagnostic may survive as history | DSP §7.3; live `take_pending`, `poll_update`, `accept` |
| Active DSP transition | Worker creates accepted target transition, starts Applied on a whole staging block | One transition; complete-in-flight before another pickup during continuing processing; sample-driven progression | Normal newer Desired does not preempt it. Applied Seek invalidates/rebuilds accepted target fresh; Refused Seek preserves it. Ending/EOF may drop an incomplete transition; current R0 has no artificial output-tail duty | DSP §7.3; PBK-002 D14.11; live `stage`, `invalidate_signal_history` |
| PCM buffered frames | Producer writes bounded edge FIFO; render pulls | Prefix write until capacity, **BLOCK** by producer wait/retry on full, not latest-wins | EOF preserves buffered drain; stopped/failed edge refuses reads before buffer drain, so frames may be abandoned. With the leg parked, provider Applied precedes history/edge purge; production then waits for the cut's tail-quiescence/commit and release consumption. Tail-quiescence is a commit condition, not a pre-purge condition. Refused Seek preserves frames exactly | PBK-002 D14.5; PBK-003 §5; [edge][edge] `write_some`, `read_frames`, `invalidate` |
| Late retained-handle commands | Same episode's retained core/control cells | Existing protocol methods only, no new attachment | Stop/Pause/Resume history inert for terminal semantics; Seek refuses. DSP/level histories may still mutate; no resurrection or successor binding. Terminal alone does not prove all worker code has stopped; joins do | PBK-002 D14.2/D14.4/D14.7/D14.9/D14.11; [handle][handle], completion/live |
| Stdin unread input | OS stdin/line reader, no playback record until parsed command dispatched | Reader's local line order only; not a universal mailbox; no bounded input admission promised. D3 seal closes command/failure-report/status-output admission | EOF closes normally; unread input may be abandoned. After seal, the reader only exits/releases local resources on wake, without dispatch or output; no duty to consume every line. Infrastructure failures recorded before the seal enter the host result and require record-then-Stop for an established unsettled episode | D3 candidate; [entry][entry] `machine_transport` |

Refusal and abort are different from a successful semantic result. In particular,
an accepted seek awaiting device-tail quiescence can remain pending indefinitely
without new tail evidence, failure, Stop or teardown (PBK-002 D14.5). No new
timeout is introduced to make the table look uniformly settled.

## 6. D5 — Shutdown and quiescence scopes

This milestone model names observations and obligations, not new production
states. Inherited authority: D11, PBK-002 D6/D14.6, K0 §G.6, PBK-003 §5.
Code mapping: [session][session] activation relations/inverses, [completion][completion]
worker/evidence paths, [WASAPI][wasapi] stream stop/join, [kernel][kernel] unwind.
The host-result milestone is D3's proposal.

| Milestone | Scope / owner | What it proves | What it does not prove |
| --- | --- | --- | --- |
| Stop requested | Episode command / Session completion | Monotone intent recorded; response routed by protocol | Terminal Stopped, worker exit, drain or join |
| Terminal committed | Episode semantic truth / Session | One immutable D11 outcome; decisive predicate evaluated without observer demand | Decode/render joined, edge memory reclaimed, audible completion |
| Decode worker exited | Worker evidence / decode exit funnel | No further decode-loop iterations; while unsettled, worker-gone is published before stranded-seek cleanup. After terminal commit, publication may be suppressed | OS thread already returned, join completed, render stopped; worker-gone is not an unconditional postterminal acknowledgement |
| Decode worker joined | Session relation / Session inverse | Thread returned; endpoint and worker-owned processing/staging resources dropped on this path | Render/device quiescence, stdin exit, success rather than failure |
| Render stopped | Backend relation / Output-owned mechanism, Session requests stop | Stop invoked; eventual mechanism acknowledgement still needed | A Stop request alone does not prove device/worker returned; terminal drain evidence is distinct |
| Render joined | Backend relation / stream `stop_and_join` | Render thread returned; backend-owned resources released on the exercised path | Physical acoustic silence on every device, reader exit, successful episode outcome |
| K0 relation obligations discharged | Composition / K0 records inverse and teardown verdicts | Registered obligations discharged in lifecycle order; no provider final release past violated dependents | Universal thread discovery, stdin termination, correctness of an untested backend's discharge claim |
| Root disposed | Whole attempted composition / App calls K0 | **Only `DisposeVerdict::Discharged`** supports clean disposal; `TeardownViolated` retains violated world and fail-stops replacement | Clean exit from a mere `dispose()` call or quiet snapshot; host/process quiescence |
| Host result settled | Invocation result / machine host | D3 pre-seal recorded-failure cut and final classification fixed; reader command/failure-report/status-output admission closed | Reader ended, final owner reports delivered, process-wide quiescence |
| Host function returned | Invocation caller boundary / host | Control returned with the sealed result (candidate D3) | Detached reader ended, all process threads joined |
| Stdin reader ended | Host-input worker / reader, acknowledgement if owned/observed | No further reader work after actual return; observed join would acknowledge it | Episode completion, all host tasks ended; production currently has no such observation |
| Process exited | Process / entrypoint + OS | Process execution ceased; OS reclaims process resources | Graceful protocol discharge, successful joins, delivery of final output |

**Episode quiescence ≠ host quiescence ≠ process quiescence.** For the current
established episode, an episode-quiescence claim requires decode and render
termination acknowledgements/joins and their resource release, plus discharged
Session relations and terminal settlement when decisive. Claims about supported
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

## 7. D6 — Bounds, backpressure and liveness scope

These are audited realization bounds at the stated SHA, not a global allocation
budget. Owners retain protocol policies in the authority table. Logical element
counts do not imply an allocator's exact byte capacity.

| Structure | Bound | Overflow/busy policy | Waiting semantics | Owner / primary evidence |
| --- | --- | --- | --- | --- |
| PcmEdge | 8192 frames × episode channels; one fixed ring | Prefix write, then **BLOCK** via capacity retry; EOF drains, stopped/failed **DROP** remaining read eligibility | Consumer condvar may wait indefinitely while empty/Open; producer waits in 2 ms slices | Session lifetime, producer/consumer cursors under edge lock; session constants, edge |
| Seek slot + worker local pending | One in-flight operation across slot/pickup/release acknowledgement, not one per location | **REFUSE** busy/ineligible; ending aborts | Worker/gate evidence slices; Applied cut may await tail indefinitely; next slot only after release consumption (or refusal/abort finish) | Completion admission + decode worker execution; completion/session |
| DSP pending | One complete config | **LATEST-WINS** before pickup; intrinsic or worker validation **REFUSE** unchanged accepted state | Pickup at whole unprocessed staging boundary, deferred during transition/seek handling | ProcessingControl / live |
| Active DSP transition | One old/new pair, scratch up to current staging block; frame count finite for episode rate | Complete-in-flight for ordinary updates; pending retains latest; Applied Seek resets, ending drops | Progress measured by processed samples, not wall-clock deadline | Decode-confined LiveProcessing; live `Transition`/`stage` |
| Staging buffer | 1024 frames × episode channels | Reused block; decoder must respect provided slice | Decode/native call may block; no end-to-end timeout | Decode worker / session |
| Remainder | At most unfinished suffix of one processed staging block | Preserve on RefusedUnchanged, discard on Applied/ending; no growing queue | May wait for edge capacity/park/seek resolution | Decode worker / session `write_observing_seek` |
| Local execution evidence | Fixed fields/cells: first terminal outcome, first worker failure, worker terminal, drain verdict, gate/cut evidence, source/position and last DSP refusal | Terminal/drain/failure **FIRST-WINS** as applicable; operation evidence reset for next cut; diagnostic replace; postterminal semantic writes **INERT** | Completion/gate/drain notifications and lock holds; observation pure | Completion, ports, PositionPublisher, ProcessingControl |
| Gate intent/release and stream-open verdict | Fixed intent flags + at most one seek release; one open verdict | Gate close makes later park requests **INERT**; release consumed once; verdict one attempt | Park slices 10 ms; open wait limit 10 s in WASAPI, abort then joins | RenderGate / Output mechanism, ports/wasapi/open-abort |

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

## 8. Execution-owner and resource inventory

An entry can be a semantic owner, execution context or owned communication
resource; these are not additional Plugins. Session core and SessionCompletion
are the semantic role and its realization, **not two terminal authorities**.
All “current” entries map to production at the audited SHA; D3 settlement/failure
responsibility remains proposed.

### 8.1 App / host

| Attribute | Current realization / candidate obligation |
| --- | --- |
| Identity; lifetime; lifetime owner | One QianqianApp composition root per attempted episode; machine invocation or longer-lived ReferencePlayer owns roots and host collections |
| Execution context; state; writers | Serialized caller/control thread; desired graph, current root/handle, playlist/navigation/volume and failure-clean disposition written by host methods |
| Inputs; outputs | User commands/probe/source and committed terminal observations → fresh assembly/replacement, disposal, status and exit result |
| Waiting seams; serialization | Synchronous probe/activation, terminal condvar and disposal joins; mutable host API/program order serializes Open and replacement |
| Semantic authority; failure responsibility | PBK-002 D14.6 App Open/replacement and U2 policy; no D11 authority. Owns assembly/admission/probe failures, fail-stop retention and proposed D3 result |
| Stop/cancellation; termination ack; resources | Stop old unsettled episode then wait/dispose; authoritative disposition `Discharged` or violation. Owns root/host collections; reader termination is separately scoped |

Evidence: entry `start_episode`, `RealEpisodeSource`; player `replace_episode`,
`retire_old_episode`, `failure_clean_start`, `quit`; [App][app] and kernel disposal.

### 8.2 Playback Session semantic core

| Attribute | Current realization / candidate obligation |
| --- | --- |
| Identity; lifetime; lifetime owner | D1 one fresh handle/core for one attempt/episode; App owns attachment/root; retained clones extend storage lifetime |
| Execution context; state; writers | No dedicated Session thread. Completion operations run on command callers, decode exit/failure paths and render evidence callbacks |
| Inputs; outputs | Commands + designated worker/drain/gate evidence → command history, seek decisions, immutable terminal Fact and read-side visibility |
| Waiting seams; serialization | Completion mutex/condvar; atomic predicate/commit holds and worker/render program order, not mailbox order |
| Semantic authority; failure responsibility | D11 single episode terminal authority; routes mechanism failures into D11 predicates, activation failure remains distinct |
| Stop/cancellation; termination ack; resources | Stop/teardown release routed waiting; terminal and worker joins are separate acknowledgements. Owns episode relation lifetime for endpoint/worker/edge/render, not provider mechanisms |

Evidence: handle/session/completion. Ownership authority: PBK-002 D6/D11/D14.2.

### 8.3 SessionCompletion realization

| Attribute | Current realization / candidate obligation |
| --- | --- |
| Identity; lifetime; lifetime owner | Shared completion Arc of D1 core; handles/activation closures/callbacks hold references, evidence callbacks use weak core references |
| Execution context; state; writers | CompletionState under mutex: stop/pause, activation/source evidence, first failure/terminal, worker/gate/cut fields, bound stop target. Command methods and worker/render callbacks are permitted writers |
| Inputs; outputs | Typed calls/evidence → gated routing, seek slot/release, notifications, observation; no PCM dispatch |
| Waiting seams; serialization | `publish_evidence` resolves/commits under one hold; `wait_terminal` only condvar-waits. Seek revalidates under completion+slot lock; edge checks do not hold nested completion lock |
| Semantic authority; failure responsibility | Implements Session authority, never an independent Fact owner; first worker failure diagnostic retained. Exact resolver precedence is current implementation detail subject to D11's external propositions, not a frozen universal arrival order |
| Stop/cancellation; termination ack; resources | Monotone Stop and teardown routing suppression; while unsettled, worker-gone published before stranded-slot abort. Postterminal publication is suppressed. Holds gate/drain/slot/Position/control cells and potentially retained stopped edge; no thread to join itself |

Evidence: completion `publish_evidence`, `wait_terminal`, `request_seek`,
`worker_exited`, `resolve`; ports synchronous observers outside signal locks.

### 8.4 Decode worker

| Attribute | Current realization / candidate obligation |
| --- | --- |
| Identity; lifetime; lifetime owner | One spawned worker of established Session; Session's registered inverse owns its JoinHandle and endpoint relation |
| Execution context; state; writers | Worker thread exclusively owns decoder endpoint, processing instances, staging/remainder, local pending seek; decoder reads/seeks and LiveProcessing progression confined to it |
| Inputs; outputs | Source frames, seek pickup, latest DSP Desired → processed PCM, actual seek landing, failure/EOF and worker-gone evidence |
| Waiting seams; serialization | Blocking provider reads/seeks; edge capacity 2 ms retries; gate/cut/release evidence waits. Provider seek → classification → history/edge invalidation → landing/cut commit → release consumption → new production |
| Semantic authority; failure responsibility | Mechanism executor, not independent D11 authority; reports decode/processing/panic evidence to Session before edge failure propagation |
| Stop/cancellation; termination ack; resources | Edge stop/episode-ending predicates; catch-unwind exit funnel calls worker-exit publication then stranded abort (already-terminal publication may be skipped); join in inverse acknowledges actual return. Endpoint/processing/staging drop with worker |

Evidence: session `decode_worker`, `write_observing_seek`; completion
worker/failure methods. PBK-002 D14.5/D14.11 own ordering and seek/DSP interaction.

### 8.5 LiveProcessing / ProcessingControl

| Attribute | Current realization / candidate obligation |
| --- | --- |
| Identity; lifetime; lifetime owner | Episode-owned processor/control resource; Session binds it, decode worker owns live engine; handle may retain control after engine ends |
| Execution context; state; writers | Setters write Desired/pending/last refusal under control lock; decode worker alone writes accepted engine, old/new transition, elapsed frames and scratch |
| Inputs; outputs | Full desired config + episode format + staging block → worker Accepted/Applied processing and refusal diagnostic; no separate thread/mailbox |
| Waiting seams; serialization | Single latest pending slot; pickup deferred during active transition; whole-block boundary and decode program order govern application |
| Semantic authority; failure responsibility | DSP §7.3 configuration/transition semantics; no terminal authority; processing error reports Session failure evidence |
| Stop/cancellation; termination ack; resources | No independent cancel/join: ending/worker drop disposes engine; Applied Seek rebuilds fresh accepted target. Owns processor histories/transition scratch; no K0 identity |

Evidence: live `ProcessingControl`, `LiveProcessing`, `poll_update`, `stage`,
`invalidate_signal_history`; DSP §7.3 and PBK-002 D14.11.

### 8.6 PcmEdge producer / consumer

| Attribute | Current realization / candidate obligation |
| --- | --- |
| Identity; lifetime; lifetime owner | One prebound episode PCM resource; Session owns relation, shared references may outlive joined workers |
| Execution context; state; writers | No thread of its own; decode producer writes samples, render consumer reads; cursor/size/terminal under edge mutex; stop/failure writers per protocol, only worker purges on Applied Seek |
| Inputs; outputs | Typed frames/close/stop/fail/purge → FIFO frames or EOF/Stopped/Failed pull results |
| Waiting seams; serialization | Data/space condvars; bounded ring; first terminal wins; read checks Stopped/Failed before buffered drain; EOF drains |
| Semantic authority; failure responsibility | Mechanism observation, not semantic truth; failure/EOF participates through Session evidence, not edge state alone |
| Stop/cancellation; termination ack; resources | Stop wakes data/space; no thread acknowledgement in edge terminal flag. Holds ring allocation until references drop |

Evidence: edge; session prebinds producer/consumer, PBK-002 D8/D14.5.

### 8.7 Render worker / device seam

| Attribute | Current realization / candidate obligation |
| --- | --- |
| Identity; lifetime; lifetime owner | One backend render stream per current Session relation; Output owns backend allocation mechanism, Session owns returned stream lifetime |
| Execution context; state; writers | WASAPI render thread owns COM/device/event objects, handed-off basis, tail probes and volume application; gate receives routed intent, render produces engagement/position/drain evidence |
| Inputs; outputs | Prebound PCM/gate/level → device submissions, actual queued-tail evidence, Position estimate, drain/failure callbacks |
| Waiting seams; serialization | Loop-top gate before buffer reservation; event waits/native calls/PCM pull; single render writer for Position, release payload consumed before next seek is admissible |
| Semantic authority; failure responsibility | Backend mechanism evidence only; Session D11 commits terminal. Output owns open failure/timeout cleanup before stream handoff; runtime failure published via owner callback |
| Stop/cancellation; termination ack; resources | Session releases gate before stop/join; open-abort closes gate before joining. Drain evidence is not join; returned stream join acknowledges thread/resources. Concrete backend is not another Plugin |

Evidence: ports, wasapi `open_stream`, `run_render_thread`, `WasapiStream`,
`open_and_run`, open-abort; PBK-003 §5/§8. Windows runtime/device evidence was
not executed by this documentation audit.

### 8.8 Machine stdin reader

| Attribute | Current realization / candidate obligation |
| --- | --- |
| Identity; lifetime; lifetime owner | Named `qianqian-stdin` thread per machine transport; host creates it, currently may detach; retained handle fixes its episode scope |
| Execution context; state; writers | Reader thread owns stdin lock/line iterator/current line; host currently ignores spawn/join result. D3 proposes reader reporting and owner sealing one local result record |
| Inputs; outputs | Before seal, stdin lines → commands/status/diagnostics; EOF normal closure; admitted spawn/read/caught-panic failures recorded then Stop routed if established/unsettled. After seal, no reader dispatch/report/output |
| Waiting seams; serialization | Blocking stdin and output I/O, local line order; no unconditional join or global input bound. D3 orders seal against admitted dispatch/output/failure response; OS stdin read is outside that ordering |
| Semantic authority; failure responsibility | Host infrastructure/result only; no D11 authority. Reader reports read/caught-panic before seal; spawning host records spawn failure synchronously; late failures discarded without output |
| Stop/cancellation; termination ack; resources | EOF/failure/return ends reader; seal closes admission but does not cancel blocked stdin. Next post-seal wake only exits/releases local resources. Actual return/join would acknowledge end; retained handles/stdin lock may survive host return |

Evidence: entry `machine_transport`; current missing failure ownership/settlement
is C2, not an implemented guarantee.

### 8.9 Observation / read-side consumers

| Attribute | Current realization / candidate obligation |
| --- | --- |
| Identity; lifetime; lifetime owner | Host/UI/status readers, not automatically Plugins; caller owns subscription/poll lifetime and retained references |
| Execution context; state; writers | Caller contexts build display/navigation projections; completion/Position/control producers own underlying truth/evidence |
| Inputs; outputs | Pure `observe`, terminal wait, source/position/diagnostic reads → display/status and separately authorized App policy decisions from committed terminal Facts |
| Waiting seams; serialization | `wait_terminal` condvar does not resolve outcome; observations can be stale/cross-cell; no universal snapshot freshness |
| Semantic authority; failure responsibility | No playback authority. PBK-001 firewall forbids projection-based correctness decisions; App U2 may consume already committed Completed |
| Stop/cancellation; termination ack; resources | Reader cessation releases references, not episode resources by itself. Host reader failures belong to that host; no observer-demand terminal commitment |

Evidence: handle/completion observation; player `completed_fact`/`poll_eof_policy`;
entry status. PBK-001 §2.3 and temporal §6 own the firewall/vocabulary.

## 9. Minimum-core traceability — 20/20

The fixed identifiers/questions are from #198. All closure evidence needed to
inspect this core is in repository files below; old ZIPs, research reports and
chat history are not prerequisites. “Candidate” means a reviewable answer here,
not a shipped or accepted corrective. Code symbols are search targets, not a
promise that private representation remains frozen.

| Question | Owning authority | Primary code evidence | Current closure / remaining gap |
| --- | --- | --- | --- |
| EA-A01 — execution identity | D1 candidate; PBK-002 D11/D14.6 | handle `new`/Clone; session `playback_session_spec*`; entry `RealEpisodeSource::start`; kernel `activate_fiber` | Fresh episode core, shared clones; repeat attachment unsupported, not structurally forbidden |
| EA-B01 — mutable state and writers | D1/D4/§8; protocol owners | completion state; live control/engine; edge cursors; wasapi render locals | Named writer/lock/thread ownership; no inferred global state owner |
| EA-B02 — owner versus serializer versus authority | PBK-002 D6/D11; DSP §7.3; §8 | session inverses; completion `publish_evidence`; live `stage` | Session lifetime ownership, locks/program order serialization, designated semantic authority kept distinct |
| EA-C01 — command admission and ending fate | PBK-002 D14.4/.5/.7; DSP §7.3; D3/D4 | completion `request_*`; live setters/`poll_update`; entry reader | Protocol-specific table complete; D3 seal closes reader admission/output, realization C2 pending |
| EA-D01 — communication model | PBK-002 D8/D14.5; PBK-003 §5; D6 | edge; ports gate/drain; completion seek slot; live pending | Typed FIFO/cells/callbacks, no generic mailbox or PCM event bus |
| EA-E01 — local serialization | Temporal §3/§6; protocol owners; §8 | completion lock holds; live control lock; session worker; wasapi loop | Local serial points identified; no universal command order |
| EA-E02 — acceptance/linearization | PBK-002 D14.5/.7; DSP §7.3; D2/D3 | completion `request_seek`/pause/stop; live setters/`accept`; player `replace_episode` | Setter record distinct from DSP Accepted; host establishment uses C1 result; host settlement remains C2 |
| EA-E04 — in-flight ownership and debt | PBK-002 D14.5; DSP §7.3; D4 | seek slot/pending/release; live `Transition`; session remainder | Fate differs per protocol; no universal successful-application debt |
| EA-F01 — ordering | Temporal §3/§6; PBK-002 D14.5/.6; DSP §7.3 | session seek→invalidate→commit→release; player retirement; kernel unwind | Program/lock/FIFO/acknowledgement orders explicit; no global clock/order |
| EA-H01 — commit authority | PBK-001 §2.3; PBK-002 D11/D14.6 | completion `publish_evidence`/`resolve`; player replacement | Session commits terminal; App owns replacement; C1 carries whole-attempt Activated |
| EA-H02 — evidence/commit/visibility stages | Temporal §2–6; DSP §7.3 | completion evidence/observe; ports callbacks; live setter/pickup/stage | Fact lens preserved; no new Fact predicates or acceptance shortcut |
| EA-I01 — projection firewall | PBK-001 §2.3; PBK-002 D14.6/.8 | entry `start_episode`; handle `observe`; completion position gate | C1 consumes the Session activation attempt result; Fiber projection cannot classify establishment |
| EA-J01 — lifecycle relation | K0 §F–G; PBK-002 D6/D14.1; D1/D5 | kernel activate/unwind; session relations; completion core | One K0 lifecycle plus terminal Fact/ordinary resources; no second episode enum |
| EA-K01 — failure responsibility | PBK-002 D11/D14.6; PBK-003 §5; D3 | completion failure methods; session catch-unwind; wasapi/open-abort; entry reader | Failure domains separated; D3 requires record then existing Stop if established/unsettled, without forging Failed; C2 pending |
| EA-K02 — failure domains | Same owners; D2/D3/D5 | player `failure_clean_start`/fail-stop; completion first failure; entry `finish_episode` | Host failure cannot forge Failed; violation retains composition; no invented recovery |
| EA-O01 — boundedness | D6; DSP §7.3; protocol owners | session constants/staging/remainder; edge ring; live slots/transition; completion fields | Logical local bounds inventoried; diagnostic/native/host bytes not globally budgeted |
| EA-O02 — backpressure/busy policy | PBK-002 D14.5; DSP §7.3; D4/D6 | edge `write_some`/wait; completion seek refuse; live latest pending | BLOCK/REFUSE/LATEST-WINS/FIRST-WINS/DROP/INERT scoped to actual structures |
| EA-Q01 — shutdown order | K0 §G.6; PBK-002 D6/D14.6; PBK-003 §5; D5 | session inverse registration; kernel `run_unwind`; player retirement; open-abort | Episode relation ordering explicit; host seal proposed, detached reader separately scoped |
| EA-Q02 — quiescence proof | Same owners; D5 | decode/render JoinHandles; dispose verdict; completion terminal | Terminal ≠ joins; Discharged ≠ stdin end; host result ≠ process quiescence |
| EA-R01 — resource lifetime | PBK-002 D6; PBK-003 §5; D1/D5/§8 | session endpoint/stream effects; wasapi RAII; core retained Arcs; App roots | Allocation versus relation ownership explicit; retained history/storage can outlive execution |

## 10. Guarantee, assumption, non-guarantee and candidate index

| Class | Claim / scope | Source |
| --- | --- | --- |
| GUARANTEE (inherited) | Single designated D11 terminal authority; autonomous immutable commit, wait/observe pure | PBK-001 §2.3; PBK-002 D11 |
| GUARANTEE (inherited) | Whole fresh-composition Activated and old-side authoritative clearance; failure-clean disposal/fail-stop | PBK-002 D14.6 |
| GUARANTEE (inherited) | Seek mutation classification, atomic admissibility/cut predicate, refusal preservation, ending abort scope | PBK-002 D14.5 |
| GUARANTEE (inherited) | DSP latest pending/complete-in-flight within processing, worker Accepted versus Desired, Seek history semantics | DSP §7.3; PBK-002 D14.11 |
| GUARANTEE (inherited) | Composition inverse/discharge ordering blocks provider release past violation | K0 §G.6; realization kernel/session |
| ASSUMPTION | A claimed backend discharge/format/read contract is honored by its implementation; tests must target the actual boundary | PBK-003 §5; decode/output ports; not established by this docs audit |
| ASSUMPTION | OS schedules runnable work, native calls/device progress and blocking I/O eventually return where liveness is claimed | D6; concrete native/backend paths |
| ASSUMPTION (candidate precondition) | Assembly attaches one fresh core/spec to only one attempt | D1; current handle contract |
| NON-GUARANTEE | No general Actor runtime, universal mailbox ordering, playback-wide generation/epoch or new global registry | K0/PBK-002 current model; D1/§1 |
| NON-GUARANTEE | No global fixed-memory budget or global operation deadline; local bounds/slices are narrower | D6 |
| NON-GUARANTEE | Terminal Fact implies neither resource quiescence nor worker joins; Discharged implies neither host nor process quiescence | D5; D11/K0 scopes |
| NON-GUARANTEE | Projection is not semantic authority; source/position/diagnostic observations have no universal atomicity/freshness/acoustic guarantee | PBK-001; PBK-002 D14.8; temporal |
| NON-GUARANTEE | No unconditional blocked-stdin join or external output-delivery deadline; reader dispatch/report/output is forbidden after seal | D3 candidate |
| OPEN / CANDIDATE DECISION | D1 attachment clarification; optional structural enforcement unchosen | D1; no behavior change |
| OPEN / CANDIDATE DECISION | D2 common host representation and consumption of existing Activated; C1 implementation in #204 (pending owner review) | D2; #201 sequencing |
| OPEN / CANDIDATE DECISION | D3 recorded-failure cut, mandatory record-then-Stop and closed post-seal reader admission/output; policy specified for review, C2 implementation unstarted | D3; #202 scope |
| OPEN / CANDIDATE DECISION | D4–D6 cross-protocol fate, quiescence and bounds scopes await independent review/freeze; inherited rules already retain their authority | This candidate |

No new runtime gap or accepted-authority contradiction is claimed by this audit.
The campaign differentials are C1/F1 (corrected by #204 pending owner review)
and C2/F2 (still unimplemented). A concrete
counterexample must be classified as model/spec mismatch, implementation defect,
authority gap, or refinement/oracle gap before changing production or authority.
Generic K0 capability is not evidence of supported playback-core reattachment;
implementation constants are not new accepted global budgets. Historical OPEN
notes are read with their later accepted narrow amendments, not used to reopen
already-owned D14.6 or DSP §7.3 semantics.

## 11. Review, corrective sequence and validation handoff

Stage 1 supplies a candidate only. Independent review must answer identity,
reattachment, whole-result consumption, host settlement, work fate, quiescence,
bounds and all 20 trace questions from this document and linked repository
evidence. C1 is implemented by #204 pending owner review; C2 remains unstarted.
Further candidate review and campaign sequencing remain owned by #198/#201; no architecture
freeze is implied by a documentation build or Cargo regression suite.

**Before Stage 7 architecture freeze begins, a separate execution-validation
owner must already exist** (#198/#201 Stage 6.5). That owner must freeze:
architecture version under test; required platforms; deterministic/failure
oracles; mandatory versus earned stress/performance/model checking; PASS / FAIL /
INCONCLUSIVE; counterexample taxonomy; implementation defect versus authority
gap; and the authority-gap reopen protocol. This document does not create that
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
