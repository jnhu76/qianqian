#!/usr/bin/env python3
"""Plugin boundary gate (H2, plugin-boundary-conformance-audit §7.2).

Enforces the Qianqian internal Cargo topology: every dependency edge
between qianqian-* crates — normal, dev, build, optional, and
target-specific alike — must be explicitly admitted by the rules below.
There is no denylist: an unadmitted edge fails closed. A new internal
crate must also be admitted explicitly (universe-level allowlist), so
sneaking a crate into the workspace fails too.

Inputs:
  - `cargo metadata --no-deps --format-version 1` for the root workspace
    (metadata does not run build scripts);
  - the same command per explicitly-known workspace-EXCLUDED production
    crate (qianqian-decode-songcore, qianqian-songcore-sys): exclusion
    from the workspace must never mean exclusion from the architecture.
  - export-surface rules (source-level): the public production surface
    of the playback / decode / output crates must not re-open concrete
    mechanism access. For qianqian-playback and qianqian-output-wasapi
    this duplicates what rustc already enforces on every workspace CI
    build. For the workspace-excluded decode chain this source gate is
    the durable continuous watcher (enforcement class:
    SOURCE_GATE_ENFORCED — honest upgrade path: real native compile CI,
    which needs an xmake + pinned-FFmpeg toolchain project, out of
    scope here, audit §7.5 / MAJOR-3).

experiments/ is intentionally not part of the production universe
(audit Appendix A): it is not production architecture.

Usage:
  python3 tools/check_plugin_boundaries.py                    # gate
  python3 tools/check_plugin_boundaries.py --negative-controls # prove non-vacuity

Exit codes: 0 PASS, 1 FAIL, 2 TOOLING-FAIL (refuses to mutate a dirty tree).
"""

import argparse
import json
import subprocess
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

# ---------------------------------------------------------------------------
# Universe: production crates this gate knows about.

WORKSPACE_EXCLUDED_PRODUCTION = [
    "crates/qianqian-decode-songcore",
    "crates/qianqian-songcore-sys",
]

INTERNAL_PREFIX = "qianqian-"

# ---------------------------------------------------------------------------
# Admitted dependency edges (allowlist-first; audit §7.2 / MAJOR-2).
# Every qianqian-* → qianqian-* edge must appear here, per dependency kind.

NORMAL_EDGES = {
    "qianqian-composition": set(),  # K0 depends on no domain crate (D2/D8)
    "qianqian-audio-api": {"qianqian-composition"},  # shared contracts only (D7)
    "qianqian-app": {"qianqian-composition"},  # generic admission layer (D3/D5)
    "qianqian-playback": {
        "qianqian-audio-api",
        "qianqian-composition",
    },  # provider-agnostic session (D6)
    "qianqian-decode-songcore": {
        "qianqian-audio-api",
        "qianqian-composition",
        "qianqian-songcore-sys",
    },
    "qianqian-output-wasapi": {"qianqian-audio-api", "qianqian-composition"},
    # The composition root: explicit Plugin-constructor admission only.
    # A future provider MUST update this allowlist on purpose (MAJOR-2).
    "qianqian-headless": {
        "qianqian-app",
        "qianqian-composition",
        "qianqian-playback",
        "qianqian-decode-songcore",  # optional, feature = playback
        "qianqian-output-wasapi",  # optional, feature = playback
    },
    "qianqian-songcore-sys": set(),
}

DEV_EDGES = {
    "qianqian-playback": {"qianqian-app"},  # admission tests
    "qianqian-decode-songcore": {"qianqian-app"},  # admission tests
    "qianqian-output-wasapi": {"qianqian-app"},  # admission tests
    "qianqian-headless": {"qianqian-audio-api"},  # H-b: TEST ADMISSION ONLY
}

BUILD_EDGES = {}  # no internal build edges are admitted for any crate

# ---------------------------------------------------------------------------
# The U2 (Issue #166 §49) architecture negative control, as one shared
# list: the temporary-playlist policy and the whole TUI shell must not
# name the composition kernel, a concrete provider/mechanism crate, the
# PCM data plane or the K0 representation type. `wasapi` /
# `qianqian_output_wasapi` / `qianqian_decode_songcore` / `songcore`
# cover the backend and decode mechanisms; `PcmEdge` /
# `DecodedPcmStream` / `RenderPcmInput` / `ComponentSpec` cover the data
# plane and the K0 noun. Deliberately NOT a vocabulary denylist: prose
# about "no Capability" / "no Fact" is legitimate in a module that
# disclaims them, so only code-level dependency spellings are scanned.
U2_SHELL_FORBIDDEN = [
    "wasapi",
    "Wasapi",
    "WASAPI",
    # The PCM data plane and the mechanism vocabulary around it. The set
    # is deliberately wider than the spellings the campaign named: an
    # import of any of these into the shell would be the same boundary
    # violation whichever name it arrived under.
    "PcmEdge",
    "DecodedPcmStream",
    "RenderPcmInput",
    "RenderRequest",
    "RenderStream",
    "GateSlice",
    "RenderGate",
    "ParkOutcome",
    "TailProbeOutcome",
    "PcmDecode",
    "AudioOutput",
    "songcore",
    "SongCore",
    # K0 composition identity.
    "ComponentSpec",
    "qianqian_output_wasapi",
    "qianqian_decode_songcore",
    "qianqian_composition",
    "qianqian_app",
]

