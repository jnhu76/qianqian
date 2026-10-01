# AGENTS.md

Qianqian is a local-first, lightweight, cross-platform music player and an architecture experiment in composable runtime design. This file defines repository-wide rules for coding agents. It is governance and routing, not a feature manual.

## Start here

Before changing code or long-lived documentation:

1. Read the current issue/task.
2. Read `CONTEXT.md` for current vocabulary and status.
3. Use `docs/README.md` to load only the minimum relevant authority.
4. Read `docs/architecture/overview.md` before changing architecture boundaries.
5. For generic composition work, read `docs/architecture/composition-kernel.md` and the K0 design/implementation authority.
6. For playback/audio work, read `docs/adr/ADR-PBK-001.md` plus current vocabulary/static playback authority `docs/adr/ADR-PBK-002.md`; for Output/backend/platform work also read `docs/adr/ADR-PBK-003.md`.
7. For SongCore cross-language binding/FFI work, read `docs/architecture/songcore-binding-architecture.md`; the canonical ABI description is `native/include/songcore.h`.
8. Inspect current repository reality before assuming a path, type, crate, test, TLA variable, or prior design is still authoritative.

Do not recursively preload historical refs or external failure evidence.

---

# Playback Foundations authority

```text
Normative Playback Foundations constitution:
    docs/adr/ADR-PBK-001.md        (ACCEPTED foundations)

Current vocabulary / Plugin-Fiber taxonomy / static playback composition:
    docs/adr/ADR-PBK-002.md        (ACCEPTED; current authority.
                                    Issue #138 records the corrective rationale/history)

Host-render backend boundary:
    docs/adr/ADR-PBK-003.md        (ACCEPTED; stable Output Plugin identity,
                                    backend-neutral AudioOutput contract,
                                    concrete backend owned mechanism by default)

Do not treat as current architecture unless a new experiment re-earns them:
    MusicKernel
    TransportKernel
    TrackSession
    DecodeSession
    Generation
    Active / Prepared
    Dual Window
    Physical Fence
```

Old playback code/specs are **experimental / executable evidence only**. Reuse bug reproducers and test techniques; do not inherit old nouns or force production to mirror old formal variables.

> **Preserve the bug, not necessarily the old solution.** (Full inherit/forbid lists: `ADR-PBK-001.md` §11; evidence status ladder: its §12.)

The full normative contracts — minimal constitution, command/fact authority, fact-authority identity, projection read-side firewall, semantic-commit definition, Fact publication vs Realtime-view publication, realtime lifetime invariant, and the publication/reclamation contract (P1–P5) — live in `ADR-PBK-001.md` §1–§2 and §6. Do not restate them normatively anywhere else; link instead.

---

# Composition Kernel K0

K0 semantic authority: `docs/architecture/composition-kernel-0-design.md`; representation decisions: `docs/architecture/composition-kernel-0-implementation-adr.md`.

Primitive budget:

```text
Context / Capability / Fiber / Effect / Reconcile
```

The generic kernel must not know:

```text
PCM / AudioGraph / FFmpeg / WASAPI
track/playlist semantics / seek / playback cursor
platform UI payloads
```

An Event/Fact system is **not automatically a new K0 primitive**. Start it as a normal Plugin-provided capability/service unless evidence demonstrates that it belongs in the kernel.

`Context` is a capability namespace/dependency view. It must not become global product state, a universal event bus, a message broker, PCM transport, a UI payload store, or a get-anything service locator.

---

# Everything is a Plugin / Fiber discipline

Current canonical rule (PBK-002 D4/D12):

> **Every independently K0-composed lifecycle/behavior unit is a Plugin; one live mounted instance is a Fiber.**

A Plugin may require or provide Capabilities/Services, but **providing a Capability is not an admission requirement**. A Plugin may be episode-scoped or long-lived; lifetime length is not Plugin identity.

Admission invariant (normative: PBK-002 D13):

> **If an existing Plugin can fully own the candidate without losing composition-level correctness or lifecycle ordering, the candidate MUST remain an owned resource/effect, not become a Plugin.**

"K0 already composes it" is evidence, never the admission reason.

Current mapping:

