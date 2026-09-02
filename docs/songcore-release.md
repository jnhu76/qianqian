# SongCore release

How SongCore becomes a frozen, independently consumable artifact. This is
the current-truth release document: what SongCore is, how the artifact is
produced, audited, and what freezes with v0.1.

For the caller contract read [songcore-api.md](songcore-api.md); for the
architecture read [audio-core.md](audio-core.md); for how the FFmpeg closure
is derived read [ffmpeg-minimization.md](ffmpeg-minimization.md).

## What SongCore is

```text
File / host IO
      ↓
SongCore v0.1  (machine-derived minimal FFmpeg closure hidden inside)
      ↓
Float32 interleaved PCM, source rate + layout
      ↓
(PlayerEngine / WASAPI — separate layers, not part of this artifact)
```

One merged static archive plus one shared library, one public header:

- public ABI: `include/songcore.h` — **ABI v1**, 15 symbols + version.
- primary artifact: `libsongcore.a` — the FFmpeg closure is merged inside;
  a consumer links one Qianqian archive + system libraries, nothing else.
- secondary: `libsongcore.so` — exports exactly the same 15 symbols
  (machine-audited); the Python ctypes consumer and playback demo
  (`tools/songcore_ffi_smoke.py --play`, `tools/play_smoke.py`) run on it.
- SongCore is a decoder core, not an audio engine: no SRC, no mixer, no DSP,
  no output backend, no loudness normalization, no metadata database.

## Release command

```bash
xmake f --av_manifest=build/ffmpeg-xmake/manifest.json -y   # PRODUCTION closure
python3 tools/songcore_release.py --target linux-x86_64
```

The tool fails closed through: session identity (must replay the production
`codec-base` profile — never the `songcore-test` closure), provenance
(FFmpeg pin, profile sha, target recipe), build, archive audit (every
closure unit present — never hollow; all 15 public symbols defined), ABI
layout gate (`tools/songcore_abi_dump.c` vs the frozen snapshot in
`bench/results/songcore-v1/`), external consumers (static C consumer,
Python ctypes decode, shared export audit), license gate (nonfree FAILS
the release; gpl/version3 are recorded, never defaulted), and a forced
rebuild compared byte-for-byte (member list + export set must be stable;
nondeterminism is recorded, never papered over). It stages
`build/release/songcore-v<version>-<target>/` — include, lib, metadata
(manifest / checksums / symbols), LICENSES, README — and writes the
in-repo release record `bench/results/songcore-v1/release-manifest.json`.

Publishing (git tag + GitHub release upload) is a separate human step and
is never automated by the tool.

## External consumers

`tests/consumer/songcore_static_smoke.c` is the outside-integrator proof:
it sees only `songcore.h` + the documented link line. The ctypes consumer
and playback demo consume the shared library with zero Qianqian code in the
loop. All three run inside the release tool; the permanent chain is
`tests/songcore/consumers.py` (WASM + recorded-Windows evidence in full
mode), provenance-pinned so a stale PASS cannot survive an artifact change.

## ABI freeze (v1)

`SONGCORE_ABI_VERSION` = 1. The frozen surface: function list, enum numeric
values, struct sizes and field offsets (`bench/results/songcore-v1/
abi-layout-<target>-v1.txt`), the 15-symbol export set, and the open /
probe / read / seek / close semantics in [songcore-api.md](songcore-api.md).
Any layout or semantic break is ABI v2 — an explicit `SONGCORE_ABI_VERSION`
bump plus a new layout snapshot. Thread-safety: a handle is externally
serialized; IO callback lifetime = handle lifetime; PCM format is Float32
interleaved, source rate/layout, native endianness; EOF is `SONG_EOF` with
zero frames; no C++ exception crosses the ABI; no FFmpeg type ever does.

## License profile (engineering compliance, not legal advice)

Derived from the actual build provenance in the closure manifest — never
defaulted: `--enable-nonfree` fails the release; `--enable-gpl` /
`--enable-version3` are recorded into `metadata/manifest.json` and change
the effective license text shipped in `LICENSES/`. The v0.1 production
profile enables none of these and no external `lib*` component, so the
effective license is **LGPL-2.1-or-later (FFmpeg)**, with the authoritative
FFmpeg license texts and the static-distribution relink note shipped in
`LICENSES/`. The Qianqian sources carry no open-source license yet; the
package grants none. This names the FFmpeg closure only — it is never a
claim that SongCore itself is LGPL-licensed ("SongCore is
LGPL-2.1-or-later" is not a statement this project makes); the
Qianqian/SongCore-owned code stays under the project's separate,
current licensing status.

## Release classification (current truth)

SongCore v0.1.0 is an **engineering freeze artifact**. It freezes the
ABI, binary shape, provenance, reproducibility, and external-consumer
contract — so the artifact is independently consumable by its holder.

The current Qianqian repository does **not** grant a public
redistribution license for the Qianqian/SongCore-owned portions. Public
redistribution requires a separate licensing/compliance decision and
must not be inferred from the FFmpeg LGPL notices. Accordingly,
`songcore-v0.1.0` is classified **ENGINEERING_FREEZE**, not
`PUBLIC_DISTRIBUTION_APPROVED`.

Upgrading a release to `PUBLIC_DISTRIBUTION_APPROVED` requires
re-confirming, at minimum:

1. an explicit Qianqian/SongCore distribution license;
2. the exact corresponding FFmpeg source availability;
3. the FFmpeg build/configure provenance;
4. the required LGPL relinkability obligations;
5. notices and license texts;
6. `nonfree = false`;
7. GPL/version3 propagation if enabled.

## Reproducibility

The release rebuilds twice and compares: artifact sha256, archive member
list, public export set. The linux-x86_64 v0.1 artifact is byte-identical
across forced rebuilds (deterministic `ar`); the semantic criteria are
checked regardless, and any byte-level nondeterminism would be recorded in
the release manifest rather than claimed away.

## Freeze policy (v0.1)

Frozen: `songcore.h` ABI v1; the Float32 PCM contract; open/probe/read/
seek/close semantics; the FFmpeg minimization profile needed by the
supported formats; the static artifact contract (one merged archive, no
internal dependency); the release manifest schema v1.

Tag scope: `songcore-v0.1.0` freezes the binary/artifact tree at
`522c16d`. Post-tag commits that only clarify repository distribution
policy do not change SongCore ABI, artifact, provenance, or binary
bits; the tag is not re-cut for them. The published v0.1.0 package
predates the classification wording in this document; its
redistribution posture is stated in its own
`LICENSES/THIRD_PARTY_NOTICES.txt` (Qianqian terms are NOT granted by
the package).

Allowed fixes after the freeze: correctness bugs, security issues,
license/compliance issues, build portability bugs, confirmed codec
regressions. New capabilities are not taken for "might be useful later".

Explicit non-goals for this artifact remain: WASAPI/output backends,
Player UI, playlist, database, metadata library, DSP/EQ/SRC/ReplayGain,
plugin systems, language wrappers, and any general-purpose FFmpeg SDK
surface.