U2_SHELL_AUTHORITY = (
    "Issue #166 §49 architecture negative control + ADR-PBK-002 D14.6 as amended by the "
    "2026-09-19 playlist/repeat/order product amendment — the temporary playlist module "
    "and the TUI shell's model/view/runtime modules (the files listed above; tui/mod.rs "
    "is a module index that only names these mechanisms to disclaim them) are ordinary "
    "App-state policy and presentation: they own no K0 composition identity, hold no "
    "Fact, and reach neither a provider mechanism nor the PCM data plane. Their only "
    "domain vocabulary is the F2 episode seam's read side (qianqian_playback's "
    "observation/handle types) and, in dev/test code, the shared PcmFormat contract"
)

# Export-surface rules. Two mechanisms, both source-class:
#
#   require / forbid        literal snippets in one lib.rs (decode keeps
#                           these; its stricter per-src-file scan below
#                           makes the forbids belt-and-braces)
#   allowed_root_public     the rule's file may declare EXACTLY these
#                           externally-visible `pub` statements — no more
#                           (unexpected surface) and no less (required
#                           surface missing). Forbidding known spellings
#                           is not enforcement: any NEW public seam
#                           (e.g. `pub fn direct_output()` handing out the
#                           mechanism service) must RED and force an
#                           explicit architecture decision, same
#                           philosophy as the Cargo edge allowlist.
#                           Statements are whitespace-canonicalized, so
#                           rustfmt reflow does not false-RED; any
#                           semantic change of the statement does.
#                           Optional "label" names the surface in
#                           violation output (defaults to "root").
#
# For playback this is TWO levels, on purpose: lib.rs freezes which names
# leave the crate, and handle.rs freezes the RIGHTS those names carry —
# the exported episode handle must grant the App exactly the D14.2
# rights (request_stop / observe / wait_terminal + the observation
# fields). A future `pub fn pause()` on the handle, or a new pub field
# on PlaybackSessionObservation, compiles fine and changes no Cargo
# edge — only this rule turns it into an explicit architecture event.

