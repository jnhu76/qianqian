# Third-party notices — qianqian-windows-x86_64

This file records the third-party material that is actually part of the
shipped `qianqian.exe` (everything is linked statically; the package
ships no third-party DLLs) or shipped beside it, with the obligations
that follow. It is generated per release from the repository's build
reality — see BUILD-MANIFEST.txt in this folder for the exact source
commit, artifact hashes and build command of this package.

## FFmpeg (statically linked, LGPL-2.1-or-later)

`qianqian.exe` contains a trimmed FFmpeg n9.0.1 build inside the
SongCore static archive:

- Upstream: https://ffmpeg.org — © FFmpeg developers, licensed under
  the GNU Lesser General Public License version 2.1 or later
  (LGPL-2.1-or-later).
- Exact source of this build: FFmpeg tag `n9.0.1`
  (commit `bf1b838f2ab88b4f8fd83443325c782ea0e0f7fa`),
  https://github.com/FFmpeg/FFmpeg/archive/refs/tags/n9.0.1.zip
  (source sha256 `d16837ddbb0963753aa2739971a09b07fe14fe855a6e84f253f851127f99c746`,
  verified at import time by the repository's pinned fetch script).
- This build is compiled with everything disabled except the LGPL
  components `libavformat`, `libavcodec`, `libswresample`, with only
  these demuxers (aac, flac, mov, mp3, ogg, wav), decoders (aac, alac,
  flac, mp3float, opus, vorbis, pcm_u8/s16le/s24le/s32le/f32le/f64le)
  and the mpegaudio parser enabled. No GPL components (no x264/x265,
  no `--enable-gpl`), no network, no programs.
- LGPL-2.1 static-linking obligations: the complete corresponding
  source of FFmpeg is the pinned upstream tag above; the complete
  build recipe and compile closure that produced the linked objects
  live in this repository (`native/` — pinned source fetch with
  sha256 verification, the frozen compile-closure manifest, and the
  Xmake replay build). Object-file availability for relinking can be
  requested against the recorded source commit.
- The LGPL-2.1 full text: https://www.gnu.org/licenses/old-licenses/lgpl-2.1.txt

## Rust toolchain and standard library

`qianqian.exe` is produced by the Rust toolchain recorded in
BUILD-MANIFEST.txt. The Rust standard library parts linked into the
executable are Copyright (c) The Rust Project Developers, under MIT OR
Apache-2.0 (https://www.rust-lang.org/policies/licenses).

## Rust dependencies (statically linked)

All crates below are compiled into `qianqian.exe`. Licenses are
permissive (MIT, Apache-2.0, MIT OR Apache-2.0, Zlib, BSL-1.0,
Unicode-3.0 as noted); each crate's license text is available in its
published source archive (crates.io / the repository recorded in its
crate metadata). Direct dependencies are marked •.

- • ratatui 0.30 — MIT
- • crossterm 0.29 — MIT
- • qianqian crates are Qianqian's own code (see LICENSE)
- allocator-api2 0.2 — MIT OR Apache-2.0
- bitflags 2 — MIT OR Apache-2.0
- castaway 0.2 — MIT
- cfg-if 1 — MIT OR Apache-2.0
- compact_str 0.9 — MIT
- convert_case 0.10 — MIT
- critical-section 1 — MIT OR Apache-2.0
- crossterm_winapi 0.9 — MIT
- darling 0.24 — MIT
- deranged 0.5 — MIT OR Apache-2.0
- derive_more 2 — MIT
- document-features 0.2 — MIT OR Apache-2.0
- either 1 — MIT OR Apache-2.0
- equivalent 1 — Apache-2.0 OR MIT
- foldhash 0.2 — Zlib
- hashbrown 0.16 / 0.17 — MIT OR Apache-2.0
- heck 0.5 — MIT OR Apache-2.0
- ident_case 1 — MIT/Apache-2.0
- indoc 2 — MIT OR Apache-2.0
- instability 0.3 — MIT
- itertools 0.14 — MIT OR Apache-2.0
- itoa 1 — MIT OR Apache-2.0
- kasuari 0.4 — MIT OR Apache-2.0
- line-clipping 0.3 — MIT OR Apache-2.0
- litrs 1 — MIT OR Apache-2.0
- lock_api 0.4 — MIT OR Apache-2.0
- lru 0.18 — MIT
- num-conv 0.2 — MIT OR Apache-2.0
- parking_lot 0.12 (+ core) — MIT OR Apache-2.0
- powerfmt 0.2 — MIT OR Apache-2.0
- proc-macro2 1 — MIT OR Apache-2.0
- quote 1 — MIT OR Apache-2.0
- rustc_version 0.4 — MIT OR Apache-2.0
- rustversion 1 — MIT OR Apache-2.0
- ryu 1 — Apache-2.0 OR BSL-1.0
- scopeguard 1 — MIT OR Apache-2.0
- semver 1 — MIT OR Apache-2.0
- smallvec 1 — MIT OR Apache-2.0
- static_assertions 1 — MIT OR Apache-2.0
- strsim 0.11 — MIT
- strum 0.28 (+ macros) — MIT
- syn 2 / 3 — MIT OR Apache-2.0
- thiserror 2 (+ impl) — MIT OR Apache-2.0
- time 0.3 (+ core) — MIT OR Apache-2.0
- unicode-ident 1 — (MIT OR Apache-2.0) AND Unicode-3.0
- unicode-segmentation 1 — MIT OR Apache-2.0
- unicode-truncate 2 — MIT OR Apache-2.0
- unicode-width 0.2 — MIT OR Apache-2.0
- winapi 0.3 (+ winapi-x86_64-pc-windows-gnu) — MIT/Apache-2.0
- windows 0.62 family (windows, windows-core, windows-result,
  windows-strings, windows-link, windows-collections, windows-future,
  windows-implement, windows-interface, windows-numerics,
  windows-threading) — MIT OR Apache-2.0

## Fonts / glyphs

The interface uses the terminal's own font. No font files are shipped.
