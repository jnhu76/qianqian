# Audio API boundary admission (#153)

Gate policy/evidence, derived from [PBK-001](../../docs/adr/ADR-PBK-001.md),
[PBK-002](../../docs/adr/ADR-PBK-002.md) and
[PBK-003](../../docs/adr/ADR-PBK-003.md); this is not new playback authority.
Baseline audit: `6e991a6ae37427491a8d79d0095f7c0d0a05ff6c`.

## Public surface inventory and rights

The crate root exposes **`pub mod ports`**, with no root re-export.
All 12 structs, 9 enums and 5 traits below are reachable through it.
[admitted-api.txt](admitted-api.txt) is the exact compiler-extracted structural
inventory, including method receivers, arguments, returns, bounds, enum payloads,
associated items, explicit/derived/auto **and blanket** implementations. None of
those impl classes is filtered: a new local blanket impl is a contract change.
This snapshot records admission for the gate, not a claim that every derived
standard-library method is normative playback authority.

| Public definition | Fields / variants / own methods | Classification and authority |
| --- | --- | --- |
| `AudioOutput` | `open_stream(&self, RenderRequest) -> Result<Box<dyn RenderStream>, OutputError>` | NORMATIVE_SHARED_CONTRACT; PBK-002 D1/D6/D7, PBK-003 §5 |
| `RenderStream: Send` | `negotiated_format(&self) -> PcmFormat`, `stop_and_join(self: Box<Self>)` | NORMATIVE_SHARED_CONTRACT; PBK-003 §5; join/release is mechanism duty, not terminal Fact |
| `RenderPcmInput: Send + Sync` | `read_frames(&self, &mut [f32]) -> PcmPull`, `stop(&self)` | NORMATIVE_SHARED_CONTRACT; PBK-002 D6/D8 |
| `PcmDecode` | `open_media(&self, &Path) -> Result<Box<dyn DecodedPcmStream>, DecodeOpenError>` | NORMATIVE_SHARED_CONTRACT; PBK-002 D1/D6/D7 |
| `DecodedPcmStream: Send` | `format(&self) -> PcmFormat`, `source_duration(&self) -> Option<Duration>`, `read_frames(&mut self, &mut [f32]) -> Result<DecodeOutcome, DecodeError>`, `seek(&mut self, Duration) -> ProviderSeekOutcome` | NORMATIVE_SHARED_CONTRACT; D14.5/.8; endpoint serialization belongs to caller |
| `AudioOutputCapability`, `PcmDecodeCapability` | unit structs; `Capability::{NAME: &'static str, Service = dyn AudioOutput / dyn PcmDecode}` | NORMATIVE_SHARED_CONTRACT; key identity at definition site, D1/D5/D7 |
| `PcmFormat` | `sample_rate: u32`, `channels: u16`, `channel_mask: u64` | NORMATIVE_SHARED_CONTRACT; source-rate/layout vocabulary, D1/D7 |
| `SourceFacts` | `format: PcmFormat`, `duration: Option<Duration>` | NORMATIVE_SHARED_CONTRACT; D14.6 stateless source preflight evidence, not episode authority |
| `RenderRequest` | `format: PcmFormat`, `input: Arc<dyn RenderPcmInput>`, `drain: DrainSignal`, `gate: RenderGate`, `level: OutputLevel`, `position: PositionEvidence` | MECHANISM_PUBLIC_FOR_PROVIDER; D6/D14.5/.7/.8/.9 and PBK-003 §5 |
| `OutputError`, `DecodeOpenError`, `DecodeError` | each has `message: String` | MECHANISM_PUBLIC_FOR_PROVIDER; open/read failure diagnostics, not a Fact |
| `DrainSignal` | `new`, `with_on_complete(impl Fn(DrainVerdict) + Send + Sync + 'static) -> Self`, `complete(&self, DrainVerdict)` | MECHANISM_PUBLIC_FOR_PROVIDER; D11/D14.3 render terminal evidence; observer installed by Session |
| `RenderGate` | `with_observer(impl Fn(GateEvent) + Send + Sync + 'static) -> Self`, `set_paused(&self, bool)`, `set_seek_hold(&self, bool)`, `release_seek_hold(&self, SeekParkRelease)`, `seek_release_pending(&self) -> bool`, `close_and_release(&self)`, `park_loop_top(&self, impl FnMut(GateSlice) -> TailProbeOutcome) -> ParkOutcome`; also `new` | MECHANISM_PUBLIC_FOR_PROVIDER for routing/park/release; D14.5/.7. `new`/`Default` are INCIDENTAL observer-free mechanism-test construction, preserved without granting production engagement authority |
| `PositionEvidence` | `new`, `publish_consumed(&self, u64, u64)`, `published(&self) -> Option<u64>`, `rebase(&self, Option<u64>)`; `MAX_POSITION: u64` | MECHANISM_PUBLIC_FOR_PROVIDER for publication/read/rebase; D14.5/.8. `MAX_POSITION` is INCIDENTAL representation limit/diagnostic, not a new Position proposition |
| `OutputLevel` | `new`, `route(&self, f32)`, `load(&self) -> f32` | MECHANISM_PUBLIC_FOR_PROVIDER; D14.9 desired stream factor; no acoustic readback |
| `PcmPull` | `Frames(usize)`, `Eof`, `Stopped` | NORMATIVE_SHARED_CONTRACT; bounded PCM edge outcomes, D6/D8 |
| `DecodeOutcome` | `Frames(usize)`, `Eof` | NORMATIVE_SHARED_CONTRACT; decode mechanism outcome, D1/D6 |
| `DrainVerdict` | `Drained`, `Aborted` | MECHANISM_PUBLIC_FOR_PROVIDER; D11/D14.3 evidence, not semantic settlement |
| `GateEvent` | `Engaged`, `TailQuiesced`, `Disengaged`, `SeekEngaged`, `SeekTailQuiesced`, `SeekDisengaged` | MECHANISM_PUBLIC_FOR_PROVIDER; D14.5/.7, attribution kept separate |
| `SeekParkRelease` | `Committed { landing: Option<u64> }`, `Aborted` | MECHANISM_PUBLIC_FOR_PROVIDER; D14.5 commit release, actual/unknown landing |
| `TailProbeOutcome` | `Pending`, `Quiesced`, `Failed` | MECHANISM_PUBLIC_FOR_PROVIDER; D14.5/.7, failure is not quiescence |
| `ParkOutcome` | `Proceeded`, `TailProbeFailed` | MECHANISM_PUBLIC_FOR_PROVIDER; D14.5/.7, mechanism failure report |
| `GateSlice` | `TailProbe`, `SeekRelease(SeekParkRelease)` | MECHANISM_PUBLIC_FOR_PROVIDER; render-owned observation/rebase execution, D14.5/.7/.8 |
| `ProviderSeekOutcome` | `Applied { landing: Option<u64> }`, `RefusedUnchanged`, `MutatedThenFailed { diagnostic: String }` | NORMATIVE_SHARED_CONTRACT; D14.5 three-class provider duty |