EXPORT_RULES = {
    "crates/qianqian-playback/src/lib.rs": {
        "allowed_root_public": [
            "pub use handle::{\n    EpisodeTerminalOutcome, PauseEngagement, PlaybackSessionHandle, PlaybackSessionObservation,\n};",
            "pub use session::{playback_session_spec, playback_session_spec_with_processing};",
            "pub use presets::EqPreset;",
            "pub use processing::{AudioProcessingConfig, EqConfig};",
            "pub use headroom::{HeadroomGuidance, estimated_eq_headroom_guidance};",
        ],
        "authority": "ADR-PBK-002 D6/D14.2/D14.3/D14.7 — the admitted public surface is exactly the F2 episode seam (extended by the D14.7 F3 pause fields/commands, whose spelling is representation); the episode mechanism is session-owned, not product API. Extended 2026-10-02 (Issue #177 I1, D14.11): the desired Audio Processing configuration's establishment handoff — the `AudioProcessingConfig` payload and the `playback_session_spec_with_processing` constructor — is the application/product-control owner's one product seam for it; the processing runtime itself stays session-owned and crate-private. Extended 2026-10-02 (Issue #177 I4, D14.11): `presets::EqPreset` joins as pure configuration DATA (the application must be able to name a desired configuration); it carries no processor/Plugin identity and no live-update right. Extended 2026-10-02 (Issue #190 D2, dsp-product-model.md §9.1): `headroom::{HeadroomGuidance, estimated_eq_headroom_guidance}` joins as the NON-BINDING headroom advisory — a deterministic pure analysis over the desired EQ data and the source rate (estimated steady-state cascade guidance, never a clipping guarantee); it grants no live-update right, carries no processor/Plugin identity, and never mutates the desired configuration",
    },
    # The processing-config freeze behind the lib.rs re-export (I1 review):
    # the desired-configuration payload's public rights are frozen here so
    # they cannot grow silently (a `pub preset` field or a live-update
    # accessor would otherwise RED nothing — live parameter update is
    # OPEN/unearned under D14.11). The episode-owned runtime
    # (EpisodeProcessing) is pub(crate) and is NOT admitted: promoting it
    # to `pub` REDs this rule.
    "crates/qianqian-playback/src/processing.rs": {
        "label": "audio-processing-config",
        # Raw-text requires: the canonicalizer truncates `;`-containing
        # declarations at the array's `;`, so the band-table ARITY and the
        # constructor's parameter list are pinned here as literal text.
        "require": [
            "pub band_gain_db: [f32; 10],",
            "pub fn new(band_gain_db: [f32; 10], q: f32) -> Self {",
        ],
        "allowed_root_public": [
            "pub struct AudioProcessingConfig {",
            "pub enabled: bool,",
            "pub gain: f32,",
            "pub eq: Option<EqConfig>,",
            "pub const BYPASS: Self = Self {",
            "pub fn gain(factor: f32) -> Self {",
            "pub fn eq(eq: EqConfig) -> Self {",
            "pub struct EqConfig {",
            # The array's `;` and the multi-parameter signature cut at
            # their deterministic terminators (see root_public_declarations).
            "pub band_gain_db: [f32;",
            "pub q: f32,",
            "pub const FLAT: Self = Self {",
            "pub fn new(band_gain_db: [f32;",
            # Canonical cut: the generic comma in `Result<(), String>` sits
            # at paren depth 0 (the empty tuple closed before it), so the
            # declaration canonicalizes truncated there — any signature
            # change still produces a NEW canonical and REDs.
            "pub fn validate(&self) -> Result<(),",
        ],
        "authority": "ADR-PBK-002 D14.11 (Issue #177 I1-I3) — the application/product-control "
        "layer owns the DESIRED Audio Processing configuration and hands ONE coherent "
        "snapshot to the episode at establishment (config model case B; live parameter "
        "update stays OPEN and unearned, so no update accessor may appear on this "
        "surface; presets are I4 configuration DATA and join only as data, never as "
        "processors). The applied snapshot and the processing runtime are session-owned "
        "subordinate resources and stay crate-private. A new public right here must "
        "first earn its narrow authority, then update this allowlist on purpose",
    },
    # The rights freeze behind the lib.rs re-export (I4 review): the
    # preset vocabulary is configuration DATA (D14.11), and its public
    # rights are frozen here so a future `pub fn apply_live(...)`-shaped
    # accessor — live parameter update is OPEN/unearned — or any
    # processor/registry vocabulary cannot grow silently. A new public
    # right here REDs until it earns narrow authority.
    "crates/qianqian-playback/src/presets.rs": {
        "label": "eq-preset-data",
        # The `;`-containing array signatures cut at their deterministic
        # `;` (see root_public_declarations), exactly like the processing
        # rule's raw-text requires.
        "allowed_root_public": [
            "pub enum EqPreset {",
            "pub fn band_gain_db(self) -> [f32;",
            "pub fn to_config(self) -> AudioProcessingConfig {",
            "pub fn from_name(name: &str) -> Option<Self> {",
            "pub fn name(self) -> &'static str {",
            "pub fn all() -> [Self;",
        ],
        "authority": "ADR-PBK-002 D14.11 (Issue #177 I4) — named EQ presets are pure "
        "product CONFIGURATION DATA over AudioProcessingConfig/EqConfig: record, "
        "resolve deterministically (case B desired configuration), and parse by "
        "name. No processor, Plugin, registry, or live-update identity may appear "
        "on this surface; a preset change reaching a live episode is unearned and "
        "must RED here until a narrow authority amendment says otherwise",
    },
    # The rights freeze behind the lib.rs re-exports (review round 3):
    # an exported type's pub methods/fields live here, not in lib.rs, so
    # the crate-root allowlist alone cannot see them grow. pub(crate)
    # items (completion) are not external surface and are NOT admitted;
    # promoting one to `pub` REDs this rule.
    "crates/qianqian-playback/src/handle.rs": {
        "label": "episode-handle",
        "allowed_root_public": [
            "pub enum EpisodeTerminalOutcome {",
            "pub enum PauseEngagement {",
            "pub struct PlaybackSessionHandle {",
            "pub struct PlaybackSessionObservation {",
            "pub terminal_outcome: Option<EpisodeTerminalOutcome>,",
            "pub failure_diagnostic: Option<String>,",
            "pub stop_requested: bool,",
            "pub pause_requested: bool,",
            "pub source_format: Option<PcmFormat>,",
            "pub source_duration: Option<Duration>,",
            "pub position: Option<u64>,",
            "pub pause_engagement: PauseEngagement,",
            "pub activation_error: Option<String>,",
            "pub fn new() -> Self {",
            "pub fn request_stop(&self) {",
            "pub fn request_pause(&self) {",
            "pub fn request_resume(&self) {",
            "pub fn request_seek(&self, target: Duration) {",
            "pub fn request_output_level(&self, level: u8) {",
            "pub fn observe(&self) -> PlaybackSessionObservation {",
            "pub fn wait_terminal(&self) -> EpisodeTerminalOutcome {",
            "pub fn paused(&self) -> bool {",
        ],
        "authority": "ADR-PBK-002 D14.2 + the D14.7 F3 amendment as narrowed by the D14.7 "
        "AUTHORITY-CORRECTIVE + the D14.8 F4 amendment + the D14.5 F5 amendment + the D14.9 "
        "F6 promotion and its 2026-09-19 VOLUME-IMPLEMENTATION-1 grounding — the App's "
        "rights over one episode are exactly "
        "new/request_stop/request_pause/request_resume/request_seek/request_output_level/"
        "observe/wait_terminal "
        "plus the admitted observation fields (pause intent is command state; engagement/"
        "tail-quiescence are mechanism evidence; paused() is a derived Projection and "
        "never a correctness basis; Resumed was REMOVED as an application-facing "
        "projection — disengagement evidence cannot prove a viable render leg remains — "
        "so resume is command-only and no disengagement latch is public surface). F4 adds "
        "exactly two fields: `position` (the D14.8 Projection — ONE loaded sample in "
        "source PCM frames, so no handed-off total, no device tail, no raw estimate and no "
        "second cell may appear on this surface: a reader must not be able to reconstruct "
        "device state) and `source_duration` (optional source-scoped Mechanism Evidence). "
        "F5 adds exactly one command: `request_seek` (D14.5) — an infallible one-way "
        "Command whose acceptance is NOT a cutover; the observable consequences are the "
        "Position rebase (to the decoder's ACTUAL landing, or withdrawn for an unknown "
        "landing) and the ordinary D11 Failed route for a destructive provider failure — "
        "so no positive seek state, no seek completion Fact, no request identity and no "
        "landing/outcome accessor may appear on this surface. D14.9 (as grounded by "
        "V-PROBE) adds exactly one command: `request_output_level(0..=100)` — the App's "
        "desired stream factor as an idempotent, non-terminal Command (clamped at the "
        "seam; routed into the session-owned OutputLevel cell; never a Fact, never a "
        "mechanism readback — no volume getter may appear on this surface). A further new "
        "public right must first earn an explicit D14/phase-authority amendment, then "
        "update this allowlist on purpose",
    },
    "crates/qianqian-decode-songcore/src/lib.rs": {
        "require": [
            "pub fn songcore_decode_plugin",
            "pub fn probe_media(path: &Path) -> Result<SourceFacts, DecodeOpenError> {",
        ],
        "forbid": [
            "pub struct SongcoreDecode",
            "pub use",
        ],
        "authority": "ADR-PBK-002 D5/D7 + the D14.6 F6-AUTHORITY-PROMOTION-1 amendment — admitted surface is the plugin constructor plus exactly ONE public stateless source-preflight query (probe_media: open → declared facts → close; no PCM read, no RT resource, mechanism evidence for an Open composition decision, never episode truth); the concrete mechanism stays crate-private (SOURCE_GATE_ENFORCED: workspace-excluded crate)",
    },
    "crates/qianqian-output-wasapi/src/lib.rs": {
        "allowed_root_public": [
            "pub fn output_plugin() -> ComponentSpec {",
        ],
        "authority": "ADR-PBK-002 D5/D7 as amended by ADR-PBK-003 §2/§3/§11 — the admitted public surface is exactly the STABLE Output Plugin constructor (the owned WASAPI Host Render Backend stays crate-private; consumers reach the mechanism only as the AudioOutput capability service; backend brand must not reappear in the composition identity)",
    },
    # U2 (Issue #166) presentation/product-policy modules. A Cargo edge
    # cannot express an intra-crate firewall, and these four modules are
    # where the playlist policy and the shell live: they may speak the
    # F2 episode seam vocabulary (qianqian_playback's read types) and
    # nothing of the composition kernel, the providers or the PCM
    # mechanism. The rule is a source scan because the property is
    # module-local; the same strings are checked by the U2 negative
    # controls below, which are what prove the scan is not vacuous.
    "apps/headless/src/playlist.rs": {
        "forbid": U2_SHELL_FORBIDDEN,
        "authority": U2_SHELL_AUTHORITY,
    },
    "apps/headless/src/tui/model.rs": {
        "forbid": U2_SHELL_FORBIDDEN,
        "authority": U2_SHELL_AUTHORITY,
    },
    "apps/headless/src/tui/view.rs": {
        "forbid": U2_SHELL_FORBIDDEN,
        "authority": U2_SHELL_AUTHORITY,
    },
    "apps/headless/src/tui/runtime.rs": {
        "forbid": U2_SHELL_FORBIDDEN,
        "authority": U2_SHELL_AUTHORITY,
    },
    # `apps/headless/src/tui/mod.rs` is deliberately NOT scanned: it is a
    # module-index document whose text NAMES these mechanisms only to
    # disclaim them ("never sees … PcmEdge …"), and a substring rule
    # cannot tell a disclaimer from a dependency. The three modules that
    # hold the shell's actual code are covered.
}

