# Playback Temporal Semantics

> **NORMATIVE AUTHORITY** for Qianqian's *cross-cutting playback temporal
> vocabulary*: the command → acceptance → evidence → commit → observation
> reading model, the evidence classes, the ordering sources, and the
> correctness carriers that name WHICH existing mechanism carries each
> guarantee.
>
> This document defines **no runtime nouns and no runtime mechanism**. It
> is descriptive/normative over mechanisms that already exist; it adds no
> field, no lock, no ID, no state. Qianqian has temporal semantics; it
> does not currently have, and does not currently need, a general
> temporal runtime framework.
>
> Authority division — this document owns only the cross-cutting model.
> Protocol-specific authority is NOT restated here and stays with its
> owners; their predicates always win over any summary below:
>
> ```text
> ADR-PBK-001 §2.2–§2.3        Command / Fact / Evidence / Projection
>                              truth classes; commit precedes Fact
>                              publication; (fact kind, subject scope)
>                              authority identity
> ADR-PBK-001 §2.4, §6         realtime/PCM firewall; P1–P5
>                              publication/reclamation
> ADR-PBK-002 §17 (D11)        episode terminal settlement — exact
>                              decision contract, late-command rule
> ADR-PBK-002 §20 D14.5        seek — exact acceptance set, cutover
>                              protocol, commit conjuncts, failure policy
> ADR-PBK-002 §20 D14.7        pause/resume — exact establishment formula,
>                              park invariant, engagement fence
> ADR-PBK-002 §20 D14.8        position — exact derivation, monotonicity
>                              scope, withdrawal
> ADR-PBK-002 §20 D14.11       audio processing minimum — ownership,
>                              placement, live-update admission
> architecture/dsp-product-model.md §7.3
>                              live-DSP semantic states and frozen
>                              propositions (Desired/Accepted/Applied/
>                              Transitioning/Settled)
> ```
>
> Owner: issue #197 (QIANQIAN-PLAYBACK-TEMPORAL-EXPLICITNESS).
> Authority corrective: [#216](https://github.com/jnhu76/qianqian/issues/216)
> reconciles the acceptance summaries with DSP §7.3 and narrows unsupported
> elapsed-time and episode-lifetime claims. Exact protocol predicates and DSP
> states are unchanged.
> Evidence basis (not authority): `research/temporal-model-0/RESULTS.md`
> and `research/temporal-mechanism-audit-0/RESULTS.md`.

---

## 1. What this document answers

Temporal correctness in Qianqian's playback subsystem is carried entirely
by structure — ownership, program order, lock boundaries, slots, fences,
reset disciplines, bounded queues and acknowledgements. None of it is
carried by clocks, counters, or IDs. That design is sufficient, but until
now it was implicit: a reader had to reconstruct the argument from many
implementation sites and ADR fragments. This page is the one place where
the argument is stated and where each guarantee is traced to the
mechanism that carries it.

The eight standing questions this document answers for the whole
subsystem:

```text
How is a command accepted?                     §3 (acceptance)
What establishes ordering?                     §4 (ordering sources)
What evidence is current-world state?          §2.3
What evidence belongs to one operation?        §2.4
Where is the commit boundary?                  §2.5, per-protocol maps §6
What prevents stale evidence crossing cycles?  §2.4 + §5 + §6.1
What correctness depends on program order?     §4, §6.1 (seek), §7 (edge)
What is merely an observation?                 §2.7
```

After reading this page, a contributor should be able to find the exact
normative predicate in the owning ADR section without re-deriving the
architecture.

---

## 2. Normalized temporal vocabulary

Eight concepts. They classify things that already exist in the code;
none of them introduces a new type, field, or runtime state. Every
classification below was derived from actual semantics (what sets it,
what clears it, what reads it), not from field names.

### 2.1 Command

An externally requested intent, recorded on the episode seam. A command
is NOT automatically accepted, and acceptance is NOT success.

```text
request_stop / request_pause / request_resume / request_seek
set_preamp / set_eq_config / set_eq_preset / set_processing_enabled
request_output_level
```

Truth class: **Command state** (PBK-001 §2.3: command ≠ fact). A
command that is never accepted — or accepted and later aborted — remains
inert command history; it never becomes semantic truth by itself.

### 2.2 Acceptance (protocol-owner-defined point)

`ACCEPTED` names the semantic acceptance point defined by the protocol's
owning authority, never API-call entry. Command recording has its own
linearization point; it need not coincide with semantic acceptance.
Some protocols record/admit intent under a control lock. DSP product
Accepted instead follows worker validation/compilation against the
episode format, with the accepted processor target held as specified by
[DSP §7.3](dsp-product-model.md). This cross-cutting category prescribes
neither a universal lock nor a universal acceptance mechanism.

```text
seek        conditional refinement at the one-seek slot plant in
            request_seek: Accepted iff the actual D14.5 eligibility
            conjunction holds there; an internal plant alone does
            not establish Accepted (see §6.1)
stop/pause  intent recorded under the one completion lock; stop intent
            is recorded BEFORE the data-plane stop is released
DSP update  Desired recording: compose + intrinsic validation + whole-config
            record under ONE ProcessingControl lock hold; later worker
            validation/compile establishes Accepted under DSP §7.3 (§6.5)
```

Worker pickups, park waits and commit polls follow their protocol's
recording/admission and revalidate the relevant conditions. DSP pickup
follows Desired recording and precedes the worker compile that establishes
Accepted. Exact acceptance predicates remain with each protocol owner.

### 2.3 World-state evidence

Evidence asserting that something is **physically true NOW**.

```text
engaged        a pause-attributed park exists at the pre-GetBuffer gate
seek_engaged   a cut-attributed park exists at the pre-GetBuffer gate
```

Properties (all three are load-bearing):

```text
may survive an operation boundary while still physically true
    (a continuously-held park across two seek cycles — §5)
must stop being usable when that world-state ends
    (Disengaged / SeekDisengaged clears it; the gate emits these on the
     leg's own thread, including probe-failed exits)
is not owned by any one command
    (a pause park asserts "the leg is parked", whatever command
     constellation produced it)
```

World-state evidence is the reason a paused episode can seek (§6.2): the
seek commit boundary deliberately reads a physical conjunction that
either park attribution proves.

### 2.4 Operation evidence

Evidence belonging to **one logical operation/cycle**, with an explicit
reset discipline.

```text
seek_landing    the CURRENT cut cycle's provider landing publication
seek_refused    the CURRENT cycle's proven pre-mutation refusal
cut_committed   the CURRENT cycle's cutover-commit record
```

Lifecycle: **reset at internal recording** — an Accepted seek therefore
starts with all three cleared before the new protocol runs. A never-Accepted
ending-raced record may also clear these private fields (§6.1); only the single worker
thread can publish new operation evidence, strictly after the slot was
re-occupied. An operation's evidence therefore can never satisfy a later
operation's commit boundary. This reset + the one-in-flight slot is why
no operation identity (no `SeekId`) is currently earned (§8).

A third, intermediate class sits between these two:

```text
ENGAGEMENT-SCOPED EVIDENCE — tail_quiesced / seek_tail_quiesced

Belongs to the CURRENT engagement or park, not to a seek cycle: it is
re-derived by a fresh tail probe for every park, and it is reset both
when the engagement begins (Engaged / SeekEngaged) and when it ends
(Disengaged / SeekDisengaged). It asserts "the queued-to-play set of
THIS park is empty" — true only while the park that derived it holds.
```

### 2.5 Commit boundary

The authority-owned predicate at which **semantic truth changes**. A
commit boundary is a predicate evaluated under the owning lock — not a
test observation, not a wall-clock instant, not a scheduling checkpoint.

```text
seek      seek_cutover_decision(): ONE three-valued sample under the
          completion lock — Committed / Aborted / Pending (D14.5;
          a Pending is a statement about missing evidence, never an
          abort)
terminal  resolve(): pure function of the lock-protected evidence
          record, memoized once — first-wins (D11)
```

What the seek commit boundary reads is the frozen D14.5 conjunct set
(landing published ∧ edge invalidated ∧ tail quiesced ∧ leg parked ∧
episode unsettled); this document only points at it.

### 2.6 Acknowledgement / quiescence evidence

Explicit mechanism evidence that some actor **can no longer perform a
class of work**. Distinct from arbitrary observation: an acknowledgement
structurally freezes a quantity or closes a duty.

```text
join (stop_and_join)             no submitter can remain
release payload consumption      the rebase instruction was applied on
                                 the leg's path (the one-seek slot frees
                                 only after it)
DrainSignal verdict              the render leg's terminal publication
worker_gone                      the protocol's only resolver has left
gate disengagement               the park has ended
tail quiescence (padding == 0)   nothing submitted before the park
                                 remains queued to play
```

Acknowledgements are what honest oracles anchor on; wall-clock waits are
not acknowledgements (§9).

### 2.7 Observation

A consumer reading current state — never a semantic act.

```text
observe() / wait_terminal()   the episode seam's pure reads (D14.2)
test probes                   verification evidence, not truth
future TUI / Observation Plane  future consumers of projections
```

Frozen invariant:

> **OBSERVATION ORDER does not create SEMANTIC ORDER.**

The terminal outcome is a memoized pure function of the lock-protected
record at publication time; `wait_terminal()` reads the committed value.
No consumer call can create, advance, or relabel it (D11 settlement
ownership). The same holds for every projection: Position, `paused()`,
and engagement evidence are derived visibility (PBK-001 §2.3), never a
correctness basis.

---

## 3. The canonical temporal pipeline

This is a **semantic reading schema**, not a runtime framework. Each
protocol owner determines the applicable steps and exact predicates;
protocols do not share data structures or an acceptance mechanism.

```text
Command                          (§2.1 — intent; not automatically accepted)
   │
   ▼
Acceptance / linearization       (§2.2 — protocol-owner-defined point;
   │                             recording may precede acceptance)
   │
   ▼
Current protocol state
   ├── world-state evidence      (§2.3 — physically true now)
   ├── engagement-scoped /       (§2.4 — this park / this cycle)
   │   operation evidence
   └── actor program-order obligations
   │
   ▼
Commit predicate                 (§2.5 — the owning authority's boundary)
   │
   ▼
Committed semantic fact          the ONLY Fact kind in this subsystem
   │                            is the D11 terminal outcome. The
   │                            recorded seek cutover is commit-scoped
   │                            PROTOCOL state, not a Fact.
   ▼
Observation / projection         (§2.7 — pure reads; no authority)
```

Reading rule: when analyzing any temporal defect or test flake, locate
the stage. Most historical flakes were **observation-stage** defects
(a scheduling window consumed as if it were a commit) while every
protocol stage behaved correctly.

---

## 4. Correctness carriers

Ordering and exclusion in Qianqian playback come from exactly these
carriers. When you need to know WHY a guarantee holds, find its carrier
here — then verify the carrier still exists in code. When you propose a
change, name which carriers your change relies on or removes.

```text
LOCK_LINEARIZATION   the one completion mutex linearizes commands,
                     evidence publication, and settlement; the
                     command routers take gate intent under it
                     (two one-directional nestings — completion →
                     slot, and completion → gate intent; no cycle is
                     reachable)
PROGRAM_ORDER        each actor is single-threaded over its own duties:
                     the decode worker is the ONLY producer and the
                     only protocol runner; the render leg is the ONLY
                     submitter; same-leg event ordering is what makes
                     Engaged the current-engagement fence
SINGLE_WRITER        each published cell has exactly one writer
                     (position cell; worker-failure record)
ONE_IN_FLIGHT        the seek slot holds at most one operation; a
                     second request is inert, not queued or coalesced
EVIDENCE_RESET       seek operation evidence is reset at recording;
                     engagement-scoped evidence is reset at engagement
                     AND disengagement
WORLD_STATE_FENCE    engagement events fence prior-cycle evidence out
                     of the current engagement; commit boundaries
                     re-read the park conjunction under the owning
                     lock. A latch can be microseconds stale
                     (disengagement publication is asynchronous with
                     the physical park exit); that staleness is real
                     but never load-bearing — §5
ATOMIC_COMMIT        the seek cutover decision is ONE three-valued
                     sample; the terminal resolve+commit runs in ONE
                     lock hold (no second sample for an evidence gap
                     to land between)
BOUNDED_QUEUE        the PcmEdge is a bounded FIFO with first-wins
                     terminals; a FAILED/STOPPED edge abandons
                     buffered frames
ACKNOWLEDGEMENT      join, release-payload consumption, worker_gone,
                     drain verdict — structural closure of a duty
FIRST_WINS           terminal outcome; edge terminal; landing latch;
                     worker-failure record — later events cannot
                     relabel earlier commitments
```

The most commonly misattributed guarantee, stated explicitly:

> **"No stale pre-cut PCM becomes post-cut PCM"** is NOT carried by a
> fresh `seek_engaged` flag. It is carried by **worker program order at
> the serialization point**: with the parked evidence in hand, the one
> producer thread runs provider seek → staging discard → the single
> `edge.invalidate()` purge → landing publication → production hold
> (writes nothing) until the commit decision routes the release. The
> purge-to-landing-to-hold discipline on the only producer thread is the
> load-bearing wall (D14.5 Applied bullet); the latches are its
> bookkeeping, not its guarantee.

---

## 5. Continuous world-state carry

A subtlety that motivates the world-state/operation split:

```text
A physical park may remain continuously true while one seek cycle ends
and another begins.
```

On the refusal/abandon routes the slot frees without the leg necessarily
observing a disengagement, and a new seek may re-arm the same in-flight
park. The latches that carry across that boundary do so **only while the
represented physical state never ceased to hold**: the leg never left
the gate, holds no device buffer, and the device tail cannot refill
while parked (the D14.7 park invariant).

Therefore:

> **World-state evidence may remain valid across operation boundaries IF
> the represented physical state never ceased to hold. This is NOT
> stale-operation evidence.**

Stale **operation** evidence (a previous cycle's landing) can never
ground a later commit — recording resets it, and the slot keeps cycles
serialized. Continuously-true **world** evidence grounding a later
commit is correct by definition: it asserts something about the world
NOW. Two boundaries keep world evidence honest: the disengagement fence
(the gate publishes SeekDisengaged on the leg's own thread on every
park exit, including a failed tail probe, and every new park re-derives
quiescence from a fresh probe), and the commit boundary's re-read under
the owning lock. A latch CAN be microseconds stale — disengagement
publication is asynchronous with the physical park exit — but that
staleness is never load-bearing: safety rests on the worker program
order (the §4 call-out) and the D14.7 park invariant (a parked leg
submits nothing, so the tail cannot refill), never on latch freshness.

---

## 6. Protocol temporal maps

Each map is a reading aid for its owning authority; the linked section
freezes the exact semantics.

### 6.1 Seek — authority: ADR-PBK-002 §20 D14.5

```text
COMMAND
    request_seek(target)                       (source-relative; inert
                                                when invalid)

ELIGIBILITY OBSERVATION
    preliminary completion checks, then a
    separate edge-Open sample; this is not
    an atomic joint eligibility observation

INTERNAL RECORD                                LOCK_LINEARIZATION ·
    second completion hold rechecks episode   ONE_IN_FLIGHT ·
    conditions, then plants in the free slot, EVIDENCE_RESET
    resets cut evidence and routes gate hold;
    it does NOT recheck edge Open

SEMANTIC ACCEPTED (D14.5 refinement)
    at the slot's false → true plant iff the
    actual edge is Open there, together with
    the protected episode/free-slot conditions;
    otherwise the plant is Refused/Inert,
    never Accepted

WORLD-STATE EVIDENCE                           WORLD_STATE_FENCE
    engaged · seek_engaged
    (either park attribution may ground
    the commit)

ENGAGEMENT-SCOPED EVIDENCE
    tail_quiesced · seek_tail_quiesced
    (re-probed fresh for every park)

OPERATION EVIDENCE
    landing · refusal · cut commit
    (reset at recording; first-wins
    landing latch)

SERIALIZATION WALL                             PROGRAM_ORDER
    purge  →  landing publication  →
    production hold
    (the load-bearing stale-PCM exclusion;
    §4 call-out)

COMMIT                                         ATOMIC_COMMIT ·
    seek_cutover_decision()                    LOCK_LINEARIZATION
    ONE three-valued sample:
    episode ending → Aborted
    landing ∧ (parked ∧ quiesced, either
    attribution) → Committed
    else → Pending (never an abort)
```

This is a classification of existing source histories, not an added
runtime check, latch or public result. The second completion hold protects
the episode conditions and serializes slot reservation; the earlier Open
sample does not protect edge state at the plant. D14.5 owns the semantic
classification; `request_seek` returns `()` and supplies no positive receipt.

In the disputed ending history, A occupies the slot while B observes Open;
the edge becomes Stopped before A frees the slot; B then records. B is
**Refused/Inert, never Accepted**. Its temporary record/reset/hold is private
bookkeeping. No instant with Open plus a free slot exists in that history.
An Open-at-plant record is instead Accepted, even if ending subsequently
aborts it; admission refusal and an accepted provider `RefusedUnchanged`
outcome are different stages.

The refinement is carried by irreversible edge terminals and worker program
order. A later worker Open check confirms that the edge was also Open at the
earlier plant; it does not introduce a later acceptance point. A non-Open
plant can never become Open again: it cannot cause provider seek, purge,
landing or committed rebase. The worker's ending/non-Open checks abandon it,
or its exit funnel publishes worker-gone and aborts the stranded slot/hold.
These private Seek fields do not participate in D11 `resolve`; the original
ending cause retains terminal authority. After an Accepted provider call,
result classification, successful purge/landing, cut commit, render rebase
and observation remain the distinct stages below.

Targeted deterministic evidence is source-visible in `completion.rs::tests::
sampled_open_busy_then_stopped_free_plant_is_never_accepted` (the exact split
recording/returning-worker cleanup boundary) and `session.rs::seek_record_tests::
stopped_record_has_no_seek_effects_while_open_record_executes_cut` (that record
through the real worker, counted provider/processing and controlled render
gate, with an Open-at-plant Applied control). These compose property-local
oracles; they do not claim a contiguous A/device race run. Existing public
`seek_seam` content/rebase/refusal/ending tests retain their distinct scope.

```text
Why no explicit seek identity is currently earned:
    one operation in flight        (slot)
  + operation evidence reset       (recording)
  + slot freed only on resolution  (release payload consumed first)
  + current-world fences           (disengagement clears; fresh probes)
  + single-writer program order    (the one producer)

No explicit seek identity is currently earned. This is a statement about
today's evidence, not an eternal prohibition: a future protocol that
allows concurrent operations or cross-actor attribution would need its
own D13-style earning review.
```

### 6.2 Pause — authority: ADR-PBK-002 §20 D14.7

```text
COMMAND
    pause (idempotent intent; resume
    clears it)

ACCEPTANCE
    intent recorded under the completion
    lock; routing withheld after stop /
    teardown release / settlement

WORLD-STATE
    Engaged (the leg is parked; no
    device buffer held)

ENGAGEMENT-SCOPED EVIDENCE
    TailQuiesced (this engagement's
    tail observed empty)

COMMITTED PRODUCT STATE
    Paused — the frozen establishment
    formula holds: unsettled ∧ pause
    intent ∧ Engaged ∧ TailQuiesced.
    A Projection, NEVER a Fact.

EXIT
    Disengaged (mechanism evidence
    only; it proves the park ended,
    never that a viable leg remains)
```

```text
Paused seek is legal (frozen D14.5 pause interaction):
    pause intent SURVIVES a seek; a seek never implicitly resumes.
    A pause-attributed physically-parked ∧ quiesced render leg MAY
    satisfy the seek output-cut precondition — the commit boundary
    reads a physical conjunction both park kinds prove equally.
    Attribution stays structurally separate in the other direction:
    seek events never touch pause latches, so a cut park can never
    fabricate Paused truth.
```

### 6.3 Terminal settlement — authority: ADR-PBK-002 §17 (D11)

```text
mechanism evidence
    (worker failure · worker terminal ·
     drain verdict)
stop intent — COMMAND state, read by
    resolve() as the Stopped/Failed
    discriminator
        ↓
resolve()   — pure function of the one
lock-protected record; publication and
settlement in ONE hold
        ↓
first semantic settlement   — FIRST_WINS +
                             LOCK_LINEARIZATION
        ↓
immutable terminal outcome
    (Completed / Stopped / Failed)
```

The stop discriminator is **decision-time stable**: stop intent is
recorded under the same lock BEFORE the data-plane stop is released, so
a stop linearized before the decisive publication necessarily
participates, and a later one cannot relabel an already-decisive
outcome (the D11 late-command rule). Observation timing cannot relabel
the terminal: nothing writes the outcome after the first settlement.

### 6.4 Position — authority: ADR-PBK-002 §20 D14.8

Four different quantities that must never be conflated:

```text
source position          seek target / reported landing (media time)
decoded/produced PCM     runs ahead of consumption (decode runs ahead)
progress
DSP-processed progress   processed future PCM; does not move Position
device-consumed          THE authoritative user-visible Position:
Position                 handed-off − queued tail, published monotone
                         by the single render-leg writer, read as one
                         pure load
```

```text
produced PCM time  ≠  playback Position.
```

The Position projection is monotone **between committed discontinuities**
— a committed cutover rebases it exactly once, on the writer's path,
basis = the decoder's reported ACTUAL landing (never the requested
target); an unknown landing withdraws the projection for the episode
(unknown stays unknown — never zero). It is withdrawn again at the
terminal Fact or an activation failure. It is a device-consumption
estimate: it claims nothing about the acoustic instant.

### 6.5 Live DSP update — authority: ADR-PBK-002 §20 D14.11 +
architecture/dsp-product-model.md §7.3

```text
Desired
   ↓  typed set_* command: compose + intrinsic validation +
   ↓  whole-config Desired recording, ONE lock hold
pending depth-1 slot (latest-wins; complete-in-flight)
   ↓  worker pickup at the FRESH-STAGING-BLOCK boundary
Accepted (pickup-time compile against the episode format)
   ↓  apply boundary = the next whole unprocessed staging block
Applied / Transitioning (bounded sample-driven crossfade; the
   ↓  processed remainder is immutable — never reprocessed)
Settled (exactly a fresh instance of the accepted configuration)
```

```text
What currently substitutes for explicit generation:
    atomic whole-config update      (one hold; racing field commands
                                     cannot lose a field)
    depth-1 latest-wins slot        (rapid updates coalesce
                                     deterministically)
    take-once semantics             (each accepted update applies at
                                     most once)
    fresh-block boundary            (never mid-block)
    processed-remainder immutability
```

This machinery is **sufficient for ENGINE temporal correctness**: no
block ever mixes configurations outside the designed crossfade. It is
deliberately **NOT a read model**: an external observer cannot tell
which configuration produced block N. A future UI read model (#188) may
earn a local `desired_generation` / `applied_generation` pair — that is
a separate product decision; this campaign adds none.

### 6.6 PcmEdge / failure — authority: D14.5 failure policy, D11,
PBK-001 §2.3 truth classes

```text
producer (the one decode worker)
    ↓
bounded PcmEdge   (8192-frame FIFO ring; first-wins terminals;
    ↓              terminal check BEFORE the data check — a FAILED or
    ↓              STOPPED edge abandons buffered frames)
consumer (the one render leg)
```

Distinguish four things that are often blurred:

```text
terminal Fact publication   decode_failed publishes failure evidence;
                            the D11 settlement commits the outcome —
                            under the completion lock, synchronously
edge terminal transition    edge.fail() runs LATER on the worker's path
                            (a scheduler-open window between the two)
already-buffered /          lawful consumption after the Failed Fact is
already-fetched PCM         subject to separate local reservoir bounds:
                            ring occupancy <= its capacity C; each pull
                            <= min(resident frames, destination frames).
                            A pull frees ring space before submission,
                            so refill can leave ring C plus fetched n.
                            There is NO single-ring-capacity aggregate
                            guarantee for these simultaneously held sets.
                            The "drain" is the PRE-edge.fail() window,
                            never a post-failure edge drain
consumer quiescence         the join (stop_and_join) is the
                            acknowledgement that no consumer remains
```

These are source-side reservoir bounds, distinct from PCM already submitted
downstream, device queues and acoustic output. Staging/remainder and gate/slot
cardinalities retain their own explicit local bounds in the execution model
§10. No replacement aggregate numerical bound is asserted here.

The guarantees come from **local capacity** (each bound is structural),
**program order** (the worker returns immediately after `edge.fail()`),
the **first-wins terminal** (a stop cannot overwrite a committed EOF),
and **join**. Not from "eventually drains" prose.

### 6.7 Episode replacement — authority: D14.11 ownership rules

Episode-owned temporal state — the completion record, the gate, the
seek slot, the position cell, the edge — remains bound to its originating
episode. On current supported fresh assembly/replacement paths, a new
episode binds fresh state (including processing state under its own
configuration snapshot). Retained old handles and callbacks still refer
to the old episode state; they cannot target the successor on these paths.
Retained references may keep old storage alive after active episode
ownership ends, including after terminal settlement and teardown/joins.
Ending the episode does not prove allocation reclamation.

**Authority corrective ([#216](https://github.com/jnhu76/qianqian/issues/216)).**
The former "dies with episode ownership" / "structurally impossible"
wording is narrowed to episode binding and the supported topology above,
not allocation lifetime or generic structural enforcement. The
[execution model's D1](playback-execution-model.md) fresh-core /
single-attempt attachment rule remains a **CANDIDATE playback-domain
precondition**. Production Session factories and generic K0 do not
structurally enforce one-shot core attachment; this correction neither
accepts D1 nor adds enforcement.

```text
The current supported topology does not require a global EpisodeEpoch
for internal playback correctness; this is not a generic core-reuse guarantee.
```

Future asynchronous consumers that hold results across episode
boundaries may earn explicit episode/generation identity independently
(the local `desired/applied` pattern in §6.5 is the pre-validated
shape) — through a narrow authority decision, not by default.

---

## 7. Protocol summary table

Under five minutes, whole subsystem:

| Protocol | Acceptance | World-state evidence | Operation / scoped evidence | Commit | Primary correctness carriers |
|---|---|---|---|---|---|
| Seek (D14.5) | plant refines to Accepted iff actual joint eligibility holds there; ending-invalid plant is Refused/Inert (§6.1) | `engaged`, `seek_engaged` (either attribution) | landing / provider refusal / `cut_committed` (per-cycle, reset at recording); `*_tail_quiesced` (per-park) | `seek_cutover_decision()` — one three-valued sample | LOCK_LINEARIZATION, ONE_IN_FLIGHT, EVIDENCE_RESET, PROGRAM_ORDER, ATOMIC_COMMIT |
| Pause (D14.7) | intent recorded under the completion lock; routing withheld after stop/teardown/settlement | `Engaged` | `TailQuiesced` (per-engagement) | the Paused establishment formula holds (a Projection, not a Fact) | WORLD_STATE_FENCE (Engaged fence), PROGRAM_ORDER (leg event order) |
| Terminal (D11) | — (settlement is not a command) | mechanism evidence: worker failure/terminal, drain verdict | stop intent recorded pre-data-plane-stop | `resolve()` memoized once, first-wins | FIRST_WINS, LOCK_LINEARIZATION |
| Position (D14.8) | — (not a command) | device-consumed presentation sample | per-discontinuity rebase at commit release | the committed cutover IS the discontinuity | SINGLE_WRITER, PROGRAM_ORDER (writer-side monotone) |
| DSP update (D14.11, §7.3) | Desired recording: compose + intrinsic validate + whole-config record, one lock hold; Accepted: worker format validation/compile per DSP §7.3 | desired configuration (App-owned command state, not physical-world evidence) | pending depth-1 slot, take-once | next whole unprocessed staging block → Applied; processed samples → Transitioning / Settled | LOCK_LINEARIZATION (atomic whole-config recording), PROGRAM_ORDER (worker acceptance/apply), ONE_IN_FLIGHT-equivalent (depth-1), fresh-block boundary |
| Edge failure | — (mechanism, not command) | edge terminal (first-wins) | buffered-at-failure quantity | the D11 settlement of the published Fact | BOUNDED_QUEUE, FIRST_WINS, ACKNOWLEDGEMENT (join), PROGRAM_ORDER (`fail()` before worker return) |

---

## 8. Correctness-carrier table

Populated from live code; do not extend it without authority.

| Guarantee | Carrier | NOT carried by |
|---|---|---|
| no stale post-cut PCM | worker program order: purge → landing → production hold on the only producer thread | wall clock; latch "freshness"; the purge primitive alone |
| seek cycle isolation (an old landing never satisfies a new seek) | one-in-flight slot + recording-time evidence reset | SeekId; timestamps |
| paused-seek progress | current physical park evidence (dual attribution, frozen D14.5) | scheduler timing; pause-specific special-casing |
| terminal immutability | first-wins memoized commit under the one lock | observer order; `wait_terminal()` call placement |
| decision-time stop stability | stop intent recorded under the completion lock before the data-plane stop releases | observation timing |
| DSP block coherence | whole-config one-hold commit + fresh-block pickup + processed-remainder immutability | UI timing; wall clock |
| local source-PCM bounds after failure | separate ring/pull bounds + terminal-check-before-data abandonment + `edge.fail()` program order (§6.6) | a single-ring-capacity aggregate; "eventually drains" reasoning |

---

## 9. Temporal anti-patterns

Qianqian does NOT use any of these as correctness authority, in
production or in oracles:

```text
wall-clock ordering
sleep duration
observer arrival order
thread scheduling position
test polling position
physical timestamp comparison
```

```text
timestamps may measure duration
but do not establish causal ownership.
```

Causality comes from the carriers in §4. A timeout can provide a
**local liveness backstop** (notify plus a capped requested wait slice)
and diagnostics, never the anchor of a safety claim. This corrective
narrows the former bounded-latency wording: the slice does not bound
scheduling, mutex acquisition/reacquisition, native I/O, join duration
or end-to-end operation latency. It does not relax PBK-001's realtime
safety/publication contracts. (Methodology for test oracles is a separate,
future guide; this section only fixes what counts as correctness authority.)

---

## 10. What is deliberately absent

```text
no TemporalManager / clock / Lamport / vector clock
no SeekId / PauseGeneration / global epoch / global sequence number
no event sourcing / global ordered event log
no generic protocol framework or state-machine runtime
no new Plugin / Capability / EventBus
```

The explicit model on this page is descriptive/normative over existing
mechanisms (slots, fences, resets, locks, program order, bounded
queues, acknowledgements). Introducing any temporal runtime noun
requires the AGENTS.md abstraction-earning review against a concrete
counterexample — the fact that this page exists does not lower that
bar; it exists so the bar's answer is readable.
