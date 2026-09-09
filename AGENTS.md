# AGENTS.md

Qianqian is a local-first, lightweight, cross-platform music player and an architecture experiment in composable runtime design. This file defines repository-wide rules for coding agents. It is a governance entry point, not a feature manual.

## Start here

Before changing code or long-lived documentation:

1. Read the current issue/task.
2. Read `CONTEXT.md` for stable vocabulary.
3. Use `docs/README.md` to load only the minimum relevant documentation.
4. Read `docs/architecture/overview.md` before changing architecture boundaries.
5. For component/plugin/composition work, read `docs/architecture/composition-kernel.md` and the current boundary/design issue.
6. For playback work, read `docs/adr/ADR-PBK-001.md` (ACCEPTED — registered playback-specific ARCH-003 authority; architecture acceptance is not implementation completion); use `specs/playback/*` only when the task actually needs state-collision evidence.
7. Audit current repository reality before assuming a path, API, module, crate, build rule, or prior design is still authoritative.

Do not recursively preload archived source/docs.

## Architecture constitution

> **Kernel controls reachability, ownership and lifetime; it should not own application payloads.**
>
> **Semantic authorities own the meaning of their facts.**
>
> **Capabilities expose contracts; providers own mechanisms.**
>
> **Fibers own plugin-instance lifetime.**
>
> **Effects make kernel-visible mutation attributable and reversible where reversal is actually valid.**
>
> **Profiles declare desired composition; Reconcile determines the running Fiber graph.**

And:

> **Inverse is not enough: independent removal also requires independence/commutativity, or explicit ordering when operations do not commute.**

Rust is the product architecture language.

The generic Composition Kernel must not know music, PCM, FFmpeg, WASAPI, PocketJS, KuiklyUI, track/playlist semantics, playback cursor/window/generation/fence state, or UI payload schemas.

## Boundary-first design rule

Do **not** begin a plugin/composition task by inventing `Context`, `Fiber`, `Effect`, registry, or loader APIs.

The required design order is:

```text
Component Granularity
        ↓
Capability / dependency boundary
        ↓
Interaction Algebra
        ↓
Effect / System Boundary
        ↓
Global lifecycle ordering
        ↓
Confluence oracle
        ↓
Context / Fiber / Effect / Reconcile implementation
```

Gate chain:

```text
#53 COMPONENT-BOUNDARY-A0        PASS / CLOSED (audit: docs/architecture/component-boundary-a0.md)
        ↓
#67 COMPOSITION-KERNEL-0 DESIGN  semantic design MERGED via PR #68
                                  (docs/architecture/composition-kernel-0-design.md)
        ↓
#70 COMPOSITION-KERNEL-0 IMPL    IMPLEMENTED via PR #71 (70 kernel tests / 75 workspace tests, 743eb86)
        ↓
ADR-PBK-001 PLAYBACK ARCH        ACCEPTED (PR #78 + #79; accepted via #81)
```

A different feature name, Rust type, crate, or file is not evidence that something deserves its own plugin.

## Component boundary discipline

Every proposed long-lived plugin/component boundary must be able to answer:

```text
What state/resources does it own?
What capabilities does it really require?
What capabilities does it provide?
What operations/data edges cross the boundary?
Which observations are intentionally public?
Which shared operations commute?
Where is non-commutative ordering expressed?
Which effects are reversible/transactional/compensatable/irreversible?
Which provider disappearance invalidates which consumers?
Does further splitting justify its configuration/naming/cognitive cost?
```

If `A requires B` and `B requires A`, first audit whether the relationship should be decomposed through an integration/mediation component. Do not automatically accept a cycle, but do not split infinitely merely to make the graph prettier.

## Everything is a Plugin

“Everything is a Plugin” does not mean every feature becomes one plugin, every component is a DLL, or everything is hot-loaded.

It means every long-lived product capability that ultimately participates in runtime composition must enter through the common composition/lifecycle protocol once its boundary has been justified.

The runtime composition unit is a **Fiber**, not the source package.

No product capability gets a privileged bypass merely because storing it directly in `AppRuntime` is convenient.

## Composition Kernel primitive budget

The generic kernel is expected to stay centered on:

```text
Context
Capability
Fiber
Effect
Reconcile
```

Do not add another generic primitive without an architecture issue demonstrating that these concepts cannot express a required invariant cleanly.