# Human-facing rule prose for violation output (mission §21 format).
EDGE_RULE_PROSE = {
    "qianqian-composition": "K0 depends on no domain crate",
    "qianqian-audio-api": "shared contracts may depend on composition only",
    "qianqian-app": "the App abstraction may depend on composition only",
    "qianqian-playback": "playback may depend on audio-api and composition only",
    "qianqian-decode-songcore": "decode provider may depend on audio-api, composition and songcore-sys only",
    "qianqian-output-wasapi": "the stable Output Plugin provider may depend on audio-api and composition only (its Host Render Backend is an owned mechanism, ADR-PBK-003)",
    "qianqian-headless": "composition root admits app, composition, playback and explicitly admitted providers only",
    "qianqian-songcore-sys": "the sys binding crate depends on no qianqian crate",
}


def fail(message):
    print(f"PLUGIN_BOUNDARY_VIOLATION\n{message}")
    sys.exit(1)


def tooling_fail(message):
    print(f"TOOLING-FAIL: {message}")
    sys.exit(2)


def cargo_metadata(manifest=None):
    cmd = ["cargo", "metadata", "--no-deps", "--format-version", "1"]
    if manifest:
        cmd += ["--manifest-path", str(ROOT / manifest)]
    try:
        out = subprocess.run(cmd, cwd=ROOT, capture_output=True, text=True)
    except FileNotFoundError:
        tooling_fail("cargo not found")
    if out.returncode != 0:
        tooling_fail(f"cargo metadata failed for {manifest or 'workspace'}:\n{out.stderr.strip()}")
    try:
        return json.loads(out.stdout)
    except json.JSONDecodeError as error:
        tooling_fail(
            f"cargo metadata returned invalid JSON for {manifest or 'workspace'}: {error}"
        )


def internal_deps(package):
    """Yield (target, kind, flags) for every qianqian-* dependency."""
    for dep in package.get("dependencies", []):
        name = dep["name"]
        if not name.startswith(INTERNAL_PREFIX):
            continue  # external crates are outside this architecture policy
        kind = dep.get("kind") or "normal"
        flags = []
        if dep.get("optional"):
            flags.append("optional")
        if dep.get("target"):
            flags.append(f"target:{dep['target']}")
        yield name, kind, flags


