# ARTIFACT — verified Windows x86_64 SongCore target artifact

```text
TRUTH CLASS   executed verification evidence (artifact gate)
SUBJECT       #324 Phase 1: the Windows target artifact every cell links
SCOPE         VERIFICATION-GRADE artifact for this campaign.
              NOT a SongCore release artifact and NOT a #172 release claim.
```

## 1. What was built

```text
artifact          native/build/artifacts/libsongcore.a
bytes             3,309,678
sha256            90bde1ca3f9924842ebb06005219f746d5053499a03fc3afafebccd1d57ba6d2
members           280   (279 FFmpeg translation units + songcore_ffmpeg.c.obj)
object format     280/280 pe-x86-64 (COFF), 0 ELF members
merged closure    build.merge_archive (Xmake ar/lib.exe policy): one archive,
                  no separate internal archive on the consumer link line
```

Member extraction note: read a member with `ar x` into a scratch directory and
hash the extracted file. `ar p <archive> <member> | sha256sum` is **not**
binary-safe in this Git Bash — the pipe converts `
` to `
` and yields a
different hash. `raw/smoke/artifact-identity.txt` uses `ar x`.

The format audit is in `raw/smoke/artifact-identity.txt`
(`objdump -f … | grep -c 'file format pe-x86-64'` = 280, `… elf` = 0). This is
the prohibition the issue names: the archive is **not** a Linux `libsongcore.a`
copied into a Windows directory, and it is not byte-identical to the Linux
artifact of record (`a0a3baf8…`, 4,067,772 bytes).

## 2. Source identity

```text
SOURCE_SHA (git HEAD)        9ffcaba945eb8f47b9e30dddc67c805a5bda078a
native/src/songcore_ffmpeg.c 55eab2432bf2f1959fe13b3f97282895b5918983925f175cad521b0ae5c05ad6
native/include/songcore.h    cf509a5c044735e4813642ac6683691242146e642342a8ac356e98fbd8e78536
native/xmake.lua             b420fc6c792fafd434309c70dc1cc0f2b4cf69b4e8701d65b28d9cc752708e63
native/build/ffmpeg.lua      aa0c95718154ef41b5ab7584a5c893d22c99759340f6a30287452baed96a5cd3
native/build/songcore.lua    4f81a90658957b657155bc13b86ce66d7b94033c36dca2878b84cf363f29959f
ffmpeg/targets/windows-x86_64.json
                             3ba0a9e686d169f0e00ba46ade11a8f8658f204537713eabe200633ad5f8492d
```

`native/src/songcore_ffmpeg.c` is the same C source the Linux #323 package
executed (`55eab243…`); the Windows artifact is a different build of the same
contract, not a transplanted binary.

## 3. FFmpeg pin and target-specific closure

```text
FFMPEG_TAG        n9.0.1
FFMPEG_COMMIT     bf1b838f2ab88b4f8fd83443325c782ea0e0f7fa
SOURCE_ZIP_SHA256 d16837ddbb0963753aa2739971a09b07fe14fe855a6e84f253f851127f99c746
                  (sha256 verified at fetch; tag→commit re-verified: ok)
PROFILE           codec-base (demuxers aac/flac/mov/mp3/ogg/wav;
                  decoders aac/alac/flac/mp3float/opus/vorbis/pcm_*;
                  parser mpegaudio; avformat+avcodec+swresample)
MANIFEST          native/build/manifest.json
MANIFEST_SHA256   0d0f7af9a6bf102be92cece254319f8b12261dd822feefa2f3125ca9ed0aed14
CLOSURE           279 translation units (0 generated); schema 2
TARGET ID         windows-x86_64   (platform windows, arch x86_64)
TARGET_OS BINDING mingw32           CROSS_PREFIX x86_64-w64-mingw32-
ORACLE ARCHIVES   libavcodec  6cdbb559ec4026fec46f15e10a4b16b288c77fc88696404180f99191a48f7718
                  libavformat 8b5535c39bc5caaed324fdf3a42cdbc1735a5fdb30b3fa102b6cd68ca3036955
                  libavutil   82db6473b72b3e961b5e9b744887133e260cdf8e2c0415e57071c7eaa6575a36
                  libswresample 40b250976071f4e5b3b9f20c164a9035d82125290fbbc81fe1281d4dde15353d
```

The manifest's `target` block is the fail-closed identity `native/build/ffmpeg.lua`
checks before any build: a Linux manifest (`target_os=linux`, no cross prefix)
can never satisfy this session, and an MSVC session could never satisfy a
mingw32 manifest. The import was produced with an explicit recipe selection:

```bash
# WSL2 build assistance (allowed): the target recipe owns the toolchain facts
cd native && python3 tools/ffmpeg_import.py --recipe windows-x86_64
xmake f -p mingw -a x86_64 -m release -y
xmake build songcore
```

## 4. Toolchain provenance

```text
artifact compiler   x86_64-w64-mingw32-gcc (GCC) 13-win32        [cross, WSL2]
artifact archiver   GNU ar 2.45.90.20260125                        [cross, WSL2]
artifact linker     GNU ld 2.45.90.20260125                        [cross, WSL2]
build driver        xmake v2.9.7+20250101
                    (captured in raw/smoke/artifact-toolchain.txt)
consumer compiler   gcc.exe (MinGW-W64 x86_64-msvcrt-posix-seh,    [Windows host]
                    built by Brecht Sanders, r1) 16.2.0
consumer linker     GNU ld 2.47.20260726 (MinGW-W64 x86_64)        [Windows host]
consumer Rust       rustc 1.96.0, target x86_64-pc-windows-gnu     [Windows host]
```

Both sides are the same linker family (GNU/mingw-w64, not MSVC). No COFF/MSVC
mix is involved.

## 5. ABI gate

```text
header            native/include/songcore.h  sha256 cf509a5c…
ABI version       1  (songcore_abi_version() returned 1 at runtime)
defined symbols   15/15 ABI v1 entry points resolve in the archive:
                  songcore_abi_version, song_last_error,
                  song_open, song_probe, song_audio_stream_count,
                  song_audio_stream_info, song_select_stream,
                  song_get_metadata, song_get_metadata_count,
                  song_get_metadata_entry, song_get_artwork_count,
                  song_get_artwork_item, song_read_pcm, song_seek, song_close
```

## 6. Real Windows consumer smoke (BUILD → LINK → RUN → CORE)

`harness/songcore_consumer_smoke.c` is a plain C consumer compiled **on the
Windows host** and linked against the archive:

```bash
gcc -O2 -Wall -Wextra -I native/include \
  evidence/windows-native-tier2-324/harness/songcore_consumer_smoke.c \
  native/build/artifacts/libsongcore.a -lbcrypt \
  -o evidence/windows-native-tier2-324/raw/smoke/songcore_consumer_smoke.exe
./…/songcore_consumer_smoke.exe native/experiments/songcore-equivalence/fixtures/flac-16-44-stereo.flac
```

Executed output (`raw/smoke/consumer-smoke.txt`, fixture sha256
`d80626686047762d2b3334ca27536799524b42d0e89aa004bd1bbe776551a4a2`):

```text
ABI_VERSION=1
SOURCE_BYTES=146237
OPEN status=0
PROBE status=0 rate=44100 channels=2 mask=3 duration_us=4000000 codec=flac container=flac streams=1 selected=0
METADATA status=0 has_title=1 has_artist=1
READ_FRAMES=176400 reads=22 eof=1
SEEK status=0 landing_us=940408
POST_SEEK_READ status=0 frames=8192
CONSUMER_SMOKE=PASS
```

That is a real PE executable, running the real statically-linked SongCore +
pinned FFmpeg closure over a real FLAC file: open → probe → metadata →
full decode → EOF → seek → post-seek read → close.

The Rust adapter consumes the same artifact (`raw/smoke/s2-rust-adapter.txt`):

```text
FORMAT rate=44100 channels=2 mask=3
DECODE frames=176400 reads=22 eof=1
SEEK outcome=Applied { landing: Some(41472) }
POST_SEEK frames=8192
S2_VERDICT=PASS
```

## 7. Limits of this artifact claim

```text
CLAIMED     a Windows x86_64 COFF SongCore archive with the target-specific
            FFmpeg closure, ABI v1, and real Windows PE consumption/runtime
            evidence — sufficient to gate the #324 fault cells.
NOT CLAIMED
            - a SongCore release artifact, packaging, or version (owned by #172)
            - MSVC/COFF-MSVC, Windows arm64, or shared/DLL support
              (the windows-x86_64 recipe declares shared=false; the DLL link
              currently needs BCrypt and is out of this scope)
            - reproducibility of the archive hash across rebuilds (absolute
              build paths are embedded in debug info; the C source hash is the
              stable identity)
            - any product behavior change: this campaign changed no production
              source, no ABI, and no build recipe.
```
