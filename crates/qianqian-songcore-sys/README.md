# qianqian-songcore-sys

Raw Rust bindings to the Qianqian SongCore C ABI v1.

Classification (normative: `docs/architecture/songcore-binding-architecture.md`
§6): this crate is the **raw binding** layer — a mechanical ABI projection,
not an independent API authority. Only representation-level transformations
are allowed here; semantic policy belongs to the ergonomic/product adapter
above it.

This crate mirrors `native/include/songcore.h` exactly: opaque handle, status
constants, channel masks, callback types, all crossing structs in `repr(C)`,
and the 15 exported `song_*` symbols. It adds nothing else:

- no RAII, no `Drop`, no `Result` conversion, no string conversion;
- no Vec ownership, no decoder class, no source abstraction;
- no safe wrapper of any kind — `unsafe` stays visible to the caller.

That is deliberate. The next layer (the real Decode Plugin) has to earn its
own ownership shape; this crate only guarantees "Rust can speak the SongCore
ABI exactly".

## Build requirement

The crate is a consumer of the native build, not an owner of it. The SongCore
static artifact must exist before this crate compiles:

```bash
cd native
xmake ffmpeg-import
xmake f -m release -y
xmake build songcore
```

`build.rs` locates `native/build/artifacts/libsongcore.a` by walking up from
the crate directory (or via `QIANQIAN_NATIVE_DIR`) and **fails closed** when
the artifact is absent — there is no fallback to a system library or to any
other FFmpeg build. The archive is linked as-is: `-lsongcore -lm -lpthread`
(static SongCore already embeds the FFmpeg closure via the native merge
policy).

`QN_SONGCORE_ARTIFACT_SHA256` / `QN_SONGCORE_HEADER_SHA256` expose the exact
linked artifact and mirrored header identity to any consumer.

## Layout gate

The ABI layout gate (sizeof / alignof / offsetof of every crossing struct,
plus every constant value) lives in the measurement experiment:

```bash
cd experiments/songcore-call-comparison
cargo run --release -- layout
```

A tiny C probe compiled against the real header is compared line-for-line
against the Rust layout probe. Any mismatch fails the gate.

## Workspace note

This crate is deliberately **not** a member of the root Cargo workspace (see
the `exclude` list in the root `Cargo.toml`). The root workspace CI
(`cargo check --workspace`) runs on docs PRs where the native artifact does
not exist; a workspace member with a hard native build requirement would
break that gate. The crate builds standalone from its own manifest.
