---
title: Plugin Graph
status: NEXT
---

# Plugin Graph

<StatusBadge status="NEXT" />

The plugin graph shows logical component boundaries and their capability dependencies.

<ClaimBadge role="interpretation" />

> Logical plugin boundary **!=** crate/shared-library boundary.

---

## Current Intended Shape

```mermaid
flowchart LR
    DEC["Decoder<br/>(encoded media → PCM)"]
    MUSIC["Music / Playback<br/>(domain semantics)"]
    AOUT["AudioOutput<br/>(PCM → physical)"]

    DEC -->|"provides Decoder capability"| MUSIC
    MUSIC -->|"requires Decoder"| DEC
    MUSIC -->|"binds PcmSink"| AOUT
    AOUT -->|"provides PcmSink"| MUSIC
```

---

## Future Components

```mermaid
flowchart TB
    subgraph Future["Future Components"]
        PROC["Processing<br/>(PCM → PCM)"]
        UH["UiHost<br/>(presentation)"]
    end

    MUSIC2["Music"] -.->|"future: PCM → PCM"| PROC
    PROC -.->|"future: PCM → physical"| AOUT2["AudioOutput"]
    MUSIC2 -.->|"future: poll snapshot"| UH
```

---

## Component Dependency Matrix

| Component | Requires | Provides |
|-----------|----------|---------|
| Music | Decoder, PcmSink | PlaybackControl, PlaybackSnapshot |
| Decoder | Nothing | Decoder capability |
| AudioOutput | Nothing | PcmSink, OutputDeviceDiscovery |
| Processing *(future)* | PCM in | PCM out |
| UiHost *(future)* | Snapshot | User input |

---

## Boundary Justification

Every component boundary must answer:

- What state/resources does it own?
- What capabilities does it require?
- What capabilities does it provide?
- What operations cross the boundary?
- Which operations commute?
- Where is non-commutative order explicit?

A different feature name is not evidence of component independence.

---

<ProvenancePanel
  :authority="['docs/architecture/component-boundary-a0.md', 'docs/architecture/overview.md']"
  :decisions="[{ issue: 53 }, { pr: 66 }]"
  :evidence="['crates/qianqian-core/src/ports.rs']"
/>
