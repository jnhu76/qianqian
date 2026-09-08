# CONTEXT.md

This file carries stable vocabulary and the repository mental model. It is not a substitute for current code, contracts, ADRs, or task-specific evidence.

## Stable vocabulary

| Term | Meaning |
|---|---|
| Qianqian / 千千·现代 | A local-first, lightweight, cross-platform music player and a testbed for composable runtime architecture. |
| Architecture v2 | Boundary-first plugin architecture: component/capability/interaction boundaries are designed first; a Rust Composition Kernel controls reachability, ownership and lifetime. |
| Boundary-first design | The rule that component granularity, capability dependencies, interaction algebra, effect/system boundary, lifecycle ordering, and confluence are decided before kernel API/runtime machinery. |
| Component Granularity | The architecture decision of what deserves to be one runtime component/plugin. Feature names do not determine granularity; ownership, dependencies, interaction semantics, ordering, and cognitive cost do. |
| Integration / Mediation Component | A component introduced to make a real ordering or cross-component relationship explicit, especially when an apparent bidirectional dependency should be decomposed into clearer one-way bindings. |
| Composition Kernel | The generic control-plane kernel. It may know Context, Capability, Fiber, Effect, and Reconcile; it must not know music, PCM, UI payloads, FFmpeg, playback cursor/window/generation state, or platform product semantics. |
| Context | The capability namespace/dependency view visible to a Fiber. It controls reachability/availability; it is not an application payload bus or global state bag. |
| Capability | A named/typed service contract that can be required/provided. Capability identity is distinct from concrete provider implementation. |
| Service Definition | The domain-facing interface/contract represented by a capability. Consumers depend on the definition across plugin seams. |
| Provider | A plugin/Fiber that makes a capability/service available while it satisfies the lifecycle contract. |
| Consumer | A plugin/Fiber declaring a requirement on a capability/service. |
| Plugin | A component definition participating in the common composition/lifecycle protocol after its boundary has been justified. Plugin does not imply dynamic library. |
| Fiber | A live plugin instance with identity, scope, requirements, provided capabilities, owned effects, and lifecycle state. It is the runtime composition unit, not the crate/package. |
| Effect | Kernel-visible mutation/resource provenance owned by a Fiber, with inverse/teardown where reversal is valid. An inverse alone does not prove cross-Fiber composability. |
| Reconcile | Moves the running Fiber graph toward desired composition while respecting dependency/lifecycle invariants. It is an implementation mechanism used after boundary design. |
| Profile | Desired product/platform composition; eventually declares the plugin graph rather than a privileged imperative boot sequence. |
| Control Plane | Composition concerns: desired graph, capability resolution, Fiber lifecycle, effect ownership, dependency invalidation, reconcile. |
| Data Plane | Product payload flow after binding: service calls, media/audio graph edges, domain events/streams, UI/domain payloads. |
| Capability plane != Data plane | Context determines who can reach whom; payload normally flows directly through the resolved service/data edge. |
| Operation Locality | An operation on one shared key/capability should only read/write the state represented by that contract. Hidden cross-key/global mutation breaks the model. |
| Independence | One component's removal preserves other independent components' observable contributions. Revertibility alone does not imply independence. |
| Commutativity | Shared operations and their inverses can be reordered without changing relevant observable behavior. Same-key mutation is not assumed commutative. |
| Interaction Algebra | Classification of cross-component operations as independently composable/commutative versus order-sensitive/non-commutative, together with the explicit ordering structure for the latter. |
| Non-commutative Relation | An interaction whose observable behavior depends on order. It must use explicit dependency/order/integration structure, not implicit registration order. |
| Contribution-oriented Interface | An interface where callers receive an opaque handle/token for their own contribution and can remove only that contribution, when this matches the semantics. |
| Observational Equivalence | Correctness criterion comparing public/relevant behavior rather than bit-identical private state. |
| Recoverable System Boundary | State/resources the runtime can legitimately own and restore/transactionally manage/compensate. Emissions outside the boundary may be irreversible. |
| Confluence | After a legal sequence of composition changes reaches quiescence, the observable result is equivalent to a clean build of the final desired composition. |
| Quiescence | A point at which relevant lifecycle/reconciliation transitions have settled, allowing confluence comparison. |
| Composition Lifecycle Root | A composed component whose episode bounds all subordinate runtime resources. For playback, this is `MusicComponent`. |
| Immediate Lifetime Owner | The direct parent responsible for a nested runtime resource lifetime. This relation is distinct from semantic authority. |
| Semantic Authority | The unique interpreter/writer of a semantic fact. Being an authority does not imply being an independent Composition plugin. |
| Domain Kernel | A semantic-authority role inside a domain component. In playback this role is split between `MusicKernel` and `TransportKernel`; neither is the global composition authority. |
| MusicComponent | The composed playback-domain lifecycle root. It binds replaceable provider capabilities such as Decoder and AudioOutput/PcmSink and contains subordinate playback runtime state. |
| MusicKernel | Music/product semantic authority: playback-state meaning, selection, playlist/repeat/shuffle policy, playback-intent meaning, and product decisions after terminal transport outcomes. It is not the playback timeline authority. |
| TransportKernel | Playback temporal authority: cursor/MediaSpan meaning, Active/Prepared roles, Generation admission, discontinuity execution, Physical Fence coordination, and interpretation of raw playback evidence. |
| TrackSession | Media identity/source lifetime root. It may contain 0..N DecodeSession values and does not itself mean one exclusive decoder cursor. |
| DecodeSession | One independently advancing decoder cursor/handle plus generation-local decode/seek/EOF state. Its immediate lifetime owner is its TrackSession. |
| Active / Prepared | Temporal roles/slots inside TransportKernel. MVP allows one Active and at most one Prepared. They are not standalone plugins or independent lifetime resources. |
| Generation admission | A generation is valid for an operation while its owning temporal role admits it. Stale does not mean “not equal to one global current generation”. |
| MediaSpan | Media-time/provenance identity across the PCM data plane. Buffer/block identity is not timeline identity. |
| Physical Fence | Physical cut/flush handshake required when old submitted audio must cease. Logical invalidation or Generation retirement cannot substitute for it. |
| Presentation | Domain-to-UI seam. Product/domain data, not a generic Composition Kernel primitive. |
| UiHost | UI rendering/input capability/plugin candidate. Owns UI mechanism, not music semantics or realtime correctness. |
| Decoder | Encoded media -> canonical PCM provider/capability seam. Exact implementation remains separate from the generic Composition Kernel. |
| Processing / DSP | Ordered PCM -> PCM transforms. Individual EQ/Gain/Resampler/Mixer nodes are not automatically independent Composition plugins. |
| PlayerGain | Main player volume in the PCM processing graph; ordinary parameter updates do not change playback generation. |
| DeviceVolume | Optional AudioOutput/platform device/system volume control, distinct from PlayerGain. |
| AudioOutput | Canonical PCM -> physical output + submitted/rendered/fence evidence. Device/provider implementation remains separate from generic kernel semantics. |
| Audio Runtime | Future specialized runtime/graph owner that may own AudioGraph, clock, buffers, format negotiation, RT scheduling, and graph publication if such a boundary is earned. |
| Realtime island | Bounded audio hot path using pre-bound data edges; no per-block generic Context resolution/reconcile. |
| Event Service | Product/domain event semantics, if needed, should begin as a normal service/plugin rather than an automatic generic-kernel primitive. |
| Everything is a Plugin | All justified long-lived runtime capabilities ultimately obey a common composition/lifecycle protocol; it does not mean every feature name, DSP node, or payload is exactly one plugin. |
| Reference Playback v1 | Frozen playback experiment proving a local-file -> decode -> PCM -> physical output path and important playback truths. Preserved as git tag `playback-reference-v1` (branch `research/playback-reference-v1`); not present in the working tree. |
| Pre-Rust archive | Complete repository state before Architecture v2 reset: git tag `pre-rust-v2` (branch `archive/pre-rust-v2`); not present in the working tree. |
| Behavioral oracle | Historical verified behavior used to preserve playback correctness while architecture changes. |
| Formal exploration | Risk-driven state-space evidence for interactions where individually legal states/events can collide. It is not a second architecture authority. |

