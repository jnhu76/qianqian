---
title: Qianqian — Project Control
status: CURRENT
---

<script setup>
import { projectState } from '../data/project-state.ts'
</script>

# Qianqian

A lightweight music player and a systems architecture laboratory.

---

## Current Frontier

**{{ projectState.currentFrontier }}**

## Last Milestone

**{{ projectState.lastMilestone }}**

<StatusBadge status="IMPLEMENTED" />

---

## Where We Are

<table class="layer-table">
  <tr>
    <td>Playback Reference</td>
    <td><StatusBadge status="HISTORICAL_EVIDENCE" /></td>
  </tr>
  <tr>
    <td>FFmpeg Research</td>
    <td><StatusBadge status="HISTORICAL_EVIDENCE" /></td>
  </tr>
  <tr>
    <td>Component Boundary</td>
    <td><StatusBadge status="FROZEN" /></td>
  </tr>
  <tr>
    <td>Base Kernel</td>
    <td><StatusBadge status="IMPLEMENTED" /></td>
  </tr>
  <tr>
    <td>Playback Kernel</td>
    <td><StatusBadge status="NEXT" /></td>
  </tr>
  <tr>
    <td>Decoder</td>
    <td><StatusBadge status="PLANNED" /></td>
  </tr>
  <tr>
    <td>Processing</td>
    <td><StatusBadge status="PLANNED" /></td>
  </tr>
  <tr>
    <td>AudioOutput</td>
    <td><StatusBadge status="PLANNED" /></td>
  </tr>
  <tr>
    <td>UI Host</td>
    <td><StatusBadge status="DEFERRED" /></td>
  </tr>
</table>

---

## System Architecture

See the canonical [system diagram](/architecture/#system-overview) for how Base Kernel, Playback Kernel, Decoder, Processing, AudioOutput, and UI Host relate.

---

## Recent Result

### Composition Kernel K0

<StatusBadge status="IMPLEMENTED" />

The generic Composition Kernel implements five primitives — Context, Capability, Fiber, Effect, Reconcile — with 70 kernel oracle tests (75 workspace tests) across six semantic guarantee groups. It is domain-agnostic: it knows nothing about music, PCM, FFmpeg, WASAPI, or UI payloads.

**Semantic guarantees validated:**

- Single-Fiber local cleanup
- Cross-Fiber independent removal
- Same-key contribution safety
- Explicit ordered/non-commutative interaction handling
- Provider-disappearance ordering
- Confluence after mutation history

<ProvenancePanel
  :authority="['docs/architecture/composition-kernel-0-design.md', 'docs/architecture/composition-kernel-0-implementation-adr.md']"
  :decisions="[{ issue: 67 }, { pr: 68 }, { pr: 69 }]"
  :implementation="[{ issue: 70 }, { pr: 71 }]"
  :evidence="['crates/qianqian-kernel/tests']"
  lastVerified="743eb86"
/>

---

## Experiment Highlight

### FFmpeg Minimization

<StatusBadge status="HISTORICAL_EVIDENCE" />

How much FFmpeg does a music player actually need? Decoder and Processing share one FFmpeg closure authority. Codec coverage is provider configuration, not a runtime layer. Two build profiles prove the closure is minimizable.

[Read the experiment →](/experiments/ffmpeg-minimization)

---

## Next Three Questions

<template v-for="(q, i) in projectState.nextQuestions" :key="i">
1. {{ q }}
</template>

---

<ProvenancePanel
  :authority="['AGENTS.md', 'CONTEXT.md', 'docs/architecture/overview.md']"
  :decisions="[{ issue: 46 }, { issue: 53 }, { issue: 67 }, { issue: 70 }]"
  lastVerified="743eb86"
/>