def root_public_declarations(text):
    """Canonical forms of every externally-visible `pub` declaration in
    a rule's file: statements starting with `pub ` (pub(crate)/pub(super)
    are not external surface), assembled until their terminator,
    whitespace-stripped so rustfmt reflow does not false-RED. A `pub use`
    statement ends at `;` — its brace list (`use p::{A, B}`) is payload,
    not a block opener. Every other kind ends at the first `;`, `{` or
    `,`: the comma keeps struct fields (`pub stop_requested: bool,`) and
    multi-parameter signatures as one deterministic canonical each, so
    any change to a frozen declaration — a new field, a new parameter, a
    changed return type — produces a NEW canonical and REDs. An exotic
    signature containing an earlier `;`/`{` (e.g.
    `pub fn f(x: [u8; 4])`) splits into unmatched fragments — a false
    RED, i.e. fail-closed.
    """
    declarations = []
    buffer = None
    use_stmt = False
    for raw in text.splitlines():
        line = raw.strip()
        if buffer is None:
            if line == "pub" or line.startswith("pub "):
                buffer = line
                tokens = line.split()
                use_stmt = len(tokens) >= 2 and tokens[1] == "use"
            else:
                continue
        else:
            buffer += " " + line
        # The tail of `buffer` is `line`: process its terminator now.
        if use_stmt:
            if ";" in line:
                cut = buffer.find(";")
                declarations.append("".join(buffer[: cut + 1].split()))
                buffer, use_stmt = None, False
            continue
        positions = [
            i for i in (buffer.find(";"), buffer.find("{")) if i != -1
        ]
        # A comma cuts only OUTSIDE parentheses: a struct field ends at
        # its trailing comma, but a comma inside a parameter list (e.g.
        # `pub fn seek(&self, target: Duration)`) is part of one
        # deterministic canonical — the signature's terminator is the
        # brace that follows.
        depth = 0
        for idx, ch in enumerate(buffer):
            if ch == "(":
                depth += 1
            elif ch == ")":
                depth = max(0, depth - 1)
            elif ch == "," and depth == 0:
                positions.append(idx)
                break
        if positions:
            cut = min(positions)
            declarations.append("".join(buffer[: cut + 1].split()))
            buffer = None
    if buffer is not None:
        declarations.append("".join(buffer.split()) + "<unterminated>")
    return declarations


def scan():
    violations = []

    # Workspace exclusion is never architectural exclusion: every
    # qianqian-* crate listed in [workspace] exclude must be explicitly
    # known to this gate (audit L7 / §7.5). Non-qianqian paths in the
    # exclude list (experiments/) are outside the production universe.
    root_manifest = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))
    admitted_excluded = {Path(p).name for p in WORKSPACE_EXCLUDED_PRODUCTION}
    for entry in root_manifest.get("workspace", {}).get("exclude", []):
        name = Path(entry).name
        if name.startswith(INTERNAL_PREFIX) and name not in admitted_excluded:
            violations.append(
                f"source: <workspace exclude>\ntarget: {name}\nkind: workspace-exclude\n"
                f"rule: a qianqian-* crate excluded from the workspace must be explicitly "
                f"admitted to the architecture universe (update WORKSPACE_EXCLUDED_PRODUCTION on purpose)\n"
                f"authority: audit §7.5 / L7 — workspace exclusion never means architectural exclusion"
            )

    universe = {}
    root = cargo_metadata()
    for pkg in root["packages"]:
        universe[pkg["name"]] = pkg
    for manifest in WORKSPACE_EXCLUDED_PRODUCTION:
        meta = cargo_metadata(f"{manifest}/Cargo.toml")
        for pkg in meta["packages"]:
            if pkg["name"] not in universe:
                universe[pkg["name"]] = pkg

    # Universe-level allowlist: every discovered internal crate must have
    # an explicit rule entry, and every rule entry must exist in reality.
    for name in sorted(universe):
        if name not in NORMAL_EDGES:
            violations.append(
                f"source: <workspace/universe>\ntarget: {name}\nkind: crate\n"
                f"rule: every internal crate must be explicitly admitted to the architecture\n"
                f"authority: audit §7.2 MAJOR-2 (allowlist-first)"
            )
    for name in sorted(NORMAL_EDGES):
        if name not in universe:
            violations.append(
                f"source: <rules>\ntarget: {name}\nkind: crate\n"
                f"rule: an admission rule names a crate that does not exist (stale rule)\n"
                f"authority: rules must match repository reality"
            )

    for name, pkg in sorted(universe.items()):
        for target, kind, flags in internal_deps(pkg):
            table = {"normal": NORMAL_EDGES, "dev": DEV_EDGES, "build": BUILD_EDGES}[kind]
            allowed = table.get(name)
            if allowed is None:
                violations.append(
                    f"source: {name}\ntarget: {target}\nkind: {kind}\n"
                    f"rule: no admitted {kind} edges for this crate (fail-closed)\n"
                    f"authority: audit §7.2 allowlist-first"
                )
            elif target not in allowed:
                violations.append(
                    f"source: {name}\ntarget: {target}\nkind: {kind}"
                    + (f" ({', '.join(flags)})" if flags else "")
                    + "\nrule: "
                    + EDGE_RULE_PROSE.get(name, "edge not explicitly admitted")
                    + f"\nauthority: ADR-PBK-002 D3/D5/D6/D7/D8"
                )

    # Export-surface rules (source class; see module docstring).
    for rel, rule in EXPORT_RULES.items():
        path = ROOT / rel
        text = path.read_text(encoding="utf-8")
        for snippet in rule.get("require", []):
            if snippet not in text:
                violations.append(
                    f"source: {rel}\ntarget: public export surface\nkind: source\n"
                    f"rule: required public surface missing: {snippet!r}\n"
                    f"authority: {rule['authority']}"
                )
        for snippet in rule.get("forbid", []):
            if snippet in text:
                violations.append(
                    f"source: {rel}\ntarget: public export surface\nkind: source\n"
                    f"rule: forbidden public mechanism exposure: {snippet!r}\n"
                    f"authority: {rule['authority']}"
                )
        if "allowed_root_public" in rule:
            noun = rule.get("label", "root")
            found = root_public_declarations(text)
            allowed = {"".join(a.split()) for a in rule["allowed_root_public"]}
            for canonical in found:
                if canonical not in allowed:
                    violations.append(
                        f"source: {rel}\ntarget: public export surface\nkind: source\n"
                        f"rule: unexpected {noun} public surface: {canonical!r}\n"
                        f"admitted surface: {sorted(allowed)}\n"
                        f"a new public product seam is an explicit architecture "
                        f"decision: update this allowlist on purpose\n"
                        f"authority: {rule['authority']}"
                    )
            for admitted in sorted(allowed):
                if admitted not in found:
                    violations.append(
                        f"source: {rel}\ntarget: public export surface\nkind: source\n"
                        f"rule: admitted {noun} public surface missing: {admitted!r}\n"
                        f"authority: {rule['authority']}"
                    )

    # Decode chain strict source scan (the chain is workspace-excluded
    # and no CI compiles it, so this source gate is its only continuous
    # watcher — it must not be defeatable by a new src file). Across
    # EVERY src/*.rs of the excluded decode crate, the only externally
    # visible items allowed are the admitted plugin constructor and the
    # D14.6 public stateless source-preflight query (exactly its frozen
    # spelling: SourceFacts in, DecodeOpenError out); everything else
    # must be private or pub(crate). lib.rs-level "pub mod" re-opening
    # is already forbidden by the EXPORT_RULES forbid list above; this
    # scan covers items declared in other files.
    decode_src = ROOT / "crates/qianqian-decode-songcore" / "src"
    admitted_decode_surface = {
        "pub fn songcore_decode_plugin() -> ComponentSpec {",
        "pub fn probe_media(path: &Path) -> Result<SourceFacts, DecodeOpenError> {",
    }
    for rs in sorted(decode_src.glob("*.rs")):
        for lineno, line in enumerate(rs.read_text(encoding="utf-8").splitlines(), 1):
            stripped = line.strip()
            if not stripped.startswith("pub "):
                continue  # private / pub(crate) / comments / strings
            if rs.name == "lib.rs" and stripped in admitted_decode_surface:
                continue
            violations.append(
                f"source: crates/qianqian-decode-songcore/src/{rs.name}:{lineno}\n"
                f"target: public export surface\nkind: source\n"
                f"rule: the decode chain admits exactly the plugin constructor and "
                f"the D14.6 probe query; found: {stripped.split('{')[0].strip()!r}\n"
                f"authority: audit §7.5 / MAJOR-3 — SOURCE_GATE_ENFORCED for the "
                f"workspace-excluded decode chain; D14.6 F6-AUTHORITY-PROMOTION-1 "
                f"(the narrow public-surface amendment, synced with the surface)"
            )

    return violations


