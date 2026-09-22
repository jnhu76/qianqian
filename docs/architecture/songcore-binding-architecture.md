# SongCore binding architecture — one core / many bindings

> **Truth class: NORMATIVE AUTHORITY** for the cross-language SongCore
> binding architecture (Issue #173 `SONGCORE-BINDING-ARCH-1`).
>
> The SongCore ABI description itself is normative in
> [`native/include/songcore.h`](../../native/include/songcore.h). This
> document governs how target artifacts and language/platform bindings may
> relate to that header. Where it summarizes header semantics it is a
> **derived projection** that routes back to the header; it never redefines
> them. Playback-side interpretation of SongCore outcomes (e.g. the seek
> classes) stays owned by `docs/adr/ADR-PBK-002.md`, above any binding.

---

## System context: SongCore component vs Qianqian product

SongCore and Qianqian are two distinct top-level product/component
boundaries. They should not be collapsed into one software layer stack or
read as "Qianqian plus an internal decoder implementation."

```text
┌───────────────────────────────────────────────┐
│                  Qianqian                     │
│                                               │
│ App / UI / K0 / Playback Session / Playlist  │
│ Output backends / devices / product policy    │
└──────────────────────┬────────────────────────┘
                       │
                 Decode contract
                       │
┌──────────────────────▼────────────────────────┐
│                  SongCore                     │
│                                               │
│ probe / streams / metadata / artwork / seek  │
│ decode                                        │
│                     ↓                         │
│ source-rate / source-layout Float32 PCM       │
└───────────────────────────────────────────────┘
```

The ownership waterline is:

> **SongCore owns media-to-PCM semantics. Qianqian owns
> PCM-to-player-product semantics.**

SongCore is independently versioned and released and may be consumed by
hosts other than Qianqian. Qianqian is one product consumer. Within
Qianqian, the Decode Plugin / decode adapter projects SongCore into the
Qianqian capability/composition model; that does not make SongCore itself
a K0 Plugin or move Qianqian product semantics into SongCore.

This document governs the lower component's cross-platform API/binding
architecture. Qianqian playback, navigation, output/device, UI and other
product semantics remain governed by Qianqian's product/ADR authorities.
The repository-level projection of this boundary is
[`overview.md`](overview.md).

---

## 1. Scope

This document freezes the libVLC-style architecture before Android/Apple
binding work starts:

```text
ONE semantic API
ONE canonical C ABI
MANY target artifacts
MANY thin bindings
ZERO platform-specific semantic forks
```

In scope:

```text
authority hierarchy (who may define what)
raw-binding boundary vs ergonomic/product-adapter boundary
allowed FFI transformations vs forbidden semantic transformations
artifact vs binding distinction
target support maturity vocabulary (BUILD/LINK/CORE_RUNTIME/BINDING_RUNTIME/RELEASE)
binding parity requirements + the canonical parity oracle
classification of the existing Rust path onto this model
```

Out of scope (explicit non-goals for #173 and everything frozen here):

```text
change ABI v1 for binding convenience
introduce UniFFI or any generator as a second ABI authority
introduce another decoder engine / add codecs / add DSP / add SRC
add output backends
add Kotlin/Swift product semantics into SongCore
rewrite the working Rust binding
claim runtime support from cross compilation
implement full Android/iOS targets
```

**Division of ownership with #172 (`SONGCORE-MULTI-TARGET-RELEASE-1`):**

```text
#172 owns: target build, target FFmpeg closure, artifact, provenance,
          packaging, release matrix
#173 owns: cross-language binding architecture, authority hierarchy,
          binding parity, binding maturity vocabulary (this document)
```

#172 reuses the maturity vocabulary defined in §5; the vocabulary's
definitions and the `BINDING_RUNTIME` evidence rule live here.

---

## 2. Decode contract and target model

The one semantic decode contract (unchanged, defined by the header):

```text
host-provided seekable media bytes (song_io callbacks)
        ↓
SongCore semantic API (open/probe/streams/metadata/artwork/read/seek/close)
        ↓
SongCore C ABI v1          ← canonical binary/API authority
        ↓
SongCore engine + target-specific FFmpeg closure
        ↓
Float32 interleaved PCM at source rate / source layout
```

Platform/language integrations sit **above** the C ABI. Target model:

```text
                       Product
                          │
       ┌──────────────────┼──────────────────┐
       │                  │                  │
      Rust             Android             Apple
       │                Kotlin         Swift / Kotlin-Native
 raw/safe layers           │                  │
       │                  JNI         C module / cinterop
       └──────────────────┼──────────────────┘
                          │
                    SongCore C ABI v1
                          │
              target-specific native artifact
```

**Support status:** today only the Rust path is implemented and evidenced
(qianqian-songcore-sys + qianqian-decode-songcore, Linux/Windows desktop).
The Android and Apple columns are the **target model** for follow-up issues;
neither is currently supported at any maturity state (§5). The diagram must
not be read as a support claim.

---

## 3. Authority hierarchy

```text
1. SongCore semantic contract
      (decode/seek/EOF/metadata/ownership meaning — as documented by
       native/include/songcore.h; playback-side interpretation of
       outcomes stays with ADR-PBK-002)
2. native/include/songcore.h
      (canonical C ABI v1 description: symbols, layouts, constants,
       units, sentinels, lifetimes, threading)
3. target-specific SongCore binary
      (per-target artifact implementing layer 2; built/provenanced/
       packaged per #172)
4. raw language/platform binding
      (mechanical projection of layer 2 into a language)
5. ergonomic/product adapter
      (language ergonomics + product capability adaptation above layer 4)
```

Rules:

```text
A higher layer MUST NOT redefine lower-layer semantics.

No generated or handwritten Kotlin/Swift/Rust/ JNI / cinterop /
modulemap declaration becomes a second ABI authority — layer 2 is the
only ABI description.

Bindings (layers 4 and 5) do NOT own:
    PCM format semantics
    resampling
    channel remapping
    seek correction
    EOF interpretation
    retry policy
    codec fallback
    stream-selection policy
    output-device behavior
    async scheduling

Stream-selection policy is fixed IN the ABI (song_probe: decodable audio
streams only; prefer AV_DISPOSITION_DEFAULT; otherwise lowest stream
index). A binding may expose song_select_stream; it may not choose a
different default or silently re-select.

Product/playback semantics above the decode contract (seek acceptance
classes, position/duration evidence classes, playlist policy) are owned
by ADR-PBK-002 and the product layers. A product adapter *realizes* that
authority; it does not invent it, and it never changes what the ABI
observed.
```

---

## 4. Binding classes and permitted transformations

### 4.1 Raw binding (layer 4)

A raw binding is a **mechanical ABI projection**. It may translate only
representation details needed to cross FFI:

| Allowed raw-binding transformation | |
|---|---|
| C pointer ↔ language pointer/reference | representation only |
| fixed-width integer ↔ same-width language integer | e.g. `uint64_t` ↔ `u64`; a non-negative C `enum` value ↔ same-width unsigned language integer |
| C enum/status ↔ exact numeric representation | value parity, no renumbering |
| callback ↔ FFI trampoline | identical signature, calling convention and return encoding |
| `repr(C)` structure ↔ layout-compatible structure | identical field order, widths, alignment |
| extern declaration of an exported symbol | exact name and signature |

A raw binding MUST preserve, without alteration:

```text
units
numeric ranges
sentinels
ownership
borrowed-view lifetimes
host-I/O callback + userdata lifetime
threading / external-serialization requirement
status / EOF distinction
seek request and actual-position meaning
PCM frame/channel layout
```

A raw binding MUST NOT add policy of any kind — no RAII invention that
changes observable lifetime rules, no error mapping, no retries, no
defaults, no caching, no async. `unsafe`/`!` boundaries stay visible.

### 4.2 Ergonomic / product adapter (layer 5)

A higher-level adapter MAY add language ergonomics:

| Allowed adapter transformation | |
|---|---|
| RAII / Drop / Closeable | runs `song_close` exactly once at the adapter's declared ownership end |
| Result / exception façade | carries the same failure/EOF distinction into the language's error idiom |
| owned copy of borrowed UTF-8 data | copy taken before the ABI's invalidation point |
| safe PCM buffer wrapper | capacity expressed in frames, format untouched |
| host `File` (or equivalent) → `song_io` callback adapter | read/seek/size semantics mapped 1:1 (bytes read / 0 at EOF / <0 error) |
| ABI-version check at construction | fail-closed on `songcore_abi_version` mismatch |
| documented unit translation to a product-owned unit | value-preserving modulo stated quantization; sentinel preserved (e.g. unknown stays unknown) |

**Condition for every adapter transformation: the observable SongCore
semantics remain equivalent.** Adapter-level product semantics (seek
acceptance classes, duration evidence class) must cite the product ADR
that owns them — they are layer-5 realizations of a higher authority, not
binding-owned behavior.

### 4.3 Forbidden hidden behavior (all layers above the core)

```text
resampling
channel conversion / remapping
silent seek correction (including replacing an unknown landing with the
    requested target)
retry
format fallback
codec fallback
different EOF rules (EOF is a normal terminal status, never an error)
different error taxonomy at the binding layer (raw statuses survive
    unchanged; a product adapter maps to product-owned types and must
    keep the failure/EOF distinction observable)
async execution policy
buffering policy that changes observable decode behavior
stream-selection policy
output-device behavior
```

These belong to product/playback/output layers under their own ADRs, never
to a binding.

---

## 5. Artifact vs binding; support maturity states

An **artifact** is a built SongCore binary for one target (`.a` primary,
`.so`/`.dll` secondary per #172's static-first policy). A **binding** is a
language/platform projection of the C ABI above an artifact. Package
containers (AAR, XCFramework, crate, tarball) are packaging for artifacts
and/or bindings; **no package format is API authority**.

Maturity is reported per `(target, binding)` pair with exactly these
states:

```text
BUILD
  The target-specific SongCore artifact was produced for that target by
  its own target recipe + target-specific FFmpeg closure (provenance
  recorded). A cross-compile PASS is at most BUILD.

LINK
  A minimal consumer compiled against native/include/songcore.h links
  the target artifact and resolves all 15 ABI v1 symbols.
  An archive or link produced here is NOT "supported".

CORE_RUNTIME
  A direct native (C) consumer executes the binding-parity scenario
  (§7) on the target's real runtime/hardware and passes the semantic
  gates (status classes, units, sentinels, PCM/reference, seek, EOF,
  malformed handling). Cross-compilation can never produce this state.

BINDING_RUNTIME
  Each language binding shipping for that target executes the same §7
  scenario through the binding and matches the direct-C consumer on the
  frozen comparison set. A loaded JNI library is NOT decode correctness;
  a produced XCFramework is NOT iOS runtime correctness.

RELEASE
  Artifact + provenance/license/checksum manifest + (when applicable)
  the binding/package are reproducibly shipped per #172's artifact
  contract. A produced archive is not a release.
```

Gatekeeping rules (normative):

```text
cross-compile PASS              != runtime support   (≤ BUILD)
archive produced                != supported          (≤ LINK)
JNI loads                       != decode correctness (≤ LINK for the
                                                   artifact; ≠ BINDING_RUNTIME)
XCFramework produced            != iOS runtime correct (≤ BUILD/LINK)
CORE_RUNTIME                    is required before BINDING_RUNTIME
BINDING_RUNTIME evidence        is defined by the §7 oracle (this doc)
BUILD…CORE_RUNTIME + RELEASE    evidence is owned by #172's release matrix
```

---

## 6. Current Rust integration mapped onto this model

### 6.1 `crates/qianqian-songcore-sys` = raw binding (layer 4)

Verified from source:

```text
repr(C)             song_error, song_io, song_info, song_stream_info,
                    song_metadata, song_metadata_entry, song_artwork_item;
                    opaque song_handle — field-for-field mirror of the header
symbol parity       exactly the 15 header exports (14 song_* +
                    songcore_abi_version), declared in one extern "C" block
constant parity     SONGCORE_ABI_VERSION, 16 status values, 20 channel
                    constants, 2 metadata scopes, 4 artwork roles —
                    identical values
callback signatures unsafe extern "C" fn with identical parameter/return
                    widths (i64, usize, pointers); nullable fn fields in
                    song_io match C function-pointer width
no semantic policy  no RAII, no Result, no ownership invention, no error
                    mapping, no defaults — unsafe stays visible
```

Mechanical evidence: the layout gate (C probe vs Rust probe,
line-for-line sizeof/alignof/offsetof + constant values) and the surface
gate (all 15 symbols executed from C and Rust) in
`experiments/songcore-call-comparison`; the ABI export gate in
`native/experiments/songcore-equivalence/run.py`.

Representation note (recorded, not drift): functions returning the C
`enum song_status` are declared `-> u32` in Rust. C enums have `int`
width; the status values (0, 1, 100–113) are non-negative and
fixed-width, so this is a same-width numeric projection covered by the
constant-parity gate.

### 6.2 `crates/qianqian-decode-songcore` = ergonomic/product adapter (layer 5)

Transformations performed, each classified:

| Transformation | Class |
|---|---|
| ABI-version check at construction, fail-closed | allowed adapter (§4.2) |
| `std::fs::File` → `song_io` read/seek/size trampolines, fail-closed guards | allowed adapter |
| RAII: `Drop` → `song_close`; `Box<File>` kept alive for the handle lifetime | allowed adapter |
| null-handle fail-closed guard on `SONG_OK` | defensive, representation-only |
| status → `DecodeOpenError`/`DecodeError` (product types from `qianqian-audio-api`); message copied to owned `String` before the next native call | allowed adapter; product taxonomy is the product contract's, not the binding's |
| `SONG_EOF` → `DecodeOutcome::Eof`, distinct from error | required preservation |
| seek status → three-class `SeekClass` (`Applied`/`RefusedUnchanged`/`MutatedThenFailed`) | layer-5 realization of **ADR-PBK-002 D14.5** — not binding-owned policy |
| `duration_us` negative → `None`, `0` → `Some(0)`; never corrected against decoded totals | layer-5 realization of **ADR-PBK-002 D14.8** sentinel treatment |
| seek landing µs → frames (documented ±1-frame quantization); `-1` landing → `None`, never manufactured or replaced by the target | documented unit translation; landing value itself never altered |
| `PcmDecode` capability + `ComponentSpec` publication (`songcore_decode_plugin`) | product capability adaptation |
| `unsafe impl Send` mirroring the ABI's external-serialization contract (`DecodedPcmStream: Send`, not `Sync`) | required preservation |

No resampling, no channel remapping, no retry, no codec/format fallback,
no stream re-selection, no async scheduling: the PCM format comes straight
from `song_info` and frames pass through untouched.

**Classification verdict:** the model in §3 of #173 is accurate —
`qianqian-songcore-sys` is the raw binding, `qianqian-decode-songcore` is
the ergonomic/product adapter. No semantic drift was found at the ABI
boundary (see the final report of #173 for the recorded observations:
product error-class granularity at layer 5, the `enum`→`u32` projection,
and the implied host-IO callback lifetime).

---

## 7. Binding parity oracle

One canonical observable scenario. It MUST be runnable by:

```text
1. a direct C consumer (reference)
2. the existing Rust raw binding + adapter
3. a future Android JNI binding
4. a future Apple C-module/cinterop binding
```

The oracle compares **observable semantics, not implementation identity**.

### 7.1 Scenario (per fixture, in order)

```text
01  songcore_abi_version == 1
02  open via host I/O callbacks            → status class
03  probe                                  → source facts snapshot
04  audio-stream enumeration + default stream identity
05  metadata: canonical fields (presence, values, units) +
    raw entry enumeration (scope order, duplicates preserved)
06  artwork enumeration where the fixture provides it
    (count, role, mime, byte identity, width/height sentinel)
07  PCM decode: full drain                 → frame count + PCM reference
08  seek 25% of duration
09  seek 50% of duration
10  seek 75% of duration
    (each seek: status class, reported landing or -1 sentinel,
     next read belongs to the landing, metadata/artwork/format
     unchanged)
11  EOF / drain                            → SONG_EOF terminal, produced == 0,
                                             EOF distinct from error, stable
12  malformed / truncated source           → typed fail-closed statuses,
                                             no fabricated data
13  close / lifetime cleanup               → views invalid after close;
                                             drop mid-stream then reopen
                                             stays stable
```

### 7.2 Frozen comparison set

For every step, the binding-visible result must match the direct-C
reference on:

```text
status / error class
units (µs positions, frames, Hz, bytes, microbels)
sentinel treatment (-1 duration, -1 landing, mask 0, has_* presence)
source facts (rate, channels, mask, duration, codec, container,
    selected index, stream count, is_default)
PCM format (Float32, interleaved, source rate, source layout)
PCM/reference policy (exact SHA-256 for lossless/PCM fixtures; the
    frozen deterministic tolerance/hash policy for lossy fixtures)
seek landing contract (at/before target, reported landing or explicit
    unknown; no manufactured landing)
EOF (status 1 = normal terminal, never an error)
ownership / lifetime behavior (view invalidation points, close
    idempotence, callback lifetime)
```

### 7.3 Evidence reuse (no duplicate corpus)

```text
fixtures + reference manifest:
    native/experiments/songcore-equivalence/fixtures
    native/experiments/songcore-equivalence/reference.json
existing gates reused as the reference implementation of this scenario:
    equivalence ABI export gate + correctness gate   (direct C consumer)
    songcore-call-comparison layout + surface gates  (C vs Rust binding)
    qianqian-decode-songcore tests                   (adapter layer)
```

Future bindings add a driver for **this scenario against this manifest** —
they do not create a second corpus or second reference numbers.

### 7.4 Maturity mapping

```text
direct C consumer passes §7 on a target   → CORE_RUNTIME for that target
a binding passes §7 with parity           → BINDING_RUNTIME for that
                                             (target, binding) pair
anything less                            → the lower state (§5)
```

---

## 8. ABI v1 inventory (derived projection of `native/include/songcore.h`)

**Not a second authority.** Header wins on any disagreement.

### 8.1 Exports (15, no FFmpeg type crosses them)

| Export | Returns | Notes |
|---|---|---|
| `songcore_abi_version` | `uint32_t` (1u) | callable at any time, no handle |
| `song_open` | `song_status` | all 3 `song_io` callbacks required; on error `*out_handle` untouched; no diagnostic for open failures |
| `song_probe` | `song_status` | idempotent (cached snapshot); selects default decodable audio stream |
| `song_audio_stream_count` | `song_status` | requires probed handle |
| `song_audio_stream_info` | `song_status` | `audio_index` in `0..count-1` |
| `song_select_stream` | `song_status` | resets position, rebuilds metadata, artwork NOT invalidated, `song_info` updated, old PCM gone; invalid index → `INVALID_ARGUMENT` |
| `song_get_metadata` | `song_status` | borrowed pointer to immutable snapshot |
| `song_get_metadata_count` | `song_status` | deterministic order: container scope, then stream scope; source parse order; duplicates kept |
| `song_get_metadata_entry` | `song_status` | string fields are views into the snapshot |
| `song_get_artwork_count` | `song_status` | 0..N, compressed bytes only |
| `song_get_artwork_item` | `song_status` | views until `song_close`; order = attached-picture stream order |
| `song_last_error` | `song_status` | diagnostics only; valid until the next SongCore call on the same handle; `SONG_OK` → NULL message |
| `song_read_pcm` | `song_status` | caller-owned `dst`; frames, not samples |
| `song_seek` | `song_status` | `out_actual_position_us` may be NULL |
| `song_close` | `void` | frees everything, invalidates all views, safe in any state, NULL is a no-op |

### 8.2 Status values

```text
SONG_OK   = 0      success
SONG_EOF  = 1      end of decoded PCM — normal terminal, never an error
SONG_ERR_INVALID_ARGUMENT = 100     SONG_ERR_STATE            = 101
SONG_ERR_NOT_OPEN         = 102     SONG_ERR_IO               = 103
SONG_ERR_UNSUPPORTED_CONTAINER = 104 SONG_ERR_NO_AUDIO_STREAM  = 105
SONG_ERR_UNSUPPORTED_CODEC = 106     SONG_ERR_CORRUPT_DATA     = 107
SONG_ERR_DECODE_ERROR     = 108     SONG_ERR_SEEK_UNSUPPORTED  = 109
SONG_ERR_SEEK_ERROR       = 110     SONG_ERR_STREAM_CHANGE    = 111
SONG_ERR_OUT_OF_MEMORY    = 112     SONG_ERR_INTERNAL_ERROR   = 113
```

Callers branch on status codes; `song_error` (message UTF-8,
`message_len` excludes NUL, `native_code`) is log diagnostics only.

### 8.3 Crossing structs

`song_error`, `song_io`, `song_info`, `song_stream_info`,
`song_metadata`, `song_metadata_entry`, `song_artwork_item`; opaque
`song_handle`; enums `song_status`, `song_metadata_scope`,
`song_artwork_role`; anonymous `SONG_CH_*` channel-mask constants.

### 8.4 Units

```text
positions / durations     int64 microseconds (duration_us, seek in/out)
PCM                       Float32 interleaved, counted in FRAMES
sample_rate               Hz (> 0)
channel count             > 0; channel_mask = u64 SMPTE/FFmpeg-style bits
ReplayGain                microbels (1e-6 dB); peaks ×100000 = full scale
I/O callbacks             bytes read; absolute byte offsets; total bytes
strings                   (pointer, length) UTF-8 views (metadata/artwork);
                          song_error.message is NUL-terminated + length
track/disc numbers        1-based, -1 absent (presence via has_*)
artwork width/height      -1 unknown
```

### 8.5 Sentinels

```text
duration_us == -1         container declares no duration
landing     == -1         seek landing unknown — explicit, never manufactured
channel_mask == 0         layout unknown; never guess order from count
bits_per_sample == 0      source depth not meaningful
has_* flags               presence is explicit; missing is not an error
SONG_EOF (= 1)            terminal condition, not an error and not a sentinel
```

### 8.6 Ownership, borrowed views, lifetimes

```text
song_handle                SongCore-owned from song_open until song_close
song_info / song_stream_info / song_metadata_entry / song_artwork_item
                           structs are caller-provided out-values; the
                           string/byte fields inside are borrowed views
song_metadata              borrowed pointer to the immutable snapshot
metadata / raw-entry views valid until: song_select_stream, or song_close;
                           stable across song_read_pcm, song_seek, EOF
artwork views              valid until song_close; stream selection does
                           NOT invalidate container artwork
song_error                 valid until the next SongCore call on the same
                           handle
read_pcm dst               caller-owned
song_io + userdata         caller-owned; must remain valid and callable
                           from song_open until song_close (the handle may
                           pull through the callbacks at any time in
                           between — implied by the header, realized by
                           keeping the host file alive for the handle
                           lifetime)
```

### 8.7 Thread safety

One `song_handle` is **not** internally thread-safe: calls on one handle
must be externally serialized. Different handles may be used concurrently.
SongCore adds no internal mutexes. A binding may offer `Send`-style move
semantics; it must not claim shared concurrent use of one handle.

### 8.8 PCM contract

Frozen: Float32, interleaved, **source** sample rate, **source** channel
layout (SongCore never resamples). `song_read_pcm` fills up to
`frame_capacity` frames: `SONG_OK` → `produced > 0` and `≤ capacity`;
`SONG_EOF` → `produced == 0`; any other status → `produced == 0` for that
call. Partial success: frames produced before a decode error in the same
call return `SONG_OK`; the error surfaces on the **next** call with zero
frames. Fail-closed format guard: mid-stream rate/layout change vs
`song_info` → `SONG_ERR_STREAM_CHANGE`, never contradicting PCM.
`frame_capacity == 0` → `SONG_ERR_INVALID_ARGUMENT`.

### 8.9 Seek contract

Playback-oriented, not sample-perfect unless the format proves it. Request
is clamped against the known duration, converted to a container seek
at/before the target, the decoder is flushed (all pending state cleared),
and the **next** `song_read_pcm` belongs to the landing.
`*out_actual_position_us` is measured from the first decoded frame's
timestamp; `-1` = unknown landing; may be NULL. Lossy/lapped codecs: first
frames may differ from sequential decode (bounded codec-frame tolerance);
lossless: frame-accurate. After a successful seek: metadata, artwork,
selected stream, rate/layout/codec/container unchanged. Errors:
`SEEK_UNSUPPORTED` / `SEEK_ERROR` / `STREAM_CHANGE` / `DECODE_ERROR`
(fail-closed).

### 8.10 EOF contract

`SONG_EOF` (1) is the normal terminal condition of decoded PCM, never an
error. `song_read_pcm` reports it with `produced == 0`; it is distinct
from every `SONG_ERR_*`; callers must never translate it into an error.

### 8.11 Metadata / artwork invalidation

```text
metadata snapshot   built after probe / stream selection;
                    selected stream overrides container per canonical
                    field; invalidated by the NEXT song_select_stream
                    (rebuilt for the new stream) and by song_close;
                    NOT invalidated by read/seek/EOF
container artwork   NOT changed by stream selection; invalidated only
                    by song_close; role best-effort (single item →
                    front cover; else mapped from source picture type,
                    else SONG_ARTWORK_UNKNOWN)
stream selection    also resets position, updates song_info, discards
                    old PCM/decoder state
```

---

## 9. Future target bindings (design only — not implemented here)

Follow-up issues own implementation; both keep the JNI/C-module layer as
**transport only**.

### Android

```text
Kotlin/product façade
       ↓
small JNI bridge .so          ← transport/adaptation only; no decoder
       ↓                        policy, no Android-specific decode path
SongCore C ABI v1
       ↓
static libsongcore.a
       ↓
Android-specific FFmpeg closure   (owned by #172)
```

An AAR may package the bridge + artifact; the AAR is not API authority.

### Apple

```text
Swift / Kotlin-Native
       ↓
C module / modulemap / cinterop
       ↓
SongCore C ABI v1
       ↓
static SongCore artifact
       ↓
XCFramework packaging where appropriate   (packaging, not API authority)
```

No Objective-C/Swift semantic API inside SongCore; convenience wrappers
stay above the canonical C ABI.

Both must pass the §7 oracle before claiming `BINDING_RUNTIME` (§5).

---

## 10. Versioning

Tracked separately; a change in one does not imply the others:

```text
SongCore semantic/API version   (this document + header contract)
SongCore ABI version            (SONGCORE_ABI_VERSION; v1 frozen)
native artifact/component version (songcore-vX.Y.Z, #172)
binding/package version         (crate / AAR / XCFramework)
```

A binding implementation change never implies ABI v2. ABI v1 is reopened
only on evidence that the C ABI cannot correctly express the
target-independent decode contract — never for JNI, Swift, Kotlin/Native,
AAR, or XCFramework convenience.
