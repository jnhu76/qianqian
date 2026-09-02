# SongCore API

How to call the SongCore native library. This is the permanent CALLER
document: it explains how to open a file, read PCM, seek, and close — for
C/C++ programs and for FFI wrapper authors (Kotlin/JVM, Kotlin/Native,
Swift/ObjC, Qt). It does not require FFmpeg knowledge and you do not need to
read any implementation file to use the library correctly.

For architecture (why SongCore exists, what AudioEngine does) read
[audio-core.md](audio-core.md). For the FFmpeg trimming method read
[ffmpeg-minimization.md](ffmpeg-minimization.md). The ABI authority is
[`include/songcore.h`](../include/songcore.h) — ABI v1, frozen.

- ABI version: `SONGCORE_ABI_VERSION` = 1. Layout is fixed; compatible
  additions use reserved fields or new functions, and any break is ABI v2.
- Public symbols: exactly **15**, listed in [§4](#4-api-reference). The
  shared library exports exactly these and nothing else (machine-audited).
- No FFmpeg type ever crosses this header.

## 1. Artifacts and linking

One conceptual library (`songcore`), two native artifact kinds plus a WASM
guest — all from the same ABI and capability model:

| Target | Static | Shared | Notes |
|---|---|---|---|
| Linux x86_64 | `libsongcore.a` | `libsongcore.so` | **Proven** — reference artifacts, measured + audited on this branch |
| Windows x86_64 | (`libsongcore.a`) | `songcore.dll` | **Proven (shared)** — target `windows-mingw-x86_64`, mingw-w64 cross (`target_os=mingw32`, `cross_prefix=x86_64-w64-mingw32-`); PE exports audited (15 exactly), real Windows Python ctypes consumer PASS with full PCM authority (§8). MSVC is a separate target recipe and is still Planned — a MinGW manifest never satisfies an MSVC session (fail-closed identity gate) |
| Android arm64 | — | `libsongcore.so` | Planned |
| macOS arm64 | `libsongcore.a` | `libsongcore.dylib` | Planned |
| iOS arm64 | `libsongcore.a` | — | Planned (XCFramework packaging is a later product-side step) |
| WASM (wasi / emscripten) | — | — | Proven as guest module `SongCore.wasm` — uses the WASM bridge (`src/wasm/`), not native FFI; see [wasm.md](wasm.md) |

"Proven" means: oracle-derived manifest, Xmake replay, corpus/PCM/ABI gates
all green on this repository's evidence. "Planned" means the recipe exists,
nothing is claimed until a toolchain derives it (see
`ffmpeg/targets/*.json` `status` fields).

Xmake targets: `songcore_static`, `songcore_shared`, aggregate `songcore`.
For the frozen release package (staging, provenance, license, freeze
policy) read [songcore-release.md](songcore-release.md).

```bash
xmake ffmpeg-import        # once per checkout: derive the FFmpeg closure
xmake build songcore       # → build/artifacts/libsongcore.a
                           #   build/artifacts/shared/libsongcore.so
```

Symbol visibility contract (`SONGCORE_API` in `songcore.h`):

- Link the static archive: nothing to define — plain declarations.
- Link `libsongcore.so` on ELF/macOS: nothing to define.
- Consume `songcore.dll` on Windows: define `SONGCORE_DLL` to get
  `dllimport`.

The shared library exports exactly the 15 ABI symbols. FFmpeg is statically
linked inside and never leaks a symbol (audited by
`tests/songcore/regression.py` and `tools/measure_songcore_artifacts.py`).
Shared dynamic dependencies on Linux: `libm`, `libc` only.

## 2. Lifecycle at a glance

```mermaid
flowchart TD
    O[song_open] --> P[song_probe]
    P --> M[metadata / artwork / streams]
    M --> R[song_read_pcm]
    R -->|SONG_OK| R
    R -->|SONG_EOF| E[finished]
    R -->|seek| S[song_seek]
    S --> R
    E --> C[song_close]
```

One `song_handle` is one song file. `song_probe` selects the default
decodable audio stream and opens its decoder; every view afterwards
(`song_info`, metadata, artwork, PCM) refers to that selection.

### Minimal C example

```c
#include <songcore.h>
#include <stdio.h>
#include <stdlib.h>

static int64_t my_read(void *ud, uint8_t *dst, size_t size)  { /* host file read */ }
static int64_t my_seek(void *ud, int64_t off)                { /* host file seek */ }
static int64_t my_size(void *ud)                             { /* host file size */ }

void play_one_file(void) {
    song_io io = { .userdata = NULL, .read = my_read,
                   .seek = my_seek, .size = my_size };
    song_handle *song = NULL;

    if (song_open(&io, &song) != SONG_OK) return;

    song_info info;
    if (song_probe(song, &info) != SONG_OK) { song_close(song); return; }

    uint64_t cap = 4096; /* frames per pull */
    float *pcm = malloc(cap * info.channels * sizeof(float));
    for (;;) {
        uint64_t frames = 0;
        song_status st = song_read_pcm(song, pcm, cap, &frames);
        if (st == SONG_EOF) break;
        if (st != SONG_OK)  break; /* typed error; see song_last_error */
        /* pcm is Float32 interleaved, source rate/layout */
        send_downstream(pcm, frames, info.sample_rate, info.channels);
    }

    free(pcm);
    song_close(song);
}
```

## 3. The PCM contract (read this twice)

`song_read_pcm` output is **frozen**:

```text
sample format : Float32 (32-bit IEEE), native endianness
interleaving  : interleaved (not planar)
sample rate   : the SOURCE rate (SongCore never resamples)
channel layout: the SOURCE layout (mask from song_info.channel_mask)
```

Memory layout for stereo: `L0 R0 L1 R1 L2 R2 ...`. Buffer size in floats =
frames × channels. One "frame" = one sample per channel.

```text
SongCore performs NO sample-rate conversion.
SongCore performs NO DSP (gain/EQ/ReplayGain/crossfade/...).
```

Downstream (the future AudioEngine) decides: source matches output →
BYPASS; otherwise SRC = aresample/libswresample; DSP = capability-trimmed
libavfilter. SongCore ends at source PCM.

Partial success: if a decode error happens mid-call after some frames were
produced, the call returns `SONG_OK` with those frames, and the typed error
surfaces on the NEXT call (with zero frames). Format changes mid-stream
(rate/channel count/layout contradicting `song_info`) fail closed with
`SONG_ERR_STREAM_CHANGE` instead of emitting contradicting PCM.

## 4. API reference

Grouped by purpose; all 15 exported symbols. All functions take the handle
as first argument (except `song_open` / `songcore_abi_version`); all return
`song_status` (except `songcore_abi_version` / `song_close`); all write
results through out-parameters, which are untouched on failure unless stated.

### ABI

**`uint32_t songcore_abi_version(void)`** — returns `SONGCORE_ABI_VERSION`
(1). Callable at any time, including before any handle exists. FFI wrappers
should gate loading on it.

### Lifecycle

**`song_status song_open(const song_io *io, song_handle **out_handle)`** —
open a handle over caller-provided I/O. `io` holds three required callbacks
plus a `userdata` pointer passed back on every call:

- `read(dst, size)` → bytes read (>0), 0 at EOF, <0 on host error;
- `seek(absolute_offset)` → resulting absolute offset, <0 on host error;
- `size()` → total source size in bytes, <0 when unavailable.

SongCore has no filesystem, network, or URI semantics of its own — the
caller owns the bytes. On failure `*out_handle` is untouched and **no
diagnostic exists** (no handle to ask). Ownership: a successful
`song_open` transfers the handle to SongCore until `song_close`.

**`void song_close(song_handle *handle)`** — frees every SongCore-owned
resource and invalidates all borrowed views (metadata, artwork, error).
Safe in any state; `NULL` is a no-op. After `song_close` the handle must
not be used again.

**`song_status song_probe(song_handle *handle, song_info *out_info)`** —
parse the container, enumerate decodable audio streams, select the default
one (decodable audio only; prefer the container's default disposition,
else lowest index), open its decoder, build the song snapshot. Idempotent:
later calls return the cached snapshot. `song_info` carries source
`sample_rate`, `channels`, `channel_mask`, `duration_us` (−1 unknown),
`bits_per_sample` (0 when meaningless), stable ASCII `codec`/`container`
names, `selected_audio_index`, `audio_stream_count`.

### Stream selection

Decodable audio streams are enumerated 0..count−1 (`audio_index`).
`stream_index` is the absolute container stream index — identity
correlation only, never used in calls.

**`song_status song_audio_stream_count(song_handle *handle, uint32_t *out_count)`**
— number of decodable audio streams (excludes non-audio and
attached-picture streams). Requires a probed handle.

**`song_status song_audio_stream_info(song_handle *handle, uint32_t audio_index, song_stream_info *out_info)`**
— per-stream info: rates, layout mask, duration, codec, `is_default`.

**`song_status song_select_stream(song_handle *handle, uint32_t audio_index)`**
— explicit switch. Semantics (part of ABI v1): the old decoder is
destroyed and a new one opened; playback position resets to the start; the
metadata snapshot is REBUILT (all metadata views from before are
invalidated); container artwork is NOT affected and stays valid;
`song_info` fields (rate/layout/selected_audio_index) update; pending PCM
from the old stream is gone. Invalid `audio_index` →
`SONG_ERR_INVALID_ARGUMENT`.

### Metadata

After a successful probe/stream selection, SongCore holds an immutable
metadata snapshot. **Borrowed views**: returned pointers point into
SongCore-owned memory; the caller does NOT free them. Views stay valid
across `song_read_pcm`, `song_seek`, and EOF, and are invalidated by the
next `song_select_stream` or by `song_close`. Canonical precedence: the
selected stream's tags override container tags; absence is not an error —
presence is explicit via `has_*`.

**`song_status song_get_metadata(song_handle *handle, const song_metadata **out_meta)`**
— pointer to the immutable snapshot (title/artist/album/album_artist/
genre/composer/date/comment, track & disc numbers, ReplayGain in
microbels with peaks at 100000 = full scale). Copy it out if it must
outlive the borrowed-view lifetime.

**`song_status song_get_metadata_count(song_handle *handle, uint32_t *out_count)`**
— size of the raw metadata enumeration (container scope first, then the
selected stream scope; source parse order within a scope; duplicate keys
preserved). Unknown/future tags are readable here without any ABI change.

**`song_status song_get_metadata_entry(song_handle *handle, uint32_t index, song_metadata_entry *out_entry)`**
— one raw `(scope, key, value)` view; out-of-range index →
`SONG_ERR_INVALID_ARGUMENT`.

### Artwork

Compressed image bytes only (JPEG/PNG/...); SongCore never decodes images.
Deterministic container order. Views are SongCore-owned, valid until
`song_close`; stream selection does NOT invalidate them. Role is
best-effort: a single artwork is treated as front cover, otherwise mapped
from the source picture-type label when available. `width`/`height` are
−1 when unknown.

**`song_status song_get_artwork_count(song_handle *handle, uint32_t *out_count)`**
— 0..N artwork items.

**`song_status song_get_artwork_item(song_handle *handle, uint32_t index, song_artwork_item *out_item)`**
— one item: `role`, `mime` (+len), `data` (+data_len). The caller does not
free `data`.

### Decode

**`song_status song_read_pcm(song_handle *handle, float *dst, uint64_t frame_capacity, uint64_t *out_frames_produced)`**
— decode up to `frame_capacity` frames into caller-owned `dst` (see
[§3](#3-the-pcm-contract-read-this-twice)). `SONG_OK` → frames > 0;
`SONG_EOF` → frames == 0, normal end; anything else → typed error with
frames == 0. `frame_capacity == 0` → `SONG_ERR_INVALID_ARGUMENT`.

### Navigation

**`song_status song_seek(song_handle *handle, int64_t requested_position_us, int64_t *out_actual_position_us)`**
— playback-oriented seek (NOT sample-perfect unless the format proves
it). The target is clamped against the known duration, converted to a
container seek at/before the target; the decoder is flushed and all
pending PCM state is cleared; the next `song_read_pcm` belongs to the
landing point. `*out_actual_position_us` reports the landing measured
from the first decoded frame's timestamp; −1 means the position is
genuinely unknown (never manufactured); the out-pointer may be NULL.
Lossless formats land frame-accurate; lossy/lapped codecs may differ from
sequential decode within bounded codec-frame tolerance. After a
successful seek: metadata, artwork, stream selection, rate/layout/codec
are all unchanged. Errors: `SONG_ERR_SEEK_UNSUPPORTED` (container cannot
seek), `SONG_ERR_SEEK_ERROR`, `SONG_ERR_STREAM_CHANGE` /
`SONG_ERR_DECODE_ERROR` if the landing frame cannot be converted
(fail-closed).

### Diagnostics

**`song_status song_last_error(song_handle *handle, const song_error **out_error)`**
— diagnostic for the last failed operation on this handle: NUL-terminated
UTF-8 `message` (+`message_len`), backend `native_code`. For logs only —
callers branch on `song_status`, never on message text. Contract:

- every typed failure (`status >= 100`) on a valid handle leaves a
  diagnostic readable here;
- `SONG_OK` and `SONG_EOF` clear it (the message reads back NULL);
- reading via `song_last_error` never clears the diagnostic — only the
  next operation on the handle does;
- not available for `song_open` failures (no handle survives).

Machine-checked per function by `tests/songcore/regression.py`
(`last-error.json` evidence).

## 5. Error model

`song_status` is typed; EOF is a normal terminal condition, not a failure.

| Status | Meaning |
|---|---|
| `SONG_OK` (0) | success |
| `SONG_EOF` (1) | end of decoded PCM |
| `SONG_ERR_INVALID_ARGUMENT` | null/illegal argument |
| `SONG_ERR_STATE` | call not allowed in current state |
| `SONG_ERR_NOT_OPEN` | operation needs a probed handle |
| `SONG_ERR_IO` | host I/O failure (your callbacks) |
| `SONG_ERR_UNSUPPORTED_CONTAINER` | container not decodable |
| `SONG_ERR_NO_AUDIO_STREAM` | no decodable audio stream |
| `SONG_ERR_UNSUPPORTED_CODEC` | stream codec not in the capability set |
| `SONG_ERR_CORRUPT_DATA` | malformed bitstream |
| `SONG_ERR_DECODE_ERROR` | decode failure mid-stream |
| `SONG_ERR_SEEK_UNSUPPORTED` | container has no seek |
| `SONG_ERR_SEEK_ERROR` | seek failed |
| `SONG_ERR_STREAM_CHANGE` | decoder changed rate/layout mid-stream (fail-closed) |
| `SONG_ERR_OUT_OF_MEMORY` | allocation failure |
| `SONG_ERR_INTERNAL_ERROR` | bug guard; please report |

## 6. Threading

A `song_handle` is NOT internally thread-safe: serialize all calls on one
handle externally. Different handles may be used concurrently from
different threads. SongCore adds no internal mutexes — an FFI wrapper that
exposes one song to multiple threads owns the lock. `songcore_abi_version`
is the only safely callable-anywhere function.

## 7. FFI guidance (Kotlin/KMP, Swift, Qt, ...)

```text
Kotlin / KMP  (or Swift / Qt / ...)
      ↓  JNI / JNA / Panama / cinterop /binding
songcore.h  (ABI v1)
      ↓
libsongcore.so · songcore.dll · libsongcore.a · static native integration
```

- The FFI wrapper owns the language-side object lifetime; the native
  `song_handle` stays opaque. Never mirror FFmpeg structures — there are
  none in the ABI.
- Gate library load on `songcore_abi_version()`.
- Copy metadata/artwork out into managed objects if they must outlive the
  native borrowed-view lifetime (§4); otherwise borrow within the stated
  lifetime. Borrowed string fields are (pointer, length) pairs — never
  read them as NUL-terminated (the ctypes consumer proves this with
  `c_void_p` + explicit length).
- PCM buffers should avoid unnecessary copies where the platform FFI
  permits (e.g. direct Float32 buffers over JNI, `HEAPF32` over WASM).
- One wrapper object ⇔ one native handle; a `close()` in the wrapper must
  reach `song_close` exactly once.

No framework-specific bindings are part of this repository (Phase 0).

## 8. Verified consumers

Every claim above is machine-checked by external consumers that use ONLY
`songcore.h` and the shipped artifacts — no Qianqian test binary participates.
Authority: `bench/results/songcore-v1/ffi-consumers.json`, re-executed live by
`tests/songcore/consumers.py` (`--out` run, `--check` fail-closed revalidation;
the host-independent gates also run inside `xmake test`).

| Consumer | Path | Evidence |
|---|---|---|
| Python ctypes decode | `python3 tools/songcore_ffi_smoke.py <songs...>` — stdlib-only (ctypes/argparse/hashlib/struct); mirrors all 15 symbols and every ABI struct; borrowed strings consumed as pointer+length (never NUL-terminated); checks open/probe/metadata/raw-metadata/artwork/decode/seek/close plus the typed refusal contracts (zero capacity, invalid stream index); every song records `decoded_frames`/full-window `pcm_sha256`/`peak`; `--pcm-dump-dir` writes bounded PCM samples for the cross-backend gate; `--play` adds audible output at source rate via sounddevice (no Python-side resampling, no `qn_pcm_dump`) | Linux: 13-fixture matrix PASS incl. ADTS typed `SONG_ERR_SEEK_UNSUPPORTED`; three real local songs PASS with UTF-8 metadata + JPEG artwork; audible run PASS |
| Static archive consumer | `tests/consumer/songcore_static_smoke.c`, compiled with the documented one-archive link line `cc -Iinclude … -Lbuild/artifacts -lsongcore -lm -lpthread` | FLAC + M4A decode/seek PASS through the merged self-contained `libsongcore.a` |
| Shared export audit | exactly the 15 `SONGCORE_API` symbols, nothing else | PASS on Linux ELF and Windows PE |
| Windows ctypes (recorded) | real Windows Python 3.13 process → ctypes → `songcore.dll` (PE exports audited: 15 exactly; imports only `bcrypt.dll`/`KERNEL32.dll`/`msvcrt.dll`; zero FFmpeg DLL dependencies); per-song PCM authority (`decoded_frames`/`pcm_sha256`/`peak`) + explicit per-gate checks | 13-fixture matrix PASS (evidence: `bench/results/songcore-v1/win-ffi.json`, pinned to the `windows-mingw-x86_64` recipe/header/artifact hashes) |
| WASM | independent `wasmtime` host instantiates `build/artifacts/wasm/SongCore.wasm` and drives the 15 contract mirrors over `qianqian_host` read/seek/size imports: `python3 tools/songcore_wasm_smoke.py <songs...>`. All guest-side buffers are allocated through the bridge exports `song_wasm_alloc`/`song_wasm_free` (host never guesses addresses; leak-checked per run); struct layouts are machine-read from `song_wasm_layout` (host never hardcodes offsets); export surface audited (15 contract + 3 bridge exports, nothing else) | 6 fixtures PASS with semantic metadata assertions (exact UTF-8 titles), raw-metadata, artwork (SHA-256 of compressed bytes), decode/seek, per-check explicit PASS map |
| Cross-backend consistency | `python3 tests/songcore/ffi_consistency.py --check` — the same fixture through Linux shared / Windows DLL / WASM guest must mean the same song: lossless PCM (FLAC/ALAC/PCM WAV) SHA-256 exactly equal; lossy within the established 1e-6 authority; metadata semantic-match; artwork exact | PASS (`bench/results/songcore-v1/ffi-consistency.json`): all lossless pairs byte-exact, lossy max\|Δ\| ≤ 1.2e-07 |

Known typed behavior verified against real content: raw ADTS AAC has no
container seek — `song_seek` returns `SONG_ERR_SEEK_UNSUPPORTED` (the ABI's
typed "container has no seek"), while indexed/seekable formats
(FLAC/MP3/M4A/ALAC/OGG/WAV) land within the documented tolerance.

The decode closure inside every artifact is the `codec-base` profile — the
PRD Common Formats set (MP3, FLAC, AAC/M4A, raw ADTS, ALAC/M4A, PCM WAV,
Ogg Vorbis, Ogg Opus) — see `ffmpeg/profiles/codec-base.json`.
