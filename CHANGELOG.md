# Changelog

Product releases use `qianqian-vMAJOR.MINOR.PATCH`. SongCore component
releases have their own tags and history.

## Unreleased

### Infrastructure

- Shared clean-source Windows release preparation, independent ZIP verification,
  adversarial controls and a tag-only publication workflow (#171).

## 0.1.0 — 2026-10-06

### Added

- Portable Windows x86_64 terminal player, playing local files and recursively
  scanned folders, including paths containing spaces and CJK characters.
- Now Playing, Playlist, Audio and Visualizer views; visible keyboard and mouse
  controls, help, responsive Wide / Normal / Compact / Minimum layouts.
- Temporary playlists with separate selection and current-track markers;
  sequential/shuffle order, repeat off/all/one and natural-completion auto-next.
- Pause/resume, relative and exact seek, and perceptual desired volume controls.
- Desired DSP enable/bypass, preamp, presets and custom EQ. Applied state is
  explicitly not reported; desired controls are not proof of applied processing.
- Live Spectrum, Peak/RMS and Waveform displays from player telemetry.
- Windows portable ZIP with QUICKSTART, licenses, third-party notices and build
  manifest; native SongCore decode and WASAPI output.

### Fixed / hardened

- Embedded-cover MP3 probing and bounded folder-scan diagnostics; unplayable
  candidates do not reject the entire folder.
- Shared-mode Audio Engine sample-rate-conversion fallback for unsupported
  output formats, perceptual volume taper and stable timeline layout at cutover.
- Package build rejects non-system runtime DLL imports.

### Known issues

- Rapid repeated Next/Previous can replay intermediate replacements and stall
  (#165 / navigation-burst research); no corrective is claimed here.
- Corrupt or unplayable-media handling remains future work (#231). A playback
  failure is reported rather than silently rewritten as completion.
- Windows x86_64/local media only. No GUI, persistent media library, playlist-file
  import/export, lyrics, cover display or skins.
- No gapless, bit-perfect or exclusive-mode claim. Visualizer cadence and modal
  polish remain follow-up work (#233–#235).

Historical sources: [published notes](https://github.com/jnhu76/qianqian/releases/tag/qianqian-v0.1.0),
[tagged source](https://github.com/jnhu76/qianqian/tree/b7a526c77173f69ba8efd14897a3c57e1a806bd4)
and the original ZIP. Changes after that commit are not attributed to 0.1.0.
The original manifest's self-hash defect is recorded in the release reality audit;
the published asset is immutable.