```text
architecture role     Plugin
K0 representation     ComponentSpec
live runtime instance Fiber
```

`ComponentSpec` remains a K0 formal/representation term. Do not create a second product taxonomy where some K0-composed Fibers are “Components but not Plugins”.

“Everything is a Plugin” does **not** mean:

```text
one feature = one Plugin
one command/fact/payload = one Plugin
one buffer/endpoint/worker = one Plugin
one AudioNode = one Plugin
one crate/DLL/thread = one Plugin
```

Subordinate resources remain resources unless they genuinely require their own K0 composition identity/lifecycle.

For every proposed new Plugin boundary, answer (review prompt for the D13 invariant):

```text
Why does this unit need independent K0 composition identity?
What lifetime/resources/behavior does it own?
What capabilities does it require and optionally provide?
Does it need independent activation/invalidation/withdrawal?
What execution/data edges cross the boundary?
What state is intentionally public?
Why is it not just an owned resource/effect of an existing Plugin?
What is the configuration/cognitive cost of splitting it?
```

Current earned Plugin roles include:

```text
Decode Plugin
Output Plugin
Playback Session Plugin (episode-scoped)
```

A concrete host-audio backend (WASAPI / ALSA / PipeWire / CoreAudio / a CPAL-backed mechanism) is **not** a Plugin by default. PBK-003 freezes the Output Plugin as the stable K0 composition role; the concrete backend is an owned, replaceable mechanism behind the backend-neutral `AudioOutput` contract unless a future D13 review independently earns Plugin identity.

Possible future `PlaylistPlugin`, `ProcessingPlugin`, UI adapter Plugins, etc. must still be earned by real composition/lifecycle pressure; feature names alone are insufficient.

---

# Plugin ownership vs K0 knowledge

A Plugin may semantically own domain resources such as decoder endpoints, workers, PCM edges or render streams. This **does not** mean K0 stores or understands those domain objects.

K0 keeps only its frozen composition knowledge:

```text
Fiber lifecycle
Capability reachability / committed bindings
composition Effect provenance + total inverse
component teardown Discharge verdict
```

Domain choreography remains inside Plugin activation/teardown/effect closures.

For current playback:

```text
Playback Session Plugin
    requires Decode + Output capabilities
    owns the episode-scoped lifetime / teardown responsibility for
    one episode's decoder endpoint / worker / PCM edge / render
    relation / completion

Output Plugin
    owns the selected host-render backend mechanism behind AudioOutput;
    backend identity is not K0 desired-composition truth
```

"Owns" here is lifecycle/teardown ownership, not allocation/implementation ownership: the Decode/Output provider Plugins still own the allocation mechanisms and internals behind those endpoints/streams (PBK-002 D6). PBK-003 further separates Output Plugin composition identity from its concrete platform backend.

PCM payload then flows through the already-bound data plane; K0 is not in the per-block path.

---

# Boundary-first design

Do not begin by inventing APIs. Required order:

```text
Plugin granularity / ownership
        ↓
Capability / dependency boundary
        ↓
Interaction / execution semantics
        ↓
Fact / state authority
        ↓
Lifetime / withdrawal ordering
        ↓
Realtime boundary where relevant
        ↓
Executable evidence
        ↓
API / representation
```

A different Rust type, file, crate, feature name, command, or test helper is not proof that a new Plugin or authority is needed.

Prefer deleting taxonomy/state over adding runtime concepts. For Phase-F playback work, first try the current Plugin/Fiber + owned-resource model; only a concrete counterexample may earn Window, Generation, a new Plugin, or a specialized Realtime Audio Runtime mechanism.

## Abstraction earning / razor rule

Before introducing any new architecture noun, lifecycle state, runtime mechanism, global service, or cross-cutting abstraction, answer in this order:

```text
1. Can the existing Plugin/Fiber lifecycle express the requirement correctly?

2. Can ordinary language/runtime mechanisms express it correctly?
   Examples: Rust ownership, RAII, join, cancellation, scoped resources,
   one existing disposer/effect, or an ordinary private field.

3. Can a local operation invariant express the requirement without
   introducing a new architecture noun/state?

4. What concrete evidence proves the simpler model is insufficient?
```

