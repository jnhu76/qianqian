---
title: History
status: CURRENT
---

# Project History

Key milestones in Qianqian Architecture v2.

---

## Timeline

### R0 Bootstrap (Pre-Design)

Created foundational Rust crates:

- `qianqian-core` — empty base, MusicKernel state machine, three empty port traits
- `qianqian-runtime` — AppRuntime constructor composition
- `apps/headless` — CLI runner

**Bootstrap witnesses** (`AppRuntime::new()`, `with_audio_output()`, `audio_output()`) are not compatibility contracts.

### Component Boundary Audit — Issue #53

**PASS / CLOSED**

The component boundary audit justified three runtime components for the first Windows slice: Music, Decoder, AudioOutput. Frozen facts include:

- MVP runtime components: Music, Decoder, AudioOutput
- Capability graph: Music requires Decoder + PcmSink
- Activation rule: Music binds PcmSink at activation
- Withdrawal ordering: dependents finish teardown before provider final release
- FFmpeg closure authority: one shared authority, Decoder/Processing

[Read the audit →](https://github.com/jnhu76/qianqian/issues/53)
[PR #66 merged](https://github.com/jnhu76/qianqian/pull/66)

### Playback Reference v1 — Frozen

Frozen playback experiment proving local-file → decode → PCM → physical output. Preserved as a branch/tag, not on main.

**Evidence preserved:** SongCore ABI v1, PlayerEngine C ABI, commit-flush handshake, WASAPI renderer, null audio backend, FFmpeg build profiles.

### Composition Kernel K0 — Semantic Design — Issue #67

**MERGED via PR #68**

Semantic design for the generic Composition Kernel: five primitives (Context, Capability, Fiber, Effect, Reconcile), paper-backed by *A Programming Paradigm for Spatiotemporal Composability* (arXiv:2608.25512v1).

[Read the design →](/architecture/base-kernel)

### Composition Kernel K0 — Implementation — Issue #70

**IMPLEMENTED via PR #71**

74 oracle tests across six semantic guarantee groups. Domain-agnostic kernel knows nothing about music, PCM, FFmpeg, WASAPI, or UI.

Merged at commit `743eb86`.

---

## Authority Chain

```text
#53 COMPONENT-BOUNDARY-A0        PASS / CLOSED
        ↓
#67 COMPOSITION-KERNEL-0 DESIGN  MERGED via PR #68
        ↓
Corrective-4                     PASS_WITH_ONE_CORRECTIVE
        ↓
Corrective-5                     PRE-IMPLEMENTATION REVIEW
        ↓
#70 COMPOSITION-KERNEL-0 IMPL   MERGED via PR #71
```

<ProvenancePanel
  :authority="['docs/architecture/overview.md', 'docs/architecture/composition-kernel.md']"
  :decisions="[{ issue: 46 }, { issue: 53 }, { issue: 67 }, { issue: 70 }]"
  :pr="[{ pr: 66 }, { pr: 68 }, { pr: 69 }, { pr: 71 }]"
  lastVerified="743eb86"
/>