def run_gate():
    violations = scan()
    if violations:
        for v in violations:
            print("PLUGIN_BOUNDARY_VIOLATION")
            print(v)
            print()
        print(f"SUITE: FAILED ({len(violations)} violation(s))")
        sys.exit(1)
    print("PLUGIN_BOUNDARY_GATE: PASS")
    print("SUITE: PASS (topology allowlist + export surface clean)")


# ---------------------------------------------------------------------------
# Negative controls: prove the gate is not vacuous. Each control mutates
# the real tree, expects a specific failure, restores byte-exactly, and
# verifies the baseline is green again. Refuses to run on a dirty tree
# (same discipline as specs/playback-concurrency/check.sh).

MUTABLE_FILES = [
    "crates/qianqian-playback/Cargo.toml",
    "crates/qianqian-playback/src/lib.rs",
    "crates/qianqian-playback/src/handle.rs",
    "crates/qianqian-decode-songcore/Cargo.toml",
    "crates/qianqian-decode-songcore/src/lib.rs",
    "crates/qianqian-output-wasapi/src/lib.rs",
    "apps/headless/Cargo.toml",
    "apps/headless/src/playlist.rs",
    "apps/headless/src/tui/model.rs",
    "apps/headless/src/tui/view.rs",
    "apps/headless/src/tui/runtime.rs",
    "Cargo.toml",
]


def git_dirty(path):
    out = subprocess.run(
        ["git", "status", "--porcelain", "--", str(ROOT / path)],
        cwd=ROOT, capture_output=True, text=True,
    )
    if out.returncode != 0:
        # fail-closed: a failed git status is NOT evidence of a clean tree
        tooling_fail(f"git status failed for {path}:\n{out.stderr.strip()}")
    return bool(out.stdout.strip())