The primitive budget was satisfied by the K0 implementation (PR #71). The boundary-design gate (#53) has passed.

## Context is not a data bus

`Context` is a capability namespace/dependency view. It controls reachability and dependency validity.

It must not become:

```text
global product state
universal event bus
message broker
PCM/audio buffer transport
UI payload store
undeclared get-anything service locator
```

> **Capability plane != Data plane.**

Once a capability is resolved/bound, ordinary business payload should flow through the service contract or direct/pre-bound data edge.

Realtime audio data must flow directly, for example:

```text
Encoded Media -> Decoder -> DSP/Processing -> AudioOutput
```

not through Context/event dispatch per block.

## Capability definition and provider separation

Across plugin seams, consumers depend on capability/service definitions, not concrete provider classes.

Preferred topology:

```text
Capability Definition
        ^
   +----+----+
   |         |
Provider  Consumer
```

Avoid hard-wiring:

```text
MusicPlugin -> WasapiOutput
UiPlugin    -> ConcreteMusicKernel
```

when the semantic dependency is on a replaceable contract.

## Interaction algebra

A disposer proves local revertibility, not cross-component composability.

### Different keys

An operation associated with one shared key/capability must not secretly read/write unrelated shared keys or hidden globals. Cross-key dependencies must be explicit.

### Same key

When multiple Fibers contribute to one shared interface, do not assume commutativity. The interface/provider must define how contributions compose and how one caller removes only its own contribution.

Contribution-oriented shape may be useful when semantically correct:

```text
register(value) -> opaque token
unregister(token)
```

### Ordered relationships

If order changes observable behavior:

```text
A -> B != B -> A
```

it is **not** an independent effect relation.

Freeze:

> **Commutative relation -> may compose as independent effects.**
>
> **Non-commutative relation -> explicit dependency/order/integration structure.**

DSP/pipeline ordering is a mandatory adversarial example. Never derive semantic order from registration timing, hash iteration, or incidental mount order.

## Effect and system boundary

Kernel-visible mutation must have an explicit owner.

Local reversible examples include:

```text
capability binding
listener/callback registration
timer registration
watcher/local handle
buffer/resource allocation owned inside the system boundary
```

(Child-fiber mounting is not an example: child mounting is out of K0 scope.)

Within one Fiber, owned effects normally unwind in reverse/LIFO order.

Do not pretend every action is reversible. Classify actions as needed:

```text
Reversible
Transactional
Compensatable
Irreversible / emitted outside system boundary
```

These labels are a system-boundary/action taxonomy for reasoning about actions — not runtime variants of a kernel Effect type; the frozen K0 Effect has exactly one shape (`composition-kernel-0-design.md` §H.5/§H.7).

Already-rendered sound cannot be “unplayed”. `Everything is Plugin` does not mean `Everything is rollbackable`.

## Observational equivalence

Restoration correctness is judged through public/architecturally relevant behavior, not private bit identity.

After removing contribution A, the system should be observationally equivalent to a world where A never contributed while independent B/C contributions remain.

Do not overfit tests to opaque token numbers, allocator layouts, private generations, or incidental IDs.

## Global lifecycle ordering

Provider disappearance must be dependency-aware.

Required semantic ordering:

```text
provider begins withdrawal
        ↓
provider stops satisfying new resolution
        ↓
dependents are invalidated and deactivate
        ↓
dependents finish teardown while required teardown access remains valid
        ↓
provider finally removes binding/resources
```

Do not destroy a provider first and let consumers discover the failure on a later call.

## Confluence

A core future correctness oracle is:

> **After any legal load/unload/replacement history reaches quiescence, the observable runtime is equivalent to a clean construction of the final desired composition.**

This is stronger than “no crash” and stronger than “all disposers ran”.

Confluence tests should compare relevant public truth such as reachable capabilities, Fiber lifecycle, service behavior, and absence of ghost contributions.

## Playback authority discipline

For ARCH-003, `docs/adr/ADR-PBK-001.md` is ACCEPTED and is the registered playback-specific authority (together with `docs/architecture/overview.md`). `component-boundary-a0.md` remains closed #53 historical evidence; its conflicting playback-specific ownership/granularity conclusions are superseded for new playback implementation. Do not combine the two playback models when implementing new code. Architecture acceptance is not implementation authorization: the ADR still gates production implementation behind the deterministic executable oracle.

The accepted model does **not** collapse playback into one giant `MusicKernel`.

```text
MusicComponent   = composition lifecycle root
MusicKernel      = music/product semantic authority
TransportKernel  = playback temporal authority
```

Nested runtime:

```text
MusicComponent
├── MusicKernel
├── TransportKernel
└── TrackSession(s)
    └── DecodeSession(s)
```

`TrackSession` is media identity/source lifetime. Each `DecodeSession` owns one independently advancing decoder cursor/handle. Same-track seek may therefore have multiple DecodeSessions under one TrackSession.

Active/Prepared are temporal roles inside TransportKernel, not independent plugins or standalone lifetime resources.

Raw playback evidence (Decoder EOF/seek landing/late decode and AudioOutput submitted/rendered/fence evidence) is interpreted by TransportKernel. MusicKernel receives typed derived facts and owns product decisions.

Never use:

```text
result.generation != global_current_generation => stale
```

Staleness is admission/role based.

Keep:

```text
decoded != queued != submitted != rendered
logical invalidation != physical stop
```

A hard discontinuity that must kill old submitted audio requires a definitive Physical Fence/flush outcome. A claimed fence cannot be rewritten by later intent. Natural EOF/drain must not terminalize away active temporal state needed by an in-flight fence.

## Realtime boundary

The realtime audio path is a data-plane mechanism island.

Per callback/block do not perform:

```text
Context lookup
capability resolution
Fiber reconciliation
arbitrary generic event dispatch
filesystem/network I/O
UI/JS/managed-runtime round trips
unbounded allocation/blocking
```

Future graph changes should be prepared on the control plane and published at an RT-safe boundary.

## UI boundary

UiHost is an ordinary plugin/capability candidate, not an architecture authority.

Current platform intent remains:

```text
Windows   -> PocketJS
Linux     -> PocketJS
Android   -> KuiklyUI
iOS       -> KuiklyUI
HarmonyOS -> KuiklyUI
macOS     -> KuiklyUI candidate / replaceable
```

UI does not own music semantics and never participates in realtime correctness.

## R0 bootstrap is provisional

RUST-ARCH-R0 introduced bootstrap witnesses such as:

```text
qianqian-core::base
qianqian-runtime::AppRuntime
AppRuntime::new()
with_audio_output()
```

They are not compatibility contracts.

The Base Kernel K0 implementation (PR #71) is authorized to replace R0 bootstrap shapes rather than preserve them for compatibility.

## Formalization policy

Formal verification is risk-driven. Ask:

> **Which independently legal states/events can interleave and collide into an illegal state?**

If that question has no concrete answer, prefer type/ownership/module constraints, unit/property tests, or static checks. Do not build a formal model merely because another architectural noun exists.

Playback's blocking formal core is intentionally limited to Dual Window, Generation admission, Physical Fence, submitted/rendered accounting, and EOF/drained/ENDED terminalization. Models are evidence under explicit assumptions, not a second architecture authority.

## Work mode

Use reality-first development:

- inspect before assuming;
- distinguish design gate from implementation gate;
- distinguish composition lifecycle root, immediate lifetime owner, and semantic authority;
- make assumptions explicit;
- use adversarial cases, especially cycles, shared-state mutation, ordered DSP/pipelines, provider disappearance, temporal interleavings, and history-vs-clean-build confluence;
- do not create hidden globals, parallel registries, or service-locator escape hatches;
- do not move application payloads into Context;
- do not treat a disposer as proof of independent removal;
- do not perform unrelated cleanup.

If current reality contradicts the task premise, surface the conflict rather than silently inventing a workaround.

## Historical evidence

Historical evidence is preserved as **git refs, not working-tree directories**:

Pre-Rust repository:

```text
git tag pre-rust-v2          (branch: archive/pre-rust-v2)
```

Frozen playback reference:

```text
git tag playback-reference-v1    (branch: research/playback-reference-v1)
```

Neither tag is present in the current working tree; inspect via `git show <tag>:<path>`. These are evidence sources, not current architecture/source-layout authority.

## Verification

Report what was actually verified and what was not.

For boundary-design work, verification is an evidence-backed design audit, not green Cargo tests.

For later Composition Kernel work, tests must distinguish at least:

```text
single-Fiber local cleanup
cross-Fiber independent removal
same-key contribution safety
explicit handling of ordered/non-commutative interaction
provider-disappearance ordering
confluence after mutation history
```

For playback temporal work, use the existing core formal checks when a change touches those high-risk state interactions; do not automatically expand the model set.

Never mark an unrun platform/device check PASS.

## Documentation

`docs/README.md` is the documentation router. Keep one clear authority per durable fact and link instead of copying whole specifications everywhere.

## Local AGENTS policy

No local `AGENTS.md` by default. Create one only for a genuine stable local rule that cannot be expressed by root governance and is explicitly justified by the current task.

## Delivery discipline

Keep commits/PRs focused, state scope and non-scope, and STOP at the current gate. Do not automatically continue into the next phase.
