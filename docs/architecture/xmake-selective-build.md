# Xmake Selective Build Boundary

## Intent vs closure vs execution

Qianqian separates three concerns:

```text
capability intent            machine-derived closure            execution
(MP3/FLAC playback)          (sources + compile flags)          (Xmake)
       │                              │                           │
       └─ human maintained           └─ generated                └─ no FFmpeg Make in normal build
```

### Capability intent

Lives in the existing profile/corpus system. It answers what the player needs, not which FFmpeg `.c` files happen to implement it in one release.

### Source closure

`tools/ffmpeg_import.py` invokes the pinned upstream build only at import/upgrade time and records the real compiler invocations. The manifest is disposable and regenerable.

### Execution

`xmake.lua` replays that manifest into one `libqianqian_av.a`. Xmake is not allowed to contain FFmpeg codec dependency knowledge.

## Upgrade invariant

An FFmpeg upgrade is acceptable only if the same capability intent can regenerate a closure and pass the same corpus/PCM contract. Source-list drift is review evidence, not a merge conflict to hand-edit away.

## WASM

WASM will use the same separation but a platform-specific import/config closure. Native and WASM may select different internal translation units; they must expose the same SongCore behavior and PCM contract.
