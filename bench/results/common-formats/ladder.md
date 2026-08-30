# Common Formats capability ladder (machine-generated)

| Stage | Capability | Oracle TU | Reachable TU | `-O3 .a` | `-Os .a` | linked stripped | `.so` stripped | min xRT | Gate |
|---|---|---:|---:|---:|---:|---:|---:|---:|---|
| c0 | MP3+FLAC | 202 | 106 | 1.46 MiB | — | 930 KiB | 539 KiB | 548× | PASS |
| c1 | +AAC/M4A +ADTS | 237 | 157 | 2.69 MiB | — | 1.78 MiB | 1.06 MiB | 531× | PASS |
| c2 | +ALAC/M4A | 240 | 160 | 2.71 MiB | — | 1.80 MiB | 1.07 MiB | 525× | PASS |
| c3 | +PCM WAV (6 fmts) | 243 | 163 | 2.74 MiB | — | 1.81 MiB | 1.08 MiB | 468× | PASS |
| c4 | +Ogg Vorbis | 257 | 180 | 2.92 MiB | — | 1.93 MiB | 1.16 MiB | 486× | PASS |
| c5 | +Ogg Opus | 276 | 198 | 3.25 MiB | — | 2.15 MiB | 1.28 MiB | 489× | PASS |
| c6 | Common Formats minimized | 198 | 198 | 3.25 MiB | 2.97 MiB | 1.26 MiB | 1.25 MiB | 372× | PASS |

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
| aac-lc-44-stereo | 1742× | 1539× | 1576× |
| alac-long | 521× | 333× | 372× |
| flac-16-44-stereo | 992× | 849× | 977× |
| flac-24-96 | 489× | 359× | 435× |
| mp3-cbr-id3v23 | 1968× | 848× | 1528× |
| mp3-long | 2009× | 837× | 1596× |
| mp3-vbr-id3v24 | 2084× | 1256× | 1891× |
| opus-48-stereo | 538× | 430× | 471× |
| vorbis-44-stereo | 1583× | 1386× | 1554× |
| wav-f32le-44-stereo | 5597× | 4472× | 5808× |
| wav-s16le-44-stereo | 17653× | 15975× | 14570× |

Final size-minimal `.so` (c6-so-lto): raw 1.37 MiB, stripped 1.25 MiB, stripped+xz 492 KiB, exports 5 SongCore APIs, deps: linux-vdso.so.1 (0x00007bc71fcc3000); 	libm.so.6 => /usr/lib/x86_64-linux-gnu/libm.so.6 (0x00007bc71f8cb000); 	libc.so.6 => /usr/lib/x86_64-linux-gnu/libc.so.6 (0x00007bc71f600000); 	/lib64/ld-linux-x86-64.so.2 (0x00007bc71fcc5000)
