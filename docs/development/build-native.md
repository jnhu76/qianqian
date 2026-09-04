# Building the native core

> Purpose: native 侧的构建操作——从 fresh checkout 到产物落盘的命令。
> Scope: operational how-to。方法与规则见
> [../architecture/ffmpeg-minimization.md](../architecture/ffmpeg-minimization.md)；
> 回归命令权威见 [tests/songcore/README.md](../../../tests/songcore/README.md)。

## Canonical build

```bash
python3 scripts/fetch-ffmpeg       # 拉取 pinned FFmpeg source（fresh checkout 一次）
xmake ffmpeg-import                # 推导 FFmpeg source closure（host target recipe）
xmake f -m release                 # native session
xmake build songcore               # 两种产物：
                                   #   build/artifacts/libsongcore.a
                                   #   build/artifacts/shared/libsongcore.so
```

## Target-specific derivation

Target-specific derivations name the target recipe instead of hand-typing
configure flags:

```bash
python3 tools/ffmpeg_profile_import.py \
    --target linux-x86_64 \
    --profile ffmpeg/profiles/codec-base.json
# → build/manifests/linux-x86_64/codec-base/manifest.json

xmake f -p mingw -m release \
    --av_manifest=build/manifests/windows-x86_64/codec-base/manifest.json
xmake build songcore_shared   # → songcore.dll
```

The manifest records its target identity; pointing a session at a manifest
derived for a different target fails the build with the re-derive
instruction. SDK roots / cross toolchains are selected through the
environment at derive time — machine-local absolute paths are rejected from
recipes and manifests.

Test/DSP closures use `--stage`（如 `songcore-test`、`avf-c2`），命令见
[tests/songcore/README.md](../../../tests/songcore/README.md)。

## Build system scope

Xmake owns: FFmpeg import/oracle replay, SongCore C/C++, libavfilter,
libswresample, native static/shared libraries, and cross-platform native
compilation. Xmake does **not** become the Kotlin/KMP build system; the
product side builds separately (Gradle / Kotlin Multiplatform) and links
the native artifact through FFI / JNI / cinterop.

## Session hygiene

All build sessions share `build/artifacts/`. Switching platform/toolchain
sessions (native ↔ `mingw` ↔ `--wasm=wasi`) must rebuild with `-r`
（细节与历史事故见
[tests/songcore/README.md](../../../tests/songcore/README.md) 的 session
hygiene 一节）。

## Release

冻结 release 包的产物 / 审计 / 许可流程：
[release.md](release.md)。
