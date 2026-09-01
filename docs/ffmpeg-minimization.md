# FFmpeg Minimization

How Qianqian ships a small FFmpeg-based decoder without maintaining a
hand-edited FFmpeg fork. This is the reusable idea of the project.

## The pipeline

```text
product capability intent
        ↓  (human, machine-readable: ffmpeg/capabilities/*.json)
pinned upstream FFmpeg configure  (ffmpeg/pin.json → n9.0.1)
        ↓  (import/oracle only: tools/ffmpeg_import.py)
dependency / source closure       (which .c, with which flags)
        ↓  (machine-derived manifest, never hand-maintained)
Xmake replay                      (xmake.lua replays the manifest)
        ↓
target-specific native artifact   (libsongcore.a / .so / .dll / .a / .wasm)
```

### Rules that make this safe

- **Upstream FFmpeg tree stays pristine.** Imported once into
  `build/ffmpeg-src/`; Qianqian never patches it.
- **`configure`/`Make` is an oracle, not the build.** It is invoked only at
  import/upgrade time to learn the real dependency graph and per-TU compile
  semantics for the pinned tag.
- **Normal builds are Xmake.** `xmake` never runs FFmpeg Makefiles; it
  replays the frozen compile manifest.
- **Capability intent is human-maintained; the source closure is
  machine-derived.** Nobody hand-maintains a "list of deleted FFmpeg
  files". The manifest records exactly what the pinned configure resolved
  for the declared intent.
- **Target manifests are target-specific.** The Linux closure is not reused
  blindly for Windows/macOS/Android/WASM; each target derives its own
  closure from the same intent + its own toolchain.
- **The shipping size authority is the final linked artifact**, never a
  source-count or directory-size proxy.

### Upgrade flow

An FFmpeg upgrade re-runs import against the new pin, re-derives the
closure from the same capability intent, and the diff is review evidence
(source/flag/size/symbol/corpus/PCM drift). Never copy the old source list
forward.

## The build

```bash
xmake ffmpeg-import          # resolve closure once per fresh checkout
xmake f -o build/xmake       # normal native session
xmake build songcore         # → build/artifacts/libsongcore.a
```

Xmake owns: FFmpeg import/oracle replay, SongCore C/C++, libavfilter,
libswresample, native static/shared libraries, and cross-platform native
compilation. Xmake does **not** become the Kotlin/KMP build system; the
product side builds separately (Gradle / Kotlin Multiplatform) and links
the native artifact through FFI / JNI / cinterop.

## Machine inputs (single source of truth)

| File | Meaning |
|---|---|
| `ffmpeg/pin.json` | pinned upstream tag + commit + source sha256 |
| `ffmpeg/capabilities/songcore.json` | codecs/containers SongCore must decode |
| `ffmpeg/capabilities/dsp.json` | libavfilter capabilities AudioEngine may use |
| `build/.../manifest.json` | machine-derived closure for one target (regenerable) |

Human-readable experiment narrative is archived in git history and
`docs/history.md`; the machine files above are the durable authority.
