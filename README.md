# Qianqian / 千千·现代

Qianqian is a local-first, lightweight, cross-platform music player.

The repository has entered **Architecture v2** and intentionally restarted from a clean `main` after freezing the first verified playback experiment.

## Architecture in 30 seconds

```text
                 Rust Core
        +-----------+------------+
        |           |            |
   Base Kernel   Music Kernel  Presentation
        |           |            |
        +-----------+------------+
                    |
                  Profile
       +------------+------------+
       |            |            |
    Decoder     AudioOutput     UiHost
       |            |            |
 native media   platform      PocketJS /
 substrate      audio         KuiklyUI
```

The design is governed by three rules:

> **Kernel owns semantics.**
>
> **Capability owns mechanism.**
>
> **Profile chooses implementation.**

Rust is the product architecture language. UI frameworks and native/media technologies sit behind replaceable boundaries.

## UI strategy

The UI is not a global framework decision. It is a `UiHost` capability selected by profile.

Current direction:

```text
Windows   -> PocketJS
Linux     -> PocketJS
Android   -> KuiklyUI
iOS       -> KuiklyUI
HarmonyOS -> KuiklyUI
macOS     -> KuiklyUI by default, replaceable by profile
```

The shared contract is product/presentation semantics, not pixels.

## Playback architecture

The first verified playback experiment established important facts that Architecture v2 must preserve, including the distinction between decoded, queued, submitted, and physically rendered audio, plus seek/flush, stale rejection, timeline, and physical-drain/ENDED behavior.

That experiment is preserved as reference evidence rather than copied forward as the new source layout.

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

These refs are historical/reference authorities. `main` is the Architecture v2 world.

## Repository entry points

- `AGENTS.md` — repository-wide agent governance.
- `CONTEXT.md` — stable vocabulary and mental model.
- `docs/README.md` — task-oriented documentation router.
- `docs/architecture/overview.md` — current Architecture v2 semantic overview.
- `CONTRIBUTING.md` — contribution entry point.

The implementation tree will be created by Architecture v2 work. Do not infer future directories from the archived repository.

## Build and run

Architecture v2 implementation has not yet established a canonical build/run command on `main`.

When the Rust workspace is bootstrapped, this section should point to the actual repository build authority rather than preserving historical commands.

## Scope

Qianqian is a music player, not a general media framework. New abstractions, codecs, platform services, UI capabilities, and framework machinery should be added because current product behavior or measured engineering pressure requires them, not because they might be useful someday.
