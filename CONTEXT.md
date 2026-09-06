# CONTEXT.md

This file carries stable vocabulary and the repository mental model. It is not a substitute for current code, contracts, or task-specific evidence.

## Stable vocabulary

| Term | Meaning |
|---|---|
| Qianqian / 千千·现代 | A local-first, lightweight, cross-platform local music player. |
| Architecture v2 | The current product architecture epoch: Rust core + capability/profile composition + replaceable UiHost implementations. |
| Rust Core | The shared product architecture written in Rust. It contains the Base Kernel, Music Kernel, Presentation, and cohesive product-domain components as they are implemented. |
| Base Kernel | Domain-light composition/lifecycle layer. It wires capabilities and profiles; it should stay small until real complexity proves otherwise. |
| Music Kernel | The authority for player and music-domain semantics: track/session/state, control intent, queue behavior, transitions, buffering meaning, recovery, and user-visible playback truth. |
| Presentation | The seam that converts product/domain truth into stable UI-facing state and actions. UI frameworks should depend on this seam rather than Music Kernel internals. |
| Capability | A replaceable mechanism boundary justified by real implementation/platform/test/resource variation. A capability is not automatically a crate or dynamic plugin. |
| Profile | Explicit composition for one product/platform configuration. Profiles choose capability implementations. |
| UiHost | A presentation/rendering/input implementation capability. It owns UI mechanism, not music semantics. |
| Decoder | Encoded media -> canonical PCM. |
| Processing | PCM -> PCM transformation such as bypass, SRC, gain, EQ, limiter, or other DSP when authorized by real requirements. |
| AudioOutput | PCM -> physical device plus physical render/output evidence. Platform implementations may include WASAPI, CoreAudio, PipeWire, or Android audio mechanisms. |
| Canonical PCM | The semantic seam between decoding and downstream PCM processing/output. Its final representation and ownership contract must be established by implementation work, not guessed in advance. |
| Native substrate | C/C++/Rust/platform-specific mechanism code used behind capabilities. Implementation language does not determine semantic ownership. |
| Realtime island | The bounded audio hot path that must not depend on UI runtimes or general product-control decisions per callback/period. |
| PocketJS UiHost | Current first-choice UiHost for Windows and later Linux. It is a UI implementation choice, not a core dependency. |
| KuiklyUI UiHost | Current mobile UiHost choice for Android first, then iOS/HarmonyOS; macOS is a profile candidate and remains replaceable. |
| Reference Playback v1 | The frozen playback experiment proving a complete local-file -> decode -> PCM -> output path and important physical playback truths. |
| Pre-Rust archive | The complete repository state before Architecture v2 reset, preserved at `archive/pre-rust-v2` / `pre-rust-v2`. |
| Behavioral oracle | Historical verified behavior used to prevent Architecture v2 from weakening proven playback correctness while allowing ownership, language, directories, and types to change. |

## Core mental model

```text
Kernel owns semantics
Capability owns mechanism
Profile chooses implementation
```

The product should share semantics, not pixels or platform mechanisms.

Conceptually:

```text
                 Rust Core
        +-----------+------------+
        |           |            |
   Music Kernel  Presentation  Base Kernel
        |           |            |
        +-----------+------------+
                    |
                 Profile
        +-----------+-----------+
        |           |           |
     Decoder    AudioOutput    UiHost
        |           |           |
     media       platform      PocketJS /
   substrate     audio         KuiklyUI
```

The actual physical repository layout must be derived from current code and real implementation pressure. This diagram does not authorize pre-creating modules, crates, libraries, registries, or dynamic plugins.

## Historical refs

The old repository tree is intentionally not the current architecture.

```text
archive/pre-rust-v2
pre-rust-v2
```

The validated playback specimen is independently preserved at:

```text
research/playback-reference-v1
playback-reference-v1
```

Use those refs as evidence when needed; do not treat their directory structure or ownership as the new default.