def expect_fail(label, expected_fragment, edits=None, creates=None):
    """Apply reversible mutations, run the scan, expect exit 1 with a
    signature. `edits` transforms existing files (snapshotted and
    restored byte-exactly); `creates` writes new files (deleted after).
    """
    edits = edits or {}
    creates = creates or {}
    for rel in creates:
        if (ROOT / rel).exists():
            tooling_fail(f"{rel} already exists; refusing to run negative controls")
    snapshots = {}
    for rel, transform in edits.items():
        p = ROOT / rel
        snapshots[rel] = p.read_text(encoding="utf-8")
        p.write_text(transform(snapshots[rel]), encoding="utf-8")
    for rel, content in creates.items():
        (ROOT / rel).parent.mkdir(parents=True, exist_ok=True)
        (ROOT / rel).write_text(content, encoding="utf-8")
    try:
        violations = scan()
        captured = "\n".join(violations)
    finally:
        for rel, original in snapshots.items():
            (ROOT / rel).write_text(original, encoding="utf-8")
        for rel in creates:
            (ROOT / rel).unlink(missing_ok=True)
        # prune directories created for the probe files, deepest first
        dirs = sorted({(ROOT / rel).parent for rel in creates},
                      key=lambda d: len(d.parts), reverse=True)
        for d in dirs:
            try:
                d.rmdir()
            except OSError:
                pass
    for rel, original in snapshots.items():
        if (ROOT / rel).read_text(encoding="utf-8") != original:
            tooling_fail(f"{label}: {rel} did not restore byte-exactly")
    if not violations:
        print(f"RESULT {label} TOOLING-FAIL (mutation was NOT caught)")
        sys.exit(1)
    if expected_fragment not in captured:
        print(f"RESULT {label} TOOLING-FAIL (failed, but not with the expected signature)")
        sys.exit(1)
    print(f"RESULT {label} RED (caught: {expected_fragment.splitlines()[0]})")


def append_dependency(text, section, line):
    marker = f"[{section}]"
    idx = text.index(marker) + len(marker)
    return text[:idx] + "\n" + line + text[idx:]