## Core mental model

The shortest constitution is:

> **Kernel controls reachability, ownership and lifetime; it should not own application payloads.**

But the design process starts **before** the Kernel:

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
Composition Kernel implementation
```

#53 COMPONENT-BOUNDARY-A0 is PASS/CLOSED (`component-boundary-a0.md`). The #67 COMPOSITION-KERNEL-0 semantic design is MERGED (PR #68, `composition-kernel-0-design.md`). The Base Kernel K0 is IMPLEMENTED (PR #71, 70 kernel tests / 75 workspace tests, 743eb86). Playback ADR-PBK-001 is `PROPOSED / FORMAL CORE PASS` (PR #78 + #79).

## Interaction mental model

For independently composable relationships:

```text
commutative operations
        ↓
independent contributions
        ↓
independent removal
```

For order-sensitive relationships:

```text
A -> B != B -> A
        ↓
explicit order / dependency / integration component
```

Do not use registration timing, container iteration, or mount order as hidden product semantics.

## Control plane / data plane

```text
                      CONTROL PLANE

             desired component graph
                       |
                       v
                Composition Kernel
             Context / Capability / Fiber
                 Effect / Reconcile

-------------------------------------------------
                       |
                  resolve/bind
                       v
                     DATA PLANE

   Encoded Media -> Decoder -> DSP/Processing -> AudioOutput

   User/Input -> domain/presentation service -> provider
