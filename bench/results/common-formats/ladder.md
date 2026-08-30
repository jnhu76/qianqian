# Common Formats capability ladder (machine-generated)

| Stage | Capability | Oracle TU | Reachable TU | `-O3 .a` | `-Os .a` | linked stripped | `.so` stripped | min xRT | Gate |
|---|---|---:|---:|---:|---:|---:|---:|---:|---|
| c0 | MP3+FLAC | 202 | 106 | 1.46 MiB | — | 930 KiB | 539 KiB | 468× | PASS |
| c1 | +AAC/M4A +ADTS | 237 | 157 | 2.69 MiB | — | 1.78 MiB | 1.06 MiB | 479× | PASS |
| c2 | +ALAC/M4A | 240 | 160 | 2.71 MiB | — | 1.80 MiB | 1.07 MiB | 466× | PASS |
| c3 | +PCM WAV (6 fmts) | 243 | 163 | 2.74 MiB | — | 1.81 MiB | 1.08 MiB | 398× | PASS |
| c4 | +Ogg Vorbis | 257 | 180 | 2.92 MiB | — | 1.93 MiB | 1.16 MiB | 442× | PASS |
| c5 | +Ogg Opus | 276 | 198 | 3.25 MiB | — | 2.15 MiB | 1.28 MiB | 454× | PASS |
| c6 | Common Formats minimized | 198 | 198 | 3.25 MiB | 2.97 MiB | 1.26 MiB | 1.25 MiB | 259× | PASS |

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
| aac-lc-44-stereo | 1699× | 695× | 1039× |
| alac-long | 487× | 200× | 259× |
| flac-16-44-stereo | 1085× | 530× | 809× |
| flac-24-96 | 454× | 211× | 287× |
| mp3-cbr-id3v23 | 1862× | 632× | 1163× |
| mp3-long | 1700× | 640× | 1097× |
| mp3-vbr-id3v24 | 2032× | 385× | 1003× |
| opus-48-stereo | 498× | 229× | 294× |
| vorbis-44-stereo | 1645× | 588× | 1335× |
| wav-f32le-44-stereo | 5358× | 2290× | 4527× |
| wav-s16le-44-stereo | 17424× | 4672× | 9759× |

Final size-minimal `.so` (c6-so-lto): raw 1.37 MiB, stripped 1.25 MiB, stripped+xz 492 KiB, exports 5 SongCore APIs, deps: linux-vdso.so.1 (0x00007f183c6e9000); 	libm.so.6 => /usr/lib/x86_64-linux-gnu/libm.so.6 (0x00007f183c2da000); 	libc.so.6 => /usr/lib/x86_64-linux-gnu/libc.so.6 (0x00007f183c000000); 	/lib64/ld-linux-x86-64.so.2 (0x00007f183c6eb000)