Accepted evidence for #4 is narrow and falsifiable:

```text
reproducible code-level counterexample
independently legal concurrency/temporal counterexample
formal counterexample
or a demonstrated ownership/lifecycle contradiction
```

If the answer to **any** of 1–3 is YES, do not introduce the new architecture concept. A future possibility, naming convenience, symmetry, diagram clarity, or “we may need it later” is not evidence.

> **Explanatory vocabulary is not architecture vocabulary.**

A term used to reason about a problem does not thereby earn a Rust type, enum state, ADR noun, K0 primitive, Plugin, Capability, registry, generation, epoch, store, or runtime subsystem. It earns architectural status only when correctness, ownership, implementation, or verification requires the distinction to exist explicitly.

Prefer, in order:

```text
local before global
domain-specific before generic
operation-specific invariant before runtime-wide mechanism
owned resource before independently composed unit
existing state before new state
```

For current playback, words such as `Dead`, `Reclaimed`, `DataPlaneAuthority`, `TimelineSegment`, `Generation`, or `Window` may be useful explanatory language but are **not current architecture** unless separately earned under this rule (the authoritative enumeration of forbidden/current playback nouns lives in `ADR-PBK-002` §13/§20; this list is routing, not a second authority). In particular, do not encode a second playback lifecycle beside K0 Fiber lifecycle + D11 terminal Fact + ordinary resource ownership.

## OPEN means un-authorized, not “agent may choose”

An ADR/roadmap item marked `OPEN`, `MECHANISM OPEN`, `AUTHORITY OPEN`, or equivalent is **not** implementation freedom for a coding agent.

If implementation requires choosing among materially different semantics, authority owners, lifecycle boundaries, or runtime mechanisms that current authority leaves open, the agent MUST stop and report an authority gap. It MUST NOT choose one because it is convenient, common in another project, or easy to code.

A coding agent may choose ordinary local representation details only when all observable semantics/ownership/lifetime obligations are already fixed and the choice does not create a new architectural noun or authority.

For Phase-F playback, `ADR-PBK-002` D11/D14 is the implementation guard. For output/backend/platform boundaries, `ADR-PBK-003` is additionally binding. Anything still explicitly OPEN there is out of scope until a narrow authority amendment earns it.

---

# Command / Fact / Projection discipline

Guardrail summary. Normative contracts — including fact-authority identity and the `(fact kind, subject scope)` uniqueness rule — live in `ADR-PBK-001` §2.2–§2.3 and are not redefined here.

```text
Command    = intent
Fact       = truth established by its designated semantic authority
Projection = derived visibility, never authority
Evidence   = mechanism observation unless separately designated as semantic truth
```

Current earned playback designation (PBK-002 D11): the Playback Session semantic role for one playback episode is the designated authority for that episode's terminal outcome (Completed / Stopped / Failed).

Agents MUST NOT:

```text
forge authority
infer semantic truth from mechanism evidence
use projection as correctness authority
```

Do not infer Playing/Starting/Paused/Position/source truth from K0 lifecycle or mechanism evidence.

---

# Realtime / PCM firewall

PCM is direct typed hot data, not Plugin dispatch payload.

Per block/quantum, never perform:

```text
Context lookup
Capability resolution
Fiber Reconcile
generic Plugin dispatch
generic Fact/Event fan-out
filesystem/network/UI round trip
unbounded allocation/blocking
```

The current playback shape is:

```text
K0 composes Plugins/Fibers
        ↓ setup / bind
Playback Session Plugin owns episode resources
        ↓
decoder → PCM edge → output
```

If a later feature creates old/new realtime views that may overlap while readers can still dereference the old world, trigger PBK-001 P1–P5 and earn the minimum publication/quiescence mechanism. Do not pre-create Window/Generation because an older model had them.

---

# Formalization policy

Formalization is risk-driven (normative policy: `ADR-PBK-001.md` §13).

> **Which independently legal states/events can interleave and collide into an illegal state?**

If there is no concrete collision, prefer types, ownership, unit/property tests, static checks, or executable stress tests. The old playback formal core is evidence, not a blocking acceptance gate. See `specs/README.md`.

## Verification authority boundary

