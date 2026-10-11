# Designing concurrency tests: premises, legal outcomes, and oracles

> **Status:** Development / test-authoring guide (NON-NORMATIVE for product behavior).
> **Origin:** [#238](https://github.com/jnhu76/qianqian/issues/238), with earlier examples in [#195](https://github.com/jnhu76/qianqian/issues/195) and [PR #220](https://github.com/jnhu76/qianqian/pull/220).
>
> This document explains how to test already-authorized behavior. It **does not** define a new playback state, ordering guarantee, lifecycle, K0 primitive, or semantic authority. The applicable ADR/design authority and current source remain the truth inputs; tests are counterexample-seeking evidence (see [AGENTS.md](../../AGENTS.md#verification-authority-boundary)).

## Scope and application

Use this guide when **adding or materially modifying** a test that depends on scheduling, threads, async completion, locks/atomics, wakeups, terminal publication, or resource teardown. The rules apply to production-path regressions as well as unit fixtures. Do **not** retroactively rewrite all existing tests, require a test registry, or gate unrelated changes on a repo-wide concurrency audit.

A test that passes 99% of runs can still be invalid: it may assume one legal interleaving without establishing it. Conversely, a test that always passes can be too weak to catch the defect it claims to prevent.

The test author must state the *property* and how the fixture establishes its *premise* **before** selecting the expected result. A timeout cannot create a happens-before relation.

## Five-question authoring contract

A compact test comment or PR rationale is enough; these fields are **reasoning prompts**, not a required framework, test metadata format, or new production types.

| Question | Required reasoning |
|---|---|
| **PROPERTY** | What specific invariant is being tested? Cite the owning ADR/design contract, or label it explicitly as a narrower implementation regression. |
| **PRECONDITION** | Which starting state and ordering edges must hold? Which real event, lock, barrier, latch, channel, or join proves them? Can the fixture make the awaited state unreachable? |
| **LEGAL OUTCOMES** | Under those premises, which results may occur? Is one path deliberately made deterministic, or are multiple legal executions intentionally left open? |
| **OBSERVATION** | At what owner/serialization/settlement boundary does the asserted fact become valid? Is the observed value intent, mechanism evidence, committed Fact, edge terminal, joined worker, or quiescence? |
| **FALSIFIER** | What *specific illegal behavior* would make the test fail? Can a targeted negative control or deliberately broken ordering demonstrate non-vacuity? |

For review, sketch only the **minimum relevant execution DAG**:

~~~text
writer / command issuer -> producer / owner -> publication / synchronization
                                            -> consumer / observer -> assertion
                          [which waits for whom? who can unblock whom?]
~~~

When a wait is involved, check every intentionally admitted interleaving: **can the awaited event still be generated before this assertion?** If a prerequisite can only be produced by work placed *after* that assertion, the fixture is unsatisfiable in that interleaving. Lengthening the timeout does not fix it.

## Choose the smallest of three test shapes

### A. Construct one required state deterministically

If a property needs a pre-parked render leg, route the pause **before** starting the leg. Do not spawn and hope the pause races ahead of the first gate visit. If a property needs pause to arrive *during* a read, use a local latch to witness entry into that read, route the pause, and release the latch so the next loop-top visit can observe it.

A test-controlled ordering edge may be stronger than production generally guarantees. State that distinction: **the fixture establishes the premise; the test does not promote it to a product guarantee**. Prefer the existing owner seam and a small local synchronization primitive over a new global scheduler/test runtime.

A bounded deadline is still useful as a **failure watchdog** after the causal premise has been established. On failure, ensure test-owned threads/resources can be released or torn down; avoid leaving a blocked detached thread behind. Do not make a sleep or polling iteration count the only evidence of an ordering edge.

### B. Leave a real race open; verify every legally observable result

If the tested contract permits different outcomes for an intentional race, assert **the shared invariant in each legal branch**. Do not simply match `A | B | C` and call it coverage: each accepted branch needs an independent correctness check (and, when applicable, a content/side-effect witness). Do not silently add impossible or unapproved outcomes to make a flaky test green.

Examples of distinct boundaries:

~~~text
Command request != acceptance/semantic commit
GateEvent (mechanism evidence) != episode terminal Fact
Failed Fact publication != PcmEdge terminal != leg joined/quiesced
observed pending Seek != guaranteed non-actionable Seek
~~~

A `Failed` Fact can legitimately coexist with PCM published before failure and with an edge that is not yet terminal. Freeze consumption only where the contract permits it (for example after the relevant legs have joined), and distinguish a **producer-resumption witness** from a coarse **consumer-total bound**.

If the list of legal outcomes depends on an unresolved authority question (currently [#195](https://github.com/jnhu76/qianqian/issues/195) for which park evidence makes a pending seek actionable), **STOP and request the owner decision**. Neither code recency nor an existing assertion authorizes silently choosing the semantics.

### C. Explore modeled schedules for a small shared mechanism

For a bounded atomic/locking protocol with a concrete interleaving risk, reuse the existing production-code [Loom PcmEdge tests](../../crates/qianqian-playback/src/loom_edge_tests.rs) and their opt-in feature/configuration where applicable. Loom models only synchronization that it can observe, under stated thread/operation/schedule bounds; it is not an automatic test of WASAPI, FFI, scheduler fairness, or the entire playback architecture.

New Loom coverage outside the current PcmEdge slice, Shuttle, or a simulator must be justified by a **specific uncovered counterexample**, not by general concern about flakiness. A modeled clean run means no counterexample **inside that model and its limits**, not full concurrency correctness.

## Safety, liveness, and evidence strength

- **Safety:** an illegal state/output never appears. Observe at a valid publication/serialization boundary; match the assertion to the contract, not incidental scheduling.
- **Liveness:** progress/wakeup eventually happens *under stated assumptions* (including where relevant fair scheduling and release conditions). CI timeout expiry is a useful hang signal, not an independent proof of fairness or a guaranteed bound promised by production.
- **Sleep:** may legitimately model a slow source, pace a workload, or temporarily widen a race window during diagnosis. It must not be the sole proof that another thread reached a gate or completed an operation.
- **Event counts and snapshots:** counts sampled before the producing owner settles may race. Use an agreed boundary or assert a prefix/monotonic invariant if that is what the contract supports.
- **Stress and reruns:** can discover counterexamples and help regression detection; 300 green runs do not prove all schedules correct. Do not hide semantic mismatches with retry-on-failure/quarantine.
- **Negative controls:** when the invariant protects lifetime, deadlock avoidance, terminal truth, or a high-impact concurrency boundary, show a plausible incorrect behavior makes the oracle red (or report a concrete limit if not feasible). A test that never exercises its key assertion is not evidence.
- **Test hygiene:** avoid production-side testing APIs, leaked parked threads, false liveness from an unbounded wait, and fixture-owned state that changes the semantics under test. Keep cleanup and panic paths explicit.

## Worked examples from this repository

These are **examples of a method**, not a claim that their tests prove every schedule or an invitation to bulk-refactor them. The links below are pinned to the [#237 merged baseline `39f53d4`](https://github.com/jnhu76/qianqian/commit/39f53d4ce7319c2958efb140220b187b9a5d8741) for historical precision.

### 1. Pre-parked state before abort (#238): deterministic premise

[Source: `open_abort.rs`, lines 205–315](https://github.com/jnhu76/qianqian/blob/39f53d4ce7319c2958efb140220b187b9a5d8741/crates/qianqian-output-wasapi/src/open_abort.rs#L205-L315)

- **PROPERTY:** once abort closes the gate, hostile re-pause cannot re-park the leg and wedge the join.
- **PRECONDITION:** first leg visit must park. `gate.set_paused(true)` is executed **before** `spawn`, so the first visit observes routed pause; the pre-abort `Engaged` witness checks it.
- **EXECUTION:** the first read is deliberately held *between* gate visits. Abort closes/releases, signals stop; the test routes a hostile pause on the closed gate and releases the held read.
- **OBSERVATION:** abort join returns; exactly one `Engaged` event was recorded. Five-second deadlines detect a wedge; they are **not** the ordering premise.
- **FALSIFIER:** if closed gate can re-park the post-close leg, the join or engagement-count assertion must fail.

The old test spawned before routing pause. A legal scheduler path entered the deliberately held read **before** seeing pause; `Engaged` could never occur before the assertion because release/stop were encoded later. This was an **oracle/fixture defect**, not evidence that production needed a new pause guarantee.

### 2. Pause arriving mid-read (#238): control at the owner's gate seam

[Source: `ports.rs`, lines 1211–1319](https://github.com/jnhu76/qianqian/blob/39f53d4ce7319c2958efb140220b187b9a5d8741/crates/qianqian-audio-api/src/ports.rs#L1211-L1319)

- **PROPERTY:** a pause arriving after an unrouted loop-top visit parks the leg at its **next** visit.
- **PRECONDITION:** an entry latch proves the leg has entered the first read. That read remains held while `set_paused(true)` runs.
- **EXECUTION:** release the read so it is non-terminal, forcing the leg through the next gate check; later resume and join.
- **OBSERVATION:** no engagement on the first unrouted visit; exactly one engagement and disengagement for the routed pause; two reads before exit.
- **FALSIFIER:** an ignored mid-read pause or duplicate engagement/disengagement violates the assertions.

This is a *different property at a different seam* from the abort test. Do not merge them into a fixture with a scheduler-dependent "who wins" assumption.

### 3. A pended seek has more than one legal ending (#195)

[Source: `live_tests.rs`, starting line 2400](https://github.com/jnhu76/qianqian/blob/39f53d4ce7319c2958efb140220b187b9a5d8741/crates/qianqian-playback/src/live_tests.rs#L2400-L2570)

- The earlier oracle assumed a pending seek would remain non-actionable until stop. Under load that window could end at a loop-top serialization point.
- The revised oracle inspects the **actual consumed content** for an audible cut and verifies the corresponding continuation/landing content, rather than assuming there can be no cut.
- The claim is **not** that arbitrary audio content is acceptable. Each admitted ending still needs a specific content/reference invariant; the #195 park-source authority question remains unresolved.

### 4. Fact publication is not data-plane quiescence (PR #220)

[Source: `gain_tests.rs`, lines 253–324](https://github.com/jnhu76/qianqian/blob/39f53d4ce7319c2958efb140220b187b9a5d8741/crates/qianqian-playback/src/gain_tests.rs#L253-L324), [`seek_seam.rs`, lines 525–637](https://github.com/jnhu76/qianqian/blob/39f53d4ce7319c2958efb140220b187b9a5d8741/crates/qianqian-playback/tests/seek_seam.rs#L525-L637), and [completion coexistence test](https://github.com/jnhu76/qianqian/blob/39f53d4ce7319c2958efb140220b187b9a5d8741/crates/qianqian-playback/src/completion.rs#L1371-L1394).

- Before correction, sampling consumed frames immediately after `Failed` and comparing after a fixed sleep assumed failure publication instantly halted the data plane.
- The tests now take the final consumer witness **after `runtime.dispose()` joins**. The destructive seek regression also uses the producer-side `post_failed_seek_reads()` witness, with a test negative control showing the oracle rejects a resumed read.
- Important: the test did **not** change the production protocol to make its former assertion true.

## Reviewer checklist (new/changed timing-sensitive tests only)

- [ ] Property is separately sourced from the test, with correct contract owner or explicitly narrowed implementation scope.
- [ ] Every required premise is established; no scheduler win/sleep/timeout is mistaken for synchronization.
- [ ] Legal interleavings either have deterministic fixtures **or** valid per-outcome invariants; no unreachable wait-for cycle.
- [ ] Assertion samples at the correct Fact/evidence/terminal/join boundary.
- [ ] Important bad behavior actually falsifies the oracle; non-vacuity and cleanup are considered.
- [ ] Evidence reports exact test/target/configuration, model bounds when relevant, executed checks, and any `NOT_RUN`; green CI is not authority acceptance.

Do not impose a bulk audit or generic abstraction as a consequence of this checklist. Reuse local Rust primitives; consider a shared **test-only** helper only after independently repeated demand establishes that it reduces risk. Production bugs or authority gaps discovered while authoring a test are routed separately, not "fixed" by silently changing the oracle.