Existing `Clone`, `Copy`, `Debug`, `Default`, `PartialEq`, `Eq`, auto traits and
standard blanket methods are captured in the machine inventory. Their spelling
is existing Rust representation, not additional semantic authority. Four cells
(`DrainSignal`, `RenderGate`, `PositionEvidence`, `OutputLevel`) have private
storage; no storage fields are public. No UNKNOWN authority right was found in
this scoped audit. INCIDENTAL rights are retained explicitly because this task
forbids changing existing provider permissions; their presence does not authorize
new mutators or promote them to normative architecture.

Real consumers inspected: Playback `session.rs` acquires endpoint/format and binds
requests, `completion.rs` creates observers and routes intent, `edge.rs` implements
`RenderPcmInput`; Decode `lib.rs` implements `PcmDecode`/`DecodedPcmStream`, classifies
seek and returns `SourceFacts` from `probe_media`; Output `wasapi.rs` implements
`AudioOutput`/`RenderStream`, publishes drain/position, applies level and uses the
loop-top park seam. `open_abort.rs` uses observer-free gates in mechanism tests.
Public provider writers are necessary mechanisms behind these existing seams;
public in a shared contract crate does not make them application-facing commands.

## Alternatives and selected minimum

| Scheme | Coverage / failures | Maintenance / dependency cost |
| --- | --- | --- |
| A: existing `root_public_declarations` + text whitelist | Misses implicit trait methods and enum members; truncates signatures at some commas/array semicolons; nested same-line declarations bypass. Executed old-gate mutants passed incorrectly. | Cheap, but cannot satisfy this issue; rejected |
| B: existing Rust-aware `public-api` over rustdoc JSON | Resolves visible Rust names, trait signatures, enum payloads, fields, hidden exports and impls; diffs additions/deletions/changes. Requires successful rustdoc extraction. | Selected: pinned library, 17 locked transitive packages, small executable wrapper; no production/dev dependency, native/OpenSSL CLI stack or custom Rust parser |
| C: whole-file hash/token freeze | Catches any edit, including private body/comment changes; says nothing about API reachability. | Very small but excessive false RED/maintenance cost; rejected |
| C: new full API parser or framework | Would need Rust name/cfg/macro resolution and continuing language maintenance. | Larger than reusing rustdoc; rejected |

