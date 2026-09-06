# Qianqian / 千千·现代

Qianqian is a local-first, lightweight, cross-platform music player and a testbed for a Rust composability kernel.

The repository is in **Architecture v2**. The first verified playback experiment was frozen, `main` was reset, and the new implementation is being rebuilt around a generic Composition Kernel plus domain plugins.

## Architecture in 30 seconds

```text
                         CONTROL PLANE

                 Profile / desired tree
                          |
                          v
                      Reconcile
                          |
                          v
                     Fiber Graph
                          |
                 provide / require
                    owned effects
                          |
                          v
              +-------------------------+
              |   Composition Kernel    |
              | Context / Capability    |
              | Fiber / Effect          |
              | Reconcile               |
              +------------+------------+
                           |
                    resolve / bind
                           |
-------------------------------------------------------------
                           |
                         DATA PLANE
                           |
       +-------------------+-------------------+
       |                   |                   |
       v                   v                   v
   Music Plugin       Audio Runtime        UiHost Plugin
       |                   |                   |
 domain semantics    direct audio graph    presentation/UI
                           |
          Media -> Decoder -> DSP -> Output -> device
```

The central rule is:

> **Kernel controls reachability, ownership and lifetime; it should not own application payloads.**

Related rules:

> **Domain kernels own domain semantics.**
>
> **Capabilities expose contracts; providers own mechanisms.**
>
> **Fibers own plugin-instance lifetime.**
>
> **Effects make kernel-visible mutation attributable and reversible where reversal is valid.**
>
> **Profiles declare desired composition; reconciliation determines the running fiber graph.**

Rust is the product architecture language.

## Everything is a plugin

In Qianqian, “everything is a plugin” means long-lived product capabilities participate in one composition/lifecycle protocol:

```text
Plugin -> Fiber
           |
           +-- requires capabilities
           +-- provides capabilities
           +-- owns effects
           +-- activates/deactivates with dependency availability
```

It does **not** mean every component is a dynamic library or must be runtime-downloaded.

Music, decoder, processing, audio output, presentation, UI host, library, media keys, and future product capabilities should not receive privileged runtime bypasses merely because they are convenient to construct directly.

## Context is not a bus

Context is the capability namespace/dependency view. It decides **who can reach whom** and **which provider satisfies which requirement**.

It must not carry all product data.

```text
Capability plane != Data plane
```

After binding, business data normally flows through the service contract or a direct data edge.

For audio, PCM must flow directly:

```text
Decoder -> Processing -> AudioOutput
```

not:

```text
Decoder -> Context/EventBus -> Processing -> Context/EventBus -> Output
```

Realtime audio must not perform Context lookup, dependency resolution, reconciliation, arbitrary event dispatch, or UI/runtime round trips per callback/block.

## Domain semantics

`MusicKernel` remains the authority for music/player meaning, but it is a **domain kernel**, not the global composition kernel.

Conceptually:

```text
Music Plugin
   |
   +-- owns MusicKernel
   +-- requires media/audio capabilities
   +-- provides transport/player-state capabilities
```

Mechanism layers provide evidence; the domain semantic owner decides what that evidence means to the product.

## UI strategy

UI is an ordinary plugin/capability axis.

Current direction:

```text
Windows   -> PocketJS UiHost
Linux     -> PocketJS UiHost
Android   -> KuiklyUI UiHost
iOS       -> KuiklyUI UiHost
HarmonyOS -> KuiklyUI UiHost
macOS     -> KuiklyUI by default, replaceable by composition/profile
```

The shared contract is product/presentation semantics, not pixels.

## Current implementation status

RUST-ARCH-R0 established the first compiling Rust workspace:

```text
qianqian-core
qianqian-runtime
qianqian-headless
```

The R0 `base` module and constructor-only `AppRuntime` composition are intentionally provisional. They are not compatibility contracts and may be replaced by the Composition Kernel implementation.

Build/test authority:

```bash
cargo run -p qianqian-headless
cargo test --workspace
```

## Historical preservation

Complete pre-Rust repository:

```text
branch: archive/pre-rust-v2
tag:    pre-rust-v2
```

Frozen playback reference:

```text
branch: research/playback-reference-v1
tag:    playback-reference-v1
```

The playback reference is a behavioral oracle, not a source-layout template.

## Repository entry points

- `AGENTS.md` — repository-wide agent governance and hard invariants.
- `CONTEXT.md` — stable vocabulary and mental model.
- `docs/README.md` — task-oriented documentation router.
- `docs/architecture/overview.md` — Architecture v2 semantic overview.
- `docs/architecture/composition-kernel.md` — Composition Kernel authority.
- `CONTRIBUTING.md` — contribution entry point.

## Scope

Qianqian is still a music player, not a generic framework product. The Composition Kernel exists because this repository deliberately tests whether spatiotemporal/plugin composability can survive a real media runtime with strict lifecycle and realtime constraints.

Kernel mechanisms should stay generic; music/media/UI policy should stay outside the kernel.
