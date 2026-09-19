# Stage A corpus census (addendum §6)

The repository's committed media fixtures all live in
`native/experiments/songcore-equivalence/fixtures/` (SongCore ABI v1
equivalence corpus). For the transport dogfood they are staged under
`C:\Users\Public\qianqian-dogfood\` under short names; nothing is
modified. All four cover one format class each but are all very short
(4–6 s), which is too short for seek/soak/volume-during-playback
scenarios; following the f6-open-smoke precedent, two SYNTHETIC media
files were generated locally with ffmpeg (sine) and are recorded here
honestly as synthetic dogfood media, not repository fixtures.

| staged name | source | codec/container | duration | rate/channels | class | used for |
|---|---|---|---|---|---|---|
| flac4.flac | repo fixture `flac-16-44-stereo.flac` | FLAC | 4 s | 44.1 kHz / 2 ch | lossless, short | A1 EOF, A14/A15/A20 playlist, A16-drain-stop |
| mp3cbr.mp3 | repo fixture `mp3-cbr-id3v23.mp3` | MP3 CBR (ID3v2.3) | 4 s | 44.1 kHz / 2 ch | lossy, short | A1 EOF, A14/A15/A20 playlist |
| alac4.m4a | repo fixture `alac-16-44-stereo.m4a` | ALAC in MP4 | 4 s | 44.1 kHz / 2 ch | lossless (m4a container), short | A1 EOF, A14 playlist |
| alac6.m4a | repo fixture `alac-long.m4a` | ALAC in MP4 | 6 s | 44.1 kHz / 2 ch | lossless (m4a), longer-of-corpus | A1 EOF |
| synth45.mp3 | SYNTHETIC (ffmpeg sine 440 Hz, 44.1 kHz stereo, 128k CBR) | MP3 CBR | 45 s | 44.1 kHz / 2 ch | lossy, long | seek/pause/volume/interaction scenarios, A19/A20 |
| synth30.flac | SYNTHETIC (ffmpeg sine 330 Hz, 44.1 kHz stereo) | FLAC | 30 s | 44.1 kHz / 2 ch | lossless, long | Open replacement target, A17–A19 |
| garbage.bin | 1024 random bytes | — | — | — | invalid candidate | A11 refusal, A15 nav refusal, A20 |
| vtest01..vtest24.mp3 | SYNTHETIC — renamed copies of `synth45.mp3` (staged by `tools/run-tui.sh`) | MP3 CBR | 45 s each | 44.1 kHz / 2 ch | 24-entry list | U2-viewport (pane windowing + selection scroll) |
| u2soak/soak01..20.mp3 | SYNTHETIC (ffmpeg sine 300+ n Hz, 44.1 kHz stereo, 128k CBR, staged by `tools/run-tui.sh`) | MP3 CBR | 100 s each | 44.1 kHz / 2 ch | 20-entry soak list | U2-soak (~33 min end-to-end playback) |

Known seekability: all real fixtures are block-aligned CBR/lossless
sources exercised by the SongCore ABI v1 equivalence corpus; synth45 /
synth30 are block-aligned sines. Truncated/corrupt corpus members from
the F4 duration study are deliberately NOT used here (the invalid Open
class is represented by garbage.bin / a nonexistent path).

The U2 (Issue #166) scenarios additionally use `flac4.flac` /
`mp3cbr.mp3` / `alac4.m4a` / `alac6.m4a` / `synth30.flac` / `千曲.flac`
for the EOF/transition cases; the EOF TRANSITION targets are deliberately
non-MP3 where possible, because an MP3 open streams ffmpeg decoder
warnings onto the pseudoconsole and can hold the feedback row polluted
longer than a 4 s episode lasts.

SHA256 of every staged file is recorded per-run in
`evidence/ENV-TUI-RUN<N>.txt`.