def run_negative_controls():
    for rel in MUTABLE_FILES:
        if git_dirty(rel):
            tooling_fail(f"{rel} is dirty; refusing to run negative controls")

    expect_fail(
        "M3 playback->output-wasapi",
        "target: qianqian-output-wasapi",
        {
            "crates/qianqian-playback/Cargo.toml": lambda t: append_dependency(
                t, "dependencies", 'qianqian-output-wasapi = { path = "../qianqian-output-wasapi" }'
            )
        },
    )
    expect_fail(
        "M4 decode->output-wasapi",
        "target: qianqian-output-wasapi",
        {
            "crates/qianqian-decode-songcore/Cargo.toml": lambda t: append_dependency(
                t, "dependencies", 'qianqian-output-wasapi = { path = "../qianqian-output-wasapi" }'
            )
        },
    )
    expect_fail(
        "M5b headless->audio-api (normal)",
        "target: qianqian-audio-api",
        {
            "apps/headless/Cargo.toml": lambda t: append_dependency(
                t, "dependencies", 'qianqian-audio-api = { path = "../../crates/qianqian-audio-api" }'
            )
        },
    )
    expect_fail(
        "unknown internal crate (universe allowlist)",
        "target: qianqian-boundary-probe",
        edits={
            "Cargo.toml": lambda t: t.replace(
                '    "crates/qianqian-playback",', '    "crates/qianqian-playback",\n    "crates/qianqian-boundary-probe",'
            ),
        },
        # The crate must exist for cargo metadata to load the workspace;
        # the gate must then reject it for having no admission rule.
        creates={
            "crates/qianqian-boundary-probe/Cargo.toml": (
                '[package]\nname = "qianqian-boundary-probe"\nversion = "0.1.0"\nedition = "2021"\n'
            ),
            "crates/qianqian-boundary-probe/src/lib.rs": "",
        },
    )
    expect_fail(
        "M1-source decode mechanism re-published",
        "forbidden public mechanism exposure: 'pub struct SongcoreDecode'",
        {
            "crates/qianqian-decode-songcore/src/lib.rs": lambda t: t.replace(
                "struct SongcoreDecode {", "pub struct SongcoreDecode {", 1
            )
        },
    )
    expect_fail(
        "M5a-source playback edge re-exported",
        "unexpected root public surface: 'pubuseedge::{PcmEdge,WriteOutcome};'",
        {
            "crates/qianqian-playback/src/lib.rs": lambda t: t.replace(
                "pub use handle::", "pub use edge::{PcmEdge, WriteOutcome};\npub use handle::", 1
            )
        },
    )
    expect_fail(
        "M2-source output mechanism re-exported",
        "unexpected root public surface: 'pubusewasapi::WasapiOutput;'",
        {
            "crates/qianqian-output-wasapi/src/lib.rs": lambda t: t.replace(
                "fn selected_backend() -> Result<Rc<dyn qianqian_audio_api::ports::AudioOutput>, String> {",
                "pub use wasapi::WasapiOutput;\n\nfn selected_backend() -> Result<Rc<dyn qianqian_audio_api::ports::AudioOutput>, String> {",
                1,
            )
        },
    )
    # M7 — an UNKNOWN new root public seam (no known mechanism spelling,
    # no re-export): the surface is frozen, so a new pub API must RED on
    # its own and force an explicit architecture decision.
    expect_fail(
        "M7-output unexpected public seam",
        "unexpected root public surface: 'pubfnboundary_escape_probe(){'",
        {
            "crates/qianqian-output-wasapi/src/lib.rs": lambda t: (
                t + "\npub fn boundary_escape_probe() {}\n"
            )
        },
    )
    expect_fail(
        "M7-playback unexpected public seam",
        "unexpected root public surface: 'pubfnboundary_escape_probe(){'",
        {
            "crates/qianqian-playback/src/lib.rs": lambda t: (
                t + "\npub fn boundary_escape_probe() {}\n"
            )
        },
    )
    # M8 — handle-RIGHT expansion (review round 3): the lib.rs allowlist
    # froze which names leave the crate; the handle.rs rule freezes what
    # those names can DO. A new pub method on the episode handle
    # compiles, changes no Cargo edge and adds no root export — the
    # episode-handle allowlist must RED on it alone (D14.2: exactly
    # request_stop / observe / wait_terminal).
    expect_fail(
        "M8-playback episode-handle right expansion",
        "unexpected episode-handle public surface: 'pubfnboundary_escape_probe(&self){'",
        {
            "crates/qianqian-playback/src/handle.rs": lambda t: (
                t + "\nimpl PlaybackSessionHandle {\n"
                "    pub fn boundary_escape_probe(&self) {}\n"
                "}\n"
            )
        },
    )
    # M8b — the observation surface is frozen at FIELD granularity: the
    # App reads exactly the D14.2/D14.7 fields and nothing else.
    expect_fail(
        "M8b observation field expansion",
        "unexpected episode-handle public surface: 'pubboundary_escape_probe:bool,'",
        {
            "crates/qianqian-playback/src/handle.rs": lambda t: t.replace(
                "pub struct PlaybackSessionObservation {",
                "pub struct PlaybackSessionObservation {\n"
                "    pub boundary_escape_probe: bool,",
                1,
            )
        },
    )
    # M8c — the D14.7 AUTHORITY-CORRECTIVE removed the Resumed product
    # projection and the public disengagement latch: re-adding either
    # spelling (the exact retired statements) must RED on its own and
    # force a fresh authority decision, not slip through as "just
    # another field".
    expect_fail(
        "M8c retired Resumed projection re-published",
        "unexpected episode-handle public surface: 'pubfnresumed(&self)->bool{'",
        {
            "crates/qianqian-playback/src/handle.rs": lambda t: t.replace(
                "impl PlaybackSessionObservation {",
                "impl PlaybackSessionObservation {\n"
                "    pub fn resumed(&self) -> bool {\n"
                "        false\n"
                "    }\n",
                1,
            )
        },
    )
    expect_fail(
        "M8c retired disengagement latch re-published",
        "unexpected episode-handle public surface: 'pubpause_disengaged_observed:bool,'",
        {
            "crates/qianqian-playback/src/handle.rs": lambda t: t.replace(
                "pub struct PlaybackSessionObservation {",
                "pub struct PlaybackSessionObservation {\n"
                "    pub pause_disengaged_observed: bool,",
                1,
            )
        },
    )
    expect_fail(
        "workspace-exclude without universe admission",
        "kind: workspace-exclude",
        edits={
            "Cargo.toml": lambda t: t.replace(
                'exclude = [\n    "crates/qianqian-decode-songcore",',
                'exclude = [\n    "crates/qianqian-probe-exclude",\n    "crates/qianqian-decode-songcore",',
            ),
        },
    )
    expect_fail(
        "decode chain pub item in a new src file",
        "source: crates/qianqian-decode-songcore/src/boundary_probe_helper.rs",
        creates={
            "crates/qianqian-decode-songcore/src/boundary_probe_helper.rs": (
                "pub struct BoundaryProbeMechanism;\n"
            ),
        },
    )
    # M9 — the U2 presentation/product-policy firewall (Issue #166 §49):
    # reaching for a concrete provider, the K0 representation type or the
    # PCM data plane from the playlist or the shell must RED on the
    # import spelling alone (a Cargo edge cannot express an intra-crate
    # module boundary, which is exactly why this rule is a source scan).
    expect_fail(
        "M9-playlist provider-mechanism import",
        "source: apps/headless/src/playlist.rs",
        {
            "apps/headless/src/playlist.rs": lambda t: t.replace(
                "use std::path::{Path, PathBuf};",
                "use std::path::{Path, PathBuf};\n"
                "use qianqian_output_wasapi::wasapi::WasapiOutput;",
                1,
            )
        },
    )
    expect_fail(
        "M9-tui-model PCM data-plane import",
        "source: apps/headless/src/tui/model.rs",
        {
            "apps/headless/src/tui/model.rs": lambda t: t.replace(
                "use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};",
                "use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};\n"
                "use qianqian_audio_api::ports::DecodedPcmStream;",
                1,
            )
        },
    )
    expect_fail(
        "M9-tui-view K0-representation import",
        "source: apps/headless/src/tui/view.rs",
        {
            "apps/headless/src/tui/view.rs": lambda t: t.replace(
                "use ratatui::Frame;",
                "use qianqian_composition::ComponentSpec;\nuse ratatui::Frame;",
                1,
            )
        },
    )
    expect_fail(
        "M9-tui-runtime PCM data-plane import",
        "source: apps/headless/src/tui/runtime.rs",
        {
            "apps/headless/src/tui/runtime.rs": lambda t: t.replace(
                "use ratatui::Terminal;",
                "use qianqian_audio_api::ports::RenderRequest;\nuse ratatui::Terminal;",
                1,
            )
        },
    )

    violations = scan()
    if violations:
        tooling_fail("baseline is not green after controls")
    print("SUITE: NEGATIVE-CONTROLS-PASS (all mutations caught; baseline green)")


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument(
        "--negative-controls",
        action="store_true",
        help="reversibly mutate the tree to prove the gate turns red (refuses a dirty tree)",
    )
    args = parser.parse_args()
    if args.negative_controls:
        run_negative_controls()
    else:
        run_gate()


if __name__ == "__main__":
    main()
