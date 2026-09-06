# AGENTS.md

Qianqian is a local-first, lightweight, cross-platform music player. This file defines repository-wide rules for coding agents. It is a governance entry point, not a feature manual or an architecture dump.

## Start here

Before changing code or long-lived documentation:

1. Read the current issue or task.
2. Read `CONTEXT.md` for stable vocabulary.
3. Use `docs/README.md` to load only the minimum relevant documentation.
4. Read `docs/architecture/overview.md` before changing architecture boundaries.
5. Audit the current repository before assuming a path, module, API, or build rule exists.

Do not recursively preload old documentation or archived source trees.

## Architecture invariants

Architecture v2 is governed by three rules:

> **Kernel owns semantics.**
>
> **Capability owns mechanism.**
>
> **Profile chooses implementation.**

The product architecture language is Rust.

The Rust core owns product and music semantics. Platform/native/UI technologies are implementations behind explicit boundaries; they do not become semantic authorities because of their implementation language.

The intended high-level flow is:

```text
Rust Base Kernel
      |
Rust Music Kernel
      |
Rust Presentation
      |
    UiHost
```

with independent mechanism capabilities such as Decoder, Processing, AudioOutput, FilePicker, and other platform services only when real variation or ownership justifies them.

## UI boundary

UI is a `UiHost` capability.

A UI host may render presentation state and translate user input into presentation actions. It must not directly own or inspect:

- decoder internals;
- FFmpeg objects;
- PCM buffers or pointers;
- audio-device handles;
- realtime generation/epoch internals;
- Music Kernel private state.

Current platform intent is:

```text
Windows   -> PocketJS UiHost
Linux     -> PocketJS UiHost
Android   -> KuiklyUI UiHost
iOS       -> KuiklyUI UiHost
HarmonyOS -> KuiklyUI UiHost
macOS     -> KuiklyUI UiHost by default, replaceable by profile
```

This is a profile choice, not permission to make UI frameworks part of the core architecture.

## Realtime boundary

UI and general product code never participate in the audio realtime hot path.

Do not design a steady-state path such as:

```text
audio callback -> JS/UI/managed runtime -> decision -> audio device
```

Realtime mechanisms must use bounded work, pre-resolved references, appropriate preallocation, and minimal local physical state. The Music Kernel consumes evidence at bounded control/event boundaries and decides product meaning.

## Native media boundary

Decoder and Processing are logically distinct capabilities:

```text
Decoder:    encoded media -> canonical PCM
Processing: PCM -> PCM
```

Their logical separation must not duplicate the FFmpeg dependency closure. The shared FFmpeg implementation substrate remains one closure authority when FFmpeg is reintroduced.

Capability boundary does not imply crate boundary, static-library boundary, dynamic-library boundary, or runtime plugin loading.

## Work mode

Use reality-first development:

- inspect before designing around assumed files or APIs;
- make the smallest change that establishes the requested truth;
- do not build framework machinery before real pressure exists;
- do not create registries, service locators, plugin marketplaces, macro-heavy DI, or dynamic loading merely to look extensible;
- do not move product policy into mechanism layers;
- do not perform unrelated cleanup in the same task.

If the current task conflicts with repository reality, classify the mismatch and either make the smallest authorized corrective or stop with evidence.

## Old world and reference evidence

The pre-Rust repository is preserved at:

```text
archive/pre-rust-v2
pre-rust-v2
```

The validated playback experiment is preserved at:

```text
research/playback-reference-v1
playback-reference-v1
```

These refs are historical/reference evidence, not current source-layout or architecture authority.

Do not copy old files, AGENTS rules, documentation routing, Kotlin/JVM structure, or PlayerEngine ownership back into `main` unless a current task explicitly proves and authorizes that reuse.

Reference Playback v1 may be consulted when a task needs its behavioral evidence, especially around decode/queue/submit/render distinctions, seek/flush, stale rejection, timeline, physical drain, and ENDED semantics.

## Verification

Choose verification from the actual changed surface.

At minimum, report:

- what was verified;
- what could not be verified and why;
- whether architecture/dependency direction changed;
- whether behavior changed;
- what explicitly did not change.

Never label an unrun platform or physical-device check as PASS.

## Documentation

`docs/README.md` is the documentation router. Do not recreate a large documentation bureaucracy before the repository needs it.

A durable fact should have one clear authority. Link to it instead of copying it across README, AGENTS, architecture docs, and comments.

Historical experiments belong in history/reference locations, not in current architecture prose.

## Local AGENTS policy

There are no local `AGENTS.md` files by default.

Create one only when a directory has a genuine, stable local rule that cannot be expressed cleanly by the root rules, and only when the current task explicitly justifies it.

A local AGENTS file may extend root governance; it must not redefine global architecture, duplicate root rules, or resurrect historical ownership.

## Delivery discipline

Keep commits and PRs focused. State the scope and non-scope. Do not continue automatically into the next architecture phase after the current acceptance gate passes.