Verification challenges the current architecture and implementation; it does not define either one.

> **Verification evidence MUST NOT silently promote a new state, primitive, lifecycle rule, vocabulary, or authority. Any such finding must first return to ADR / design-authority review.**

Apply the following rules to TLA+/TLC, Kani, Loom, Miri, property tests, stress tests, mutation tests and other executable verification:

```text
Authority / ADR        defines intended semantics
Production Rust        realizes current behavior
Verification evidence  searches for counterexamples and regressions
```

- Build verification models from the minimum current authority plus an explicit mapping to current implementation reality. Auxiliary verifier-only variables are allowed, but remain non-normative — model variables/state are proof abstractions, not production representation requirements.
- Prefer the verification mechanism closest to the property: Rust types/ownership first; bounded state/invariant checking next; concurrency schedule exploration for implementation interleavings; TLA+/TLC for concrete temporal/state collisions that are awkward or impossible to express directly against the Rust implementation.
- A clean bounded/model-checking run means only that no counterexample was found within the stated model, bounds, assumptions and fairness conditions. It is not architecture acceptance and must not be reported as "the architecture is proven correct."
- A counterexample must be classified before any production change: model/spec mismatch, production defect, authority gap, or refinement/oracle gap. Do not patch production merely to satisfy an over-strong verifier oracle.
- Formal evidence challenges authority; it does not silently define production. If evidence suggests the architecture needs a new semantic concept, record the differential and use the Authority resolution process below.
- Verification harnesses may encode stronger diagnostic checks than production contracts only when those checks are named as diagnostics and are not allowed to redefine lifecycle or correctness semantics.

---

# UI boundary

UI is not playback authority and never participates in realtime correctness.

A UI widget is not a Plugin merely because it invokes a command. A UI adapter/controller may earn Plugin identity only if it genuinely needs independent K0 composition/lifecycle identity.

UI must consume application-facing commands and read-side facts/projections without seeing K0 internals, PcmEdge, SongCore handles or WASAPI objects.

---

# External evidence isolation

External failure mining lives under `evidence/` and is opt-in.

Do not load it during ordinary architecture/ADR/implementation review unless the task explicitly asks for external failure evidence or adversarial inspiration. External systems can inspire distinctions; re-derive Qianqian invariants locally.

---

# Review discipline

Fresh-context reviewers for playback/architecture work should prioritize:

```text
Plugin vs owned-resource confusion
Output Plugin vs concrete host-backend identity leakage
backend-specific API vocabulary promoted into a generic contract
ComponentSpec representation accidentally becoming a second taxonomy
feature-shaped Plugin over-fragmentation
Context/event/PCM misuse
hidden global authority
command/fact confusion
fact-authority forgery
projection used as correctness authority
provider/resource release before realtime readers quiesce
accidental preservation of old Window/Generation assumptions
```

---

# Verification

Report what was actually verified. Never mark an unrun device/platform/audio check PASS.

Green Cargo tests are regression evidence, not architecture acceptance. Verification reports must state target, tool, assumptions/bounds and result class.

## Local pre-push gate (Lefthook)

Lefthook is the repository's Git hook orchestrator. The `pre-push` gate is the fast local rejection layer; GitHub Actions remains the authority for platform, formal, concurrency, mutation, Windows and release evidence.

Every coding agent must run, before proposing or pushing code:

```bash
lefthook run pre-push --all-files
```

The gate runs checks only — it never formats, auto-fixes, stages, commits, or touches the network:

```text
always:                 git diff --check
                        python3 tools/check_architecture_vocabulary.py
Rust paths changed:     plugin boundary gate, cargo fmt --check,
                        cargo clippy --workspace --all-targets -- -D warnings,
                        cargo test --workspace
docs/website changed:   website docs:verify (needs `cd website && npm ci` once)
```

What it proves / does not prove:

```text
proves:       the current WORKING TREE passes the fast deterministic local
              checks (the manual --all-files run executes the complete gate)
does not      Windows / cfg(windows) compilation, TLA+/TLC formal suites,
prove:        Miri, Loom, mutation negatives, WASAPI/device evidence,
              VitePress build, commit/PR convention enforcement
```

