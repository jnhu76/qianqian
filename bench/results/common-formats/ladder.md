# Common Formats capability ladder (machine-generated)

| Stage | Capability | Oracle TU | Reachable TU | `-O3 .a` | `-Os .a` | linked stripped | `.so` stripped | min xRT | Gate |
|---|---|---:|---:|---:|---:|---:|---:|---:|---|
| c0 | MP3+FLAC | 202 | 106 | 1.46 MiB | — | 930 KiB | 539 KiB | 474× (flac-24-96) | PASS |
| c1 | +AAC/M4A +ADTS | 237 | 157 | 2.69 MiB | — | 1.78 MiB | 1.06 MiB | 526× (flac-24-96) | PASS |
| c2 | +ALAC/M4A | 240 | 160 | 2.71 MiB | — | 1.80 MiB | 1.07 MiB | 506× (alac-long) | PASS |
| c3 | +PCM WAV (6 fmts) | 243 | 163 | 2.74 MiB | — | 1.81 MiB | 1.08 MiB | 519× (flac-24-96) | PASS |
| c4 | +Ogg Vorbis | 257 | 180 | 2.92 MiB | — | 1.93 MiB | 1.16 MiB | 533× (flac-24-96) | PASS |
| c5 | +Ogg Opus | 276 | 198 | 3.25 MiB | — | 2.15 MiB | 1.28 MiB | 541× (alac-long) | PASS |
| c6 | Common Formats minimized | 198 | 198 | 3.25 MiB | 2.97 MiB | 1.26 MiB | 1.25 MiB | 393× (alac-long) | PASS |

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
| aac-lc-44-stereo | 1792× | 1721× | 1722× |
| alac-long | 541× | 400× | 393× |
| flac-16-44-stereo | 1249× | 1037× | 1016× |
| flac-24-96 | 556× | 459× | 463× |
| mp3-cbr-id3v23 | 2166× | 1602× | 1650× |
| mp3-long | 2105× | 1633× | 1672× |
| mp3-vbr-id3v24 | 2416× | 1870× | 1905× |
| opus-48-stereo | 559× | 496× | 500× |
| vorbis-44-stereo | 1762× | 1557× | 1669× |
| wav-f32le-44-stereo | 5610× | 5474× | 5915× |
| wav-s16le-44-stereo | 19395× | 27093× | 26982× |

Final size-minimal `.so` (c6-so-lto): raw 1.37 MiB, stripped 1.25 MiB, stripped+xz 492 KiB, exports 5 SongCore APIs, deps: linux-vdso.so.1 (0x00007f789862d000); 	libm.so.6 => /usr/lib/x86_64-linux-gnu/libm.so.6 (0x00007f78984f0000); 	libc.so.6 => /usr/lib/x86_64-linux-gnu/libc.so.6 (0x00007f7897e00000); 	/lib64/ld-linux-x86-64.so.2 (0x00007f789862f000)
