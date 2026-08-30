# Common Formats capability ladder (machine-generated)

| Stage | Capability | Oracle TU | Reachable TU | `-O3 .a` | `-Os .a` | linked stripped | `.so` stripped | min xRT | Gate |
|---|---|---:|---:|---:|---:|---:|---:|---:|---|
| c0 | MP3+FLAC | 202 | 106 | 1.46 MiB | — | 930 KiB | 539 KiB | 303× (flac-24-96) | PASS |
| c1 | +AAC/M4A +ADTS | 237 | 157 | 2.69 MiB | — | 1.78 MiB | 1.06 MiB | 440× (flac-24-96) | PASS |
| c2 | +ALAC/M4A | 240 | 160 | 2.71 MiB | — | 1.80 MiB | 1.07 MiB | 400× (flac-24-96) | PASS |
| c3 | +PCM WAV (6 fmts) | 243 | 163 | 2.74 MiB | — | 1.81 MiB | 1.08 MiB | 349× (alac-long) | PASS |
| c4 | +Ogg Vorbis | 257 | 180 | 2.92 MiB | — | 1.93 MiB | 1.16 MiB | 442× (flac-24-96) | PASS |
| c5 | +Ogg Opus | 276 | 198 | 3.25 MiB | — | 2.15 MiB | 1.28 MiB | 422× (flac-24-96) | PASS |
| c6 | Common Formats minimized | 198 | 198 | 3.25 MiB | 2.97 MiB | 1.26 MiB | 1.25 MiB | 287× (alac-long) | PASS |

## Codec marginal cost per capability increment

| Capability increment | Δ reachable TU | Δ `-O3 .a` | Δ `.so` stripped |
|---|---:|---:|---:|
| +AAC/M4A +ADTS | +51 | +1256 KiB | +548 KiB |
| +ALAC/M4A | +3 | +19 KiB | +4 KiB |
| +PCM WAV (6 fmts) | +3 | +35 KiB | +16 KiB |
| +Ogg Vorbis | +17 | +177 KiB | +84 KiB |
| +Ogg Opus | +18 | +347 KiB | +124 KiB |
| Common Formats minimized | +0 | +0 B | -36 KiB |

## Codegen throughput tradeoff (full Common Formats set, songcore-output xRT)

| Codec sample | `-O3` | `-Os` | `-Os+LTO` |
|---|---:|---:|---:|
| aac-lc-44-stereo | 1455× | 1281× | 1161× |
| alac-long | 461× | 319× | 287× |
| flac-16-44-stereo | 969× | 761× | 818× |
| flac-24-96 | 422× | 364× | 351× |
| mp3-cbr-id3v23 | 1578× | 796× | 1446× |
| mp3-long | 1610× | 1105× | 1366× |
| mp3-vbr-id3v24 | 1361× | 1472× | 1572× |
| opus-48-stereo | 472× | 387× | 386× |
| vorbis-44-stereo | 1420× | 801× | 1343× |
| wav-f32le-44-stereo | 3677× | 3628× | 4512× |
| wav-s16le-44-stereo | 11408× | 13734× | 9571× |

Final size-minimal `.so` (c6-so-lto): raw 1.37 MiB, stripped 1.25 MiB, stripped+xz 492 KiB, exports 5 SongCore APIs, deps: linux-vdso.so.1 (0x00007e92c4a47000); 	libm.so.6 => /usr/lib/x86_64-linux-gnu/libm.so.6 (0x00007e92c490a000); 	libc.so.6 => /usr/lib/x86_64-linux-gnu/libc.so.6 (0x00007e92c4200000); 	/lib64/ld-linux-x86-64.so.2 (0x00007e92c4a49000)