On a real `git push`, commands with `glob` are narrowed to the pushed changeset (Lefthook's native push-file detection; it can only narrow on a real, known delta and never silently skips a real change). The manual `--all-files` run is always the complete gate. Install and prerequisites: `CONTRIBUTING.md`.

## Real-path testing and reproducible evidence

Verification should be chosen from the failure being investigated, not from a preferred testing technology.

Before implementing a playback, lifetime, realtime, FFI, backend or portability mechanism whose intended behavior is known, enumerate the concrete ways it can fail and identify the independent observation that would expose each important failure. Do not implement a mechanism first and then add unit tests that merely encode its private state layout or call sequence. A regression test added after a real defect is found is valid when it independently reproduces that failure.

For cross-layer behavior, prefer the real execution path appropriate to the claim, for example:

```text
SongCore
→ language binding / ABI
→ decode
→ PCM/data plane
→ Output Plugin
→ concrete host backend
```

Do not require every property to be E2E. Realtime races, publication/reclamation, ownership collisions and temporal behavior may be better established with Loom, Kani, Miri, TLA+/TLC, controlled stress tests or deterministic local tests. Use the verification mechanism closest to the property being established.

Mock-only evidence cannot establish that a real host audio backend, FFI boundary or platform integration works. A mock may isolate an upstream contract, but platform success requires evidence from the relevant real boundary.

A portability claim is target-specific. Successful compilation for one target does not prove another target, and successful cross-compilation does not prove runtime behavior. Never report Windows, macOS, Linux, Android or iOS runtime support as PASS unless that target's required validation was actually executed.

For real-path playback and portability checks, retain a repeatable evidence artifact where practical. Depending on the target, record:

```text
repository commit
target triple / platform version
build configuration
SongCore ABI/version identity
input media hash
decoder and backend identity
decoded frame/sample counts
deterministic PCM hash or reference output where applicable
runtime log / trace
host-backend smoke result
device or simulator identity
exact build/run command
generated target artifact identity
```

For nondeterministic physical audio output, distinguish deterministic pipeline evidence from device/audible smoke evidence rather than pretending one proves the other.

During development, run the smallest relevant Rust/native/binding/target test loop. At a release, portability, playback-foundation or architecture gate, run the complete required target/configuration matrix. A green generic Cargo test suite is regression evidence only; it must not substitute for the specific realtime, backend, FFI or target claim under review.

---

# Documentation

`docs/README.md` is the documentation router and truth-class index.

Keep one current authority per durable fact. PBK-001 owns playback foundations; PBK-002 owns current vocabulary, Plugin/Fiber taxonomy, earned static playback composition and the D11 terminal-outcome authority designation; PBK-003 owns the stable Output Plugin / pluggable Host Render Backend boundary and backend-neutral `AudioOutput` contract interpretation.

`README.md`, `CONTEXT.md`, `docs/architecture/overview.md`, `docs/architecture/registry.yml`, website and diagrams are derived projections/routers; they may summarize, route and show status, but must not define or extend architecture semantics.

For SongCore portability work, `docs/architecture/songcore-binding-architecture.md` owns the one-core/many-bindings authority hierarchy, binding-class boundaries, target support maturity vocabulary and binding parity (Issue #173); the canonical SongCore ABI description remains `native/include/songcore.h`, and #172 owns target build/FFmpeg closure/artifact/provenance/packaging/release.

Historical evidence may retain historical terminology. Do not rewrite history for grep cleanliness.

---

# Authority resolution

If production code and normative authority differ:

```text
1. record the differential explicitly
2. determine whether code is wrong or authority should change
3. if authority changes, amend/replace it explicitly under review
4. only then is the new shape current architecture
```

Forbidden: “code is newer, therefore code wins”; “ADR is older, therefore code must mechanically copy it”; issue comments/tests/projections silently superseding authority.

---

# Delivery discipline

- inspect before assuming;
- keep changes narrow to the current gate;
- distinguish evidence from authority;
- do not perform unrelated cleanup;
- **do not silently preserve stale architecture or stale APIs merely for compatibility; resolve the authority differential explicitly;**
- prefer the smallest coherent model;
- stop after opening the requested PR unless explicitly authorized to merge.