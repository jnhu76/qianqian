# History

How the current decisions were reached. Details live in git history and
merged PRs; this page keeps only the durable conclusions.

- **FFmpeg minimization** → capability-driven configure oracle + Xmake
  replay validated (see [ffmpeg-minimization.md](ffmpeg-minimization.md)).
- **Common Formats** → MP3/FLAC/AAC/M4A/ALAC/WAV/Vorbis/Opus validated
  through the SongCore contract; regression in `tests/songcore/`.
- **WASM** → technically viable; not required as the default native path.
  WASM stays a documented future target behind the same ABI.
- **PCM processing** → explicit bypass/processing boundary validated;
  SongCore emits source-rate Float32 and never processes.
- **DSP/SRC** → trimmed libavfilter + aresample/libswresample selected
  (intent: `ffmpeg/capabilities/dsp.json`).
- **SongCore** → metadata/artwork/typed errors/seek/stream policy
  finalized as ABI v1 (`include/songcore.h`).
- **Native vs WASM / audio quality / codec support** → decisions are
  corpus- and benchmark-driven; the evidence gates live in
  `tests/songcore/` and `bench/results/songcore-v1/`.