```

Context establishes reachability. It does not carry PCM blocks or playback temporal truth.

## Playback mental model

The model below is ADR-PBK-001's (**PROPOSED / FORMAL CORE PASS**) proposed playback structure — the registered ARCH-003 authority has not yet migrated to it.

```text
MusicComponent
├── MusicKernel       music/product semantics
├── TransportKernel   playback temporal semantics
└── TrackSession(s)
    └── DecodeSession(s)
```

Same-track seek may legitimately have two decoder cursors under one TrackSession:

```text
TrackSession A
├── DecodeSession gen17 -> Active
└── DecodeSession gen18 -> Prepared
```

Raw playback evidence is interpreted once:

```text
Decoder EOF / seek landing / late decode
AudioOutput submitted / rendered / fence verdict
                    |
                    v
             TransportKernel
                    |
            typed derived facts
                    v
               MusicKernel
```

Never create a second timeline authority by letting MusicKernel independently reinterpret raw temporal evidence.

Generation validity is role/admission based, not global-current equality.

## Physical-time mental model

```text
decoded != queued != submitted != rendered
logical invalidation != physical stop
```

A hard discontinuity follows the semantic shape:

```text
prepare
  -> prime
  -> close old admission
  -> Physical Fence
  -> promote / stop / fail closed
  -> retire old generation
```

A claimed fence cannot be rewritten by later intent. Natural EOF/drain must not terminalize away active temporal state needed by an in-flight fence.

## Lifecycle mental model

Provider removal is not “delete binding, then see who fails”.

```text
provider starts withdrawal
        ↓
no new resolution sees it as available
        ↓
dependents deactivate / tear down
        ↓
provider releases final binding/resources
```

The implementation may refine this sequence, but dependent safety is the invariant.

## Confluence mental model

Given a legal history:

```text
load A
insert B
replace C
remove B
settle
```

compare the settled runtime to:

```text
clean root
  ↓
build final desired graph directly
```

Relevant observable **composition** truth should match even if private IDs/tokens/layout differ. Historical playback position/state is domain/temporal continuity, not automatic composition-confluence truth.

## Formalization mental model

Ask first:

> **Which independently legal states/events can interleave and collide into an illegal state?**

If that question has no concrete answer, prefer type-system rules, ownership, module boundaries, unit/property tests, or static checks over a new formal model.

Playback's blocking formal core is deliberately narrow: Dual Window, Generation admission, Physical Fence, submitted/rendered accounting, and EOF/drained/ENDED terminalization.

## Current code status

Current Rust workspace includes:

```text
qianqian-core
qianqian-kernel
qianqian-runtime
qianqian-headless
```

The generic Base Kernel K0 is implemented. Product code carries separate `MusicKernel` and `TransportKernel` authority shells; final playback API/state representation remains deliberately unfrozen.

`qianqian-core::base`, `qianqian-runtime::AppRuntime`, and bootstrap convenience APIs are not compatibility contracts.

## Historical refs

Historical evidence is preserved as **git refs, not working-tree directories**:

```text
pre-rust-v2              git tag (branch: archive/pre-rust-v2)
playback-reference-v1    git tag (branch: research/playback-reference-v1)
```

Inspect via `git show <tag>:<path>`. Use these as opt-in evidence, never as automatic source-layout/ownership authority.