The narrow syn visitor is a **source policy guard**, not a second API extractor.
It allows the two existing files, root `ports`, documentation and existing built-in
derives; private `cfg(test)` modules are outside production. Other conditional
attributes, expanding attributes, external modules, public re-exports and item/type
macros require explicit review and RED before rustdoc. Built-in expression macros
reject opaque nested attribute/macro tokens (`#`/`!`, including nested groups).
This can conservatively reject harmless new syntax (including macro negation),
which must be reviewed rather than silently extending the supported grammar.
It closes the single-configuration blind spot instead of claiming a Linux build
sees Windows/feature-only API. The crate manifest is frozen structurally to its existing composition path
dependency and inherited version/edition; new dependencies/features/lib/build
configuration (including an auto-discovered build.rs) require review. This prevents
an imported procedural macro from shadowing a built-in derive by spelling. Private function-body edits and comments
continue to pass within this admitted grammar.

## Commands, admission changes and limits

One-time preparation (network permitted here, never inside the gate):

```sh
rustup toolchain install nightly-2025-11-22 --profile minimal
rustup target add --toolchain nightly-2025-11-22 x86_64-unknown-linux-gnu
cargo fetch --locked
cargo fetch --locked --manifest-path tools/audio-api-surface/Cargo.toml
```

The existing entrypoints now include this gate and 17 audio-api controls:

```sh
python3 tools/check_plugin_boundaries.py
python3 tools/check_plugin_boundaries.py --negative-controls
lefthook run pre-push --all-files
```

The gate runs offline/locked, uses a dedicated target directory, strips caller
Rust flags and extracts the canonical Linux target with hidden items included.
Exit **1** is a diagnosed contract diff/source-policy RED; missing prerequisite,
parse error, compilation or extraction failure is **2 TOOLING-FAIL**. A mutant
that merely fails to compile cannot count as contract RED. The controls require
both the RED class and a target-specific diagnostic, restore original bytes
(including line endings), then check the baseline again.

This is **structural public-contract admission**, not a full ABI, layout, semantics,
constant-value, default-method-body or all-platform/runtime proof. `public-api`
reports incomplete external references/synthetic IDs in this pinned baseline;
we do not treat its output as complete external-library verification. The source
guard rejects new public re-exports, including external-module expansion, instead
of relying on that limitation. Bounds/types/impls in the admitted crate are compared;
constant initializers and behavior remain subject to their existing authority and
verification. Pin upgrades need an explicit inventory diff and review.

For a deliberately authorized API change, generate a candidate into a temporary
file using the exact rustdoc/helper commands in `audio_api_surface()`; review its
diff against this inventory and ADR before replacing `admitted-api.txt`. There is
no update-snapshots switch or automatic acceptance path. Never change this snapshot
or negative expectations simply to obtain green checks.
