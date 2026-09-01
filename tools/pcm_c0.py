#!/usr/bin/env python3
"""E10-C0 driver: capability-driven libavfilter minimization ladder.

Answers (with machine evidence, no DSP-backend selection):

  Can Qianqian treat FFmpeg/libavfilter as a capability repository and replay
  only the source closure required by mainstream music-player DSP features
  through the existing Xmake-based FFmpeg build system?

Pipeline (all machine-owned, resumable per stage):

  bench/dsp-capabilities.json (human-maintained capability intent)
      ↓ derive profile (codec base + tier filters)
  tools/common_import.py  → upstream configure/Make oracle (import-time only)
      ↓ V=1 compile-closure manifest (build/minimize/<stage>/manifest.json)
  xmake f --av_manifest=<manifest> → qianqian_av replay
      ↓ + qn_avfilter_cap_probe (bench-only product-shaped link)
  registration / smoke / negotiated-format / ldd / sizes / link-map live bytes
      ↓
  bench/results/avfilter-minimize/*.json  (authority tree)
      ↓
  docs/experiments/e10-libavfilter-minimization.md (generated tables)

  python3 tools/pcm_c0.py                 # run everything missing
  python3 tools/pcm_c0.py --stage avf-c2  # (re)run one ladder stage
  python3 tools/pcm_c0.py --probes        # run add-one probes
  python3 tools/pcm_c0.py --union         # manifest union accounting
  python3 tools/pcm_c0.py --aggregate     # derive aggregate JSONs
  python3 tools/pcm_c0.py --report        # regenerate doc tables
  python3 tools/pcm_c0.py --check         # drift check (exit 1 on drift)
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import platform
import re
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tools"))

CAPS = ROOT / "bench" / "dsp-capabilities.json"
CODEC_PROFILE = ROOT / "bench" / "profiles" / "n3-min-noswr.json"
RESULTS = ROOT / "bench" / "results" / "avfilter-minimize"
STAGES = ROOT / "build" / "minimize"
BUILDIR = "build/xmake-avf"
ARTIFACTS = ROOT / "build" / "artifacts"
ARCHIVE = ARTIFACTS / "libqianqian_av.a"
PROBE = ARTIFACTS / "qn_avfilter_cap_probe"
DOC = ROOT / "docs" / "experiments" / "e10-libavfilter-minimization.md"

TIER_FILE = {  # tier id -> results filename (task §27 naming)
    "F0": "f0-framework.json", "F1": "f1-core-eq.json", "F2": "f2-dynamics.json",
    "F3": "f3-stereo.json", "F4": "f4-classic-effects.json",
    "F5": "f5-utilities.json", "F6": "f6-advanced.json", "F7": "f7-tempo.json",
}
TIER_SHORT = {"F0": "framework", "F1": "core-eq", "F2": "dynamics",
              "F3": "stereo", "F4": "classic-effects", "F5": "utilities",
              "F6": "advanced", "F7": "tempo"}
# add-one probes (task §14); base = codec + F0
PROBE_FILTERS = ["alimiter", "loudnorm", "afir", "firequalizer", "surround",
                 "headphone", "atempo", "crossfeed", "chorus", "flanger"]
# which probe filters are expected to need the format-adaptation foundation
# (aresample+swresample); declared hypothesis, machine-verified by graph runs
# Review P0-1: the old PROBE_NEEDS_ARESAMPLE pre-declaration is REMOVED.
# Nothing is pre-paid: configure-required dependencies are parsed from the
# pinned configure; format adaptation is discovered by instantiating the
# graph and, only on a graph-config failure, adding the minimal conversion
# capability (aresample) and rebuilding.
LTO_STAGES = {"avf-c0", "avf-c1", "avf-c4", "avf-c8"}

PINNED_CONFIGURE = ROOT / "build" / "ffmpeg-src" / "configure"
FFMPEG_LIBS = {"avutil", "avcodec", "avformat", "avfilter", "swresample",
               "swscale", "postproc"}
EXPECTED_LICENSE_PREFIX = "LGPL"
ALLOWED_DYNAMIC_LIBS = {"libc.so.6", "libm.so.6"}
def sig(name, freq=1000.0, amp=0.5):
    return f"{name}:{freq}:{amp}"


# multi-input filters: minimal functional 2-input smokes (review P1-1
# Option A). chain is exactly ONE filter; the second input feeds pad 1.
MULTI_INPUT_SMOKE = {
    "afir": {"chain": "afir", "signal": sig("sine"), "signal_b": sig("impulse", 0, 1.0),
             "ch": 2, "ch_b": 2, "frames": 48000, "frames_b": 64},
    "acrossfade": {"chain": "acrossfade=d=1:c1=tri:c2=tri", "signal": sig("sine"),
                   "signal_b": sig("sine", 400, 0.4), "ch": 2, "ch_b": 2,
                   "frames": 96000, "frames_b": 96000},
    "headphone": {"chain": "headphone=map=FL|FR:hrir=multich", "signal": sig("sine"),
                  "signal_b": sig("impulse", 0, 1.0), "ch": 2, "ch_b": 4,
                  "frames": 48000, "frames_b": 64},
}

# experiment-defining inputs (review P1-2/P0-2 provenance): results bind
# these hashes, NOT a self-referential git commit pointer.
INPUT_FILES = ["bench/dsp-capabilities.json", "tools/pcm_c0.py",
               "tools/common_import.py", "tools/ffmpeg_import.py",
               "tools/ffmpeg_manifest_union.py",
               "bench/pcm/avfilter/qn_avfilter_cap_probe.c", "xmake.lua",
               "bench/profiles/n3-min-noswr.json", "bench/ffmpeg-pin.json"]

FILTER_ALIAS = {"asrc_abuffer": "abuffer", "asink_abuffer": "abuffersink"}
FILTER_ALWAYS_PRESENT = {"abuffer", "abuffersink"}  # base OBJS of libavfilter,
# unconditionally appended to filter_list by configure (configure:8953); no
# CONFIG_*_FILTER variable exists for them
FILTER_VIDEO_BUFFER = {"buffer"}  # vsrc/vsink_buffer land in filter_list too


def sh(cmd: list, **kw) -> subprocess.CompletedProcess:
    print("+", " ".join(map(str, cmd)), flush=True)
    return subprocess.run(list(map(str, cmd)), cwd=ROOT, **kw)


def must(cmd: list, **kw) -> str:
    p = subprocess.run(list(map(str, cmd)), cwd=ROOT, text=True,
                       stdout=subprocess.PIPE, stderr=subprocess.STDOUT, **kw)
    if p.returncode:
        print(p.stdout[-6000:], file=sys.stderr)
        raise SystemExit(f"failed ({p.returncode}): {' '.join(map(str, cmd))}")
    return p.stdout


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for b in iter(lambda: f.read(1 << 20), b""):
            h.update(b)
    return h.hexdigest()


def jwrite(path: Path, data) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(data, indent=1, sort_keys=True) + "\n")


def jload(path: Path):
    return json.loads(path.read_text())


# --------------------------------------------------------------------------
# provenance: experiment-defining inputs (review P1-2)
# --------------------------------------------------------------------------

def experiment_inputs() -> dict:
    files = {f: sha256_file(ROOT / f) for f in INPUT_FILES}
    combined = hashlib.sha256(
        "\n".join(f"{k}:{v}" for k, v in sorted(files.items())).encode()
    ).hexdigest()
    pin = jload(ROOT / "bench" / "ffmpeg-pin.json")
    return {"files": files, "combined_sha256": combined, "ffmpeg_pin": pin}


def git_info() -> dict:
    """Supplementary context only (review §24): authority is the input
    hashes, not a self-referential commit pointer."""
    return {"measured_source_commit":
            must(["git", "rev-parse", "HEAD"]).strip(),
            "git_branch":
            must(["git", "rev-parse", "--abbrev-ref", "HEAD"]).strip()}


def configure_required_libs(filters) -> dict:
    """Parse <filter>_filter_deps from the pinned upstream configure.
    These are authoritative hard dependencies (review task §15); anything
    NOT here is a graph-format-adaptation question answered by executing
    the graph, never by pre-declaration."""
    if not PINNED_CONFIGURE.is_file():
        raise SystemExit(f"pinned configure missing: {PINNED_CONFIGURE} "
                         "(run the repo's FFmpeg source fetch/import first)")
    text = PINNED_CONFIGURE.read_text(errors="replace")
    out = {}
    for f in sorted(filters):
        m = re.search(rf"^{re.escape(f)}_filter_deps=\"([^\"]*)\"",
                      text, re.M)
        if m:
            libs = [d for d in m.group(1).split() if d in FFMPEG_LIBS]
            if libs:
                out[f] = sorted(libs)
    return out


def graph_config_failures(smoke: dict) -> list:
    """Graph checks that failed specifically at avfilter_graph_config —
    the discovery signal that a format conversion is required but the
    closure has no conversion filter."""
    return [r for r in smoke.get("results", [])
            if r.get("kind") in ("graph", "graph2")
            and not r.get("ok")
            and str(r.get("error", "")).startswith("graph_config")]


# --------------------------------------------------------------------------
# stage model
# --------------------------------------------------------------------------

def caps_doc() -> dict:
    return jload(CAPS)


def ladder() -> list:
    """[(stage_id, tier_id|None, cumulative tier ids)]"""
    caps = caps_doc()
    tiers = [t["id"] for t in caps["tiers"]]
    out = [("avf-c0", None, [])]
    for i, tid in enumerate(tiers):
        out.append((f"avf-c{i+1}", tid, tiers[:i + 1]))
    return out


def stage_dir(stage: str) -> Path:
    return STAGES / stage


def stage_result_file(stage: str) -> Path:
    caps = caps_doc()
    if stage == "avf-c0":
        return RESULTS / "c0-codec-only.json"
    m = re.fullmatch(r"avf-c(\d+)", stage)
    if m:
        tid = caps["tiers"][int(m.group(1)) - 1]["id"]
        return RESULTS / TIER_FILE[tid]
    m = re.fullmatch(r"avf-p-(.+)", stage)
    if m:
        return RESULTS / f"probe-{m.group(1)}.json"
    return RESULTS / f"{stage}.json"


def derive_profile(stage: str, tier_ids: list, extra_filters=(),
                   adaptation: bool = False) -> dict:
    """Capability intent (tier filters + probe filter) + configure-required
    dependencies parsed from the pinned configure. NO pre-paid format
    adaptation: with adaptation=True the minimal conversion capability
    (aresample -> swresample) is added as a DISCOVERED requirement, and the
    result records that explicitly."""
    base = jload(CODEC_PROFILE)
    caps = caps_doc()
    by_id = {t["id"]: t for t in caps["tiers"]}
    p = json.loads(json.dumps(base))  # deep copy
    p["profile"] = stage
    p["experiment"] = "e10-c0"
    p["derived_from"] = CODEC_PROFILE.name
    p["description"] = f"E10-C0 ladder: codec closure + tiers {tier_ids or '[]'}"
    filters: set = set(extra_filters)
    for tid in tier_ids:
        filters |= set(by_id[tid]["filters"])
    req = configure_required_libs(filters)
    req_libs = sorted({l for libs in req.values() for l in libs})
    enable = list(p["libraries"]["enable"])
    disable = list(p["libraries"]["disable"])
    if filters:  # avf-c0 stays codec-only
        enable.append("avfilter")
        enable += req_libs
    disable = [x for x in disable if x not in ("avfilter", "swresample")]
    # --disable-everything only disables components; libraries stay on by
    # default, so swresample must stay explicitly disabled unless a
    # configure-required dep (pan) or a discovered adaptation needs it
    if "swresample" not in enable:
        disable.append("swresample")
    if adaptation:
        filters.add("aresample")   # aresample_filter_deps=swresample
        enable.append("swresample")
        disable = [x for x in disable if x != "swresample"]
    p["libraries"] = {"enable": sorted(set(enable)), "disable": sorted(set(disable))}
    p.setdefault("components", {})["filter"] = sorted(filters)
    p["configure_required_deps"] = req
    p["format_adaptation_added"] = bool(adaptation)
    return p


def write_profile(stage: str, tier_ids: list, extra_filters=(),
                  adaptation: bool = False) -> Path:
    d = stage_dir(stage)
    d.mkdir(parents=True, exist_ok=True)
    pf = d / "profile.json"
    prof = derive_profile(stage, tier_ids, extra_filters, adaptation)
    if adaptation:
        # persist the direct (pre-adaptation) profile alongside for
        # provenance; the oracle imports d/profile.json
        direct = json.loads(pf.read_text()) if pf.is_file() else None
        if direct and not direct.get("format_adaptation_added"):
            (d / "profile-direct.json").write_text(json.dumps(direct, indent=1) + "\n")
    pf.write_text(json.dumps(prof, indent=1) + "\n")
    return pf


# --------------------------------------------------------------------------
# oracle (upstream configure/Make, import-time only)
# --------------------------------------------------------------------------

def run_oracle(stage: str, tier_ids: list, force: bool = False,
               rewrite_profile: bool = True) -> None:
    d = stage_dir(stage)
    if (d / "manifest.json").is_file() and not force:
        m = jload(d / "manifest.json")
        want = sha256_file(d / "profile.json")
        if m.get("profile_sha256") == want:
            print(f"[{stage}] oracle manifest exists and matches profile, "
                  "skipping import")
            return
        # the oracle import is itself a cached stage whose identity is the
        # derived profile; a stale manifest would silently measure the OLD
        # closure (this actually happened once and poisoned direct-vs-
        # effective accounting — caught and re-run)
        print(f"[{stage}] oracle manifest profile MISMATCH (stale), "
              "re-importing")
    if rewrite_profile:
        write_profile(stage, tier_ids)
    pf = d / "profile.json"
    shutil.rmtree(d / "oracle", ignore_errors=True)
    must([sys.executable, "tools/common_import.py", "--stage", stage,
          "--profile", str(pf.relative_to(ROOT)), "--force"])


def parse_config_evidence(stage: str) -> dict:
    d = stage_dir(stage)
    mak = d / "oracle" / "ffbuild" / "config.mak"
    filters, externals, misc = [], {}, {}
    if mak.is_file():
        for line in mak.read_text(errors="replace").splitlines():
            m = re.fullmatch(r"CONFIG_(\w+)_FILTER=yes", line)
            if m:
                name = m.group(1).lower()
                filters.append(FILTER_ALIAS.get(name, name))
            m = re.fullmatch(r"CONFIG_([A-Z0-9_]*LIB[A-Z0-9_]*)=yes", line)
            if m:
                externals[m.group(1)] = True
            m = re.fullmatch(r"CONFIG_(RUBBERBAND|LIBRUBBERBAND|LIBMYSOFA|LADSPA|LV2|OPENCL|LIBVULKAN)=yes", line)
            if m:
                externals[m.group(1)] = True
            m = re.fullmatch(r"(FFMPEG_LICENSE|CONFIG_SMALL|CONFIG_STATIC|CONFIG_SHARED)=(.*)", line)
            if m:
                misc[m.group(1)] = m.group(2).strip()
    license_ = None
    cfg_h = d / "oracle" / "config.h"
    if cfg_h.is_file():
        m2 = re.search(r'#define FFMPEG_LICENSE "(.*)"', cfg_h.read_text(errors="replace"))
        if m2:
            license_ = m2.group(1)
    # generated filter registration list (first-class provenance, task §19);
    # only meaningful when libavfilter itself is in the closure
    flist = d / "oracle" / "libavfilter" / "filter_list.c"
    registered = []
    if flist.is_file() and (d / "oracle" / "libavfilter" / "libavfilter.a").is_file():
        for sym in re.findall(r"&ff_([a-z0-9_]+)", flist.read_text()):
            # ff_asrc_abuffer/ff_asink_abuffer -> abuffer/abuffersink; other
            # classes strip their prefix (ff_af_volume -> volume)
            if sym in FILTER_ALIAS:
                name = FILTER_ALIAS[sym]
            elif "_" in sym:
                name = sym.split("_", 1)[1]
            else:
                name = sym
            if name not in FILTER_VIDEO_BUFFER:
                registered.append(name)
    return {
        "filters_enabled_config": sorted(set(filters)),
        "avfilter_in_closure": (d / "oracle" / "libavfilter" / "libavfilter.a").is_file(),
        "external_libs_enabled": sorted(externals),
        "license": license_,
        "misc": misc,
        "filter_list_registered": sorted(set(registered)),
        "filter_list_sha256": sha256_file(flist) if flist.is_file() else None,
    }


# --------------------------------------------------------------------------
# manifest mutation (shipping / lto projections, minimize_flags.py style)
# --------------------------------------------------------------------------

def projected_manifest(stage: str, kind: str) -> Path:
    """kind: 'shipping' (-Os + function/data sections) or 'lto' (+ -flto).

    Same flag-mutation semantics as tools/minimize_flags.py: the closure is
    unchanged, only codegen granularity/level moves (S4/S5 style).
    """
    src = stage_dir(stage) / "manifest.json"
    m = jload(src)
    extra = ["-ffunction-sections", "-fdata-sections"]
    if kind == "lto":
        extra.append("-flto")
    for u in m["units"]:
        flags = u.get("flags") or []
        out, replaced = [], False
        for f in flags:
            if (re.fullmatch(r"-O\d", f) or f == "-Os") and f != "-O0":
                if not replaced:
                    out.append("-Os")
                    replaced = True
                continue
            if f == "-flto":
                continue
            out.append(f)
        out.extend(x for x in extra if x not in out)
        u["flags"] = out
    m["projection"] = {"kind": kind, "derived_from": src.name,
                       "derived_from_sha256": sha256_file(src),
                       "flag_mutation": extra + ["replace -O* with -Os"]}
    out_path = stage_dir(stage) / f"manifest-{kind}.json"
    out_path.write_text(json.dumps(m, indent=2, sort_keys=True) + "\n")
    return out_path


# --------------------------------------------------------------------------
# xmake replay + measurement
# --------------------------------------------------------------------------

def xmake_build(manifest_rel: str, gc=False, lto=False, link_map: Path | None = None,
                no_avfilter: bool = False) -> None:
    shutil.rmtree(ROOT / BUILDIR, ignore_errors=True)
    shutil.rmtree(ARTIFACTS, ignore_errors=True)
    # av_replay_exact=y is C0-only (review P0-3): it normalizes the replayed
    # C units (-fvisibility=default -UNDEBUG) for upstream-oracle equivalence.
    # Normal production builds never set it and keep mode.release semantics.
    cfg = ["xmake", "f", "-o", BUILDIR, "-m", "release",
           f"--av_manifest={manifest_rel}", "--av_replay_exact=y", "-y"]
    if gc:
        cfg.append("--gc_sections=y")
    if lto:
        cfg.append("--lto=y")
    must(cfg)
    env = dict(os.environ)
    if link_map:
        env["QN_LINK_MAP"] = str(link_map)
    if no_avfilter:
        env["QN_PROBE_NO_AVFILTER"] = "1"
    must(["xmake", "build", "qn_avfilter_cap_probe"], env=env)


def run_probe(scenario: Path, out: Path) -> dict:
    p = subprocess.run([str(PROBE), str(scenario), str(out)],
                       cwd=ROOT, capture_output=True, text=True)
    if not out.is_file():
        raise SystemExit(f"probe produced no JSON (rc={p.returncode}): {p.stderr[-2000:]}")
    return jload(out)


def ldd_probe() -> list:
    out = must(["ldd", str(PROBE)])
    deps = []
    for line in out.splitlines():
        m = re.match(r"\s*(\S+) => (\S+)", line)
        if m:
            deps.append({"lib": m.group(1), "path": m.group(2)})
    return deps


def binary_sizes(binary: Path, workdir: Path, tag: str) -> dict:
    raw = binary.stat().st_size
    stripped = workdir / f"{tag}.stripped"
    shutil.copyfile(binary, stripped)
    must(["strip", str(stripped)])
    ss = stripped.stat().st_size
    must(["xz", "-9", "-f", "-k", str(stripped)])
    xs = (workdir / f"{tag}.stripped.xz").stat().st_size
    return {"raw_bytes": raw, "stripped_bytes": ss, "xz_bytes": xs}


def ff_symbols_archive(archive: Path) -> set:
    out = must(["nm", "--defined-only", str(archive)])
    # normalize GCC codegen split suffixes (.part.0/.constprop.N/...) — the
    # oracle Make build and the xmake replay may split identically-valued
    # symbols differently; registration identity is the ff_* stem
    norm = lambda s: re.sub(r"\.(part|constprop|isra|unlikely|cold)[.\d]*$", "", s)
    return {norm(l.split()[-1]) for l in out.splitlines()
            if l.strip() and " ff_" in l}


def archive_members(archive: Path) -> list:
    return [l for l in must(["ar", "t", str(archive)]).splitlines() if l.strip()]


def library_of_unit(unit: dict) -> str:
    return unit["object"].split("/")[0]


def per_library_units(manifest: dict) -> dict:
    out = {}
    for u in manifest["units"]:
        out[library_of_unit(u)] = out.get(library_of_unit(u), 0) + 1
    return out


def parse_live_map(map_path: Path, members: list, manifest: dict) -> dict:
    """Live-bytes ledger from a --gc-sections link map.

    member→unit alignment: xmake archives qianqian_av members in manifest
    order (validated 1:1 by tools/link_audit.py in E07; re-asserted here so
    a xmake ordering change fails loudly instead of mis-attributing bytes).
    Map lines referencing the archive appear as
    '<path>libqianqian_av.a(<member>)'.
    """
    text = map_path.read_text(errors="replace")
    idx_to_unit = [u["object"] for u in manifest["units"]]
    if len(idx_to_unit) != len(members):
        raise SystemExit(f"archive/manifest drift: {len(members)} members "
                         f"vs {len(idx_to_unit)} units")
    def member_base(name: str) -> str:
        # xmake keeps the source suffix in archive members (ac3_parser.c.o);
        # same normalization as tools/link_audit.py
        for suf in (".c.o", ".s.o", ".S.o", ".cpp.o", ".m.o"):
            if name.endswith(suf):
                return name[: -len(suf)] + ".o"
        return name

    for i, (member, obj) in enumerate(zip(members, idx_to_unit)):
        if member_base(member) != Path(obj).name:
            raise SystemExit(f"archive/manifest order drift at {i}: "
                             f"member {member} vs unit {obj}")

    live_by_unit: dict[str, int] = {}
    live_total = 0
    disc_total = 0
    unrecognized = set()
    in_discarded = False
    member_re = re.compile(r"^(\s*)(\S+)\s+0x[0-9a-f]+\s+0x([0-9a-f]+)\s+(.*)$")
    # GNU ld wraps long input-section names onto their own line:
    #   .note.gnu.property
    #                 0x0000000000000000       0x20 <file>
    # so merge a name-only line with its continuation before matching.
    lines = text.splitlines()
    merged = []
    i = 0
    while i < len(lines):
        cur = lines[i]
        if i + 1 < len(lines) and re.fullmatch(r"\s*\.\S+\s*", cur) \
                and not re.search(r"0x[0-9a-f]", cur):
            cur = cur.rstrip() + " " + lines[i + 1].strip()
            i += 1
        merged.append(cur)
        i += 1
    for raw in merged:
        line = raw.rstrip("\n")
        if line.startswith("Discarded input sections"):
            in_discarded = True
            continue
        if line.startswith("Linker script and memory map"):
            in_discarded = False
            continue
        m = member_re.match(line)
        if not m:
            continue
        _indent, sec, size_hex, rest = m.groups()
        size = int(size_hex, 16)
        if in_discarded:
            if (size and sec.startswith((".text", ".data", ".rodata", ".bss"))
                    and "libqianqian_av.a(" in rest):
                disc_total += size
            continue
        ma = re.search(r"libqianqian_av\.a\(([^)]+)\)", rest)
        if not ma:
            continue
        member = ma.group(1)
        if member not in members:
            unrecognized.add(member)
            continue
        unit = idx_to_unit[members.index(member)]
        live_by_unit[unit] = live_by_unit.get(unit, 0) + size
        live_total += size
    if unrecognized:
        raise SystemExit(f"link map references archive members absent from "
                         f"`ar t` listing: {sorted(unrecognized)[:5]}")
    lib_live: dict[str, int] = {}
    lib_compiled: dict[str, int] = {}
    for u in manifest["units"]:
        lib = library_of_unit(u)
        lib_compiled[lib] = lib_compiled.get(lib, 0) + 1
        if u["object"] in live_by_unit:
            lib_live[lib] = lib_live.get(lib, 0) + 1
    return {
        "live_bytes_total": live_total,
        "live_units": len(live_by_unit),
        "discarded_bytes_total": disc_total,
        "live_units_by_library": lib_live,
        "compiled_units_by_library": lib_compiled,
        "live_bytes_by_unit_top": sorted(
            ({"unit": k, "bytes": v} for k, v in live_by_unit.items()),
            key=lambda e: -e["bytes"])[:40],
    }


def oracle_archive_set(stage: str) -> set:
    d = stage_dir(stage) / "oracle"
    syms = set()
    for lib in ("libavfilter", "libavutil", "libavcodec", "libavformat", "libswresample"):
        a = d / lib / f"{lib}.a"
        if a.is_file():
            syms |= ff_symbols_archive(a)
    return syms


# --------------------------------------------------------------------------
# scenarios
# --------------------------------------------------------------------------

def graph_smoke_table():
    """(tier, id, chain, signal, rate, ch, frames, kwargs) per capability.
    Two-input filters (afir, acrossfade, headphone) are intentionally absent:
    outside the linear probe scope, they get registration + closure evidence."""
    return [
        ("F0", "f0_endpoints", "", sig("sine"), 48000, 2, 48000,
         {"want_fmt": "flt", "expect": "frames_equal"}),
        ("F0", "f0_anull", "anull", sig("sine"), 48000, 2, 48000,
         {"want_fmt": "flt", "expect": "frames_equal"}),
        ("F0", "f0_aformat_flt", "aformat=sample_fmts=flt:sample_rates=48000",
         sig("sine"), 48000, 2, 48000, {"want_fmt": "flt", "sink_fmt": "flt"}),
        ("F1", "f1_volume", "volume=volume=-6dB", sig("sine"), 48000, 2, 48000,
         {"expect": "amplitude_db:db=-6.0206:tol=0.5:in_amp=0.5",
          "lifecycle": 1}),
        ("F1", "f1_equalizer", "equalizer=frequency=1000:gain=6",
         sig("sine", 1000), 48000, 2, 48000,
         {"expect": "rms_ratio_vs_anull_db:ch=0:min=3.5:max=8.5", "lifecycle": 1}),
        ("F1", "f1_bass", "bass=g=6:f=100", sig("sine", 30), 48000, 2, 48000,
         {"expect": "rms_ratio_vs_anull_db:ch=0:min=4.0:max=8.5"}),
        ("F1", "f1_treble", "treble=g=6:f=8000", sig("sine", 16000), 48000, 2, 48000,
         {"expect": "rms_ratio_vs_anull_db:ch=0:min=3.5:max=8.5"}),
        ("F1", "f1_lowshelf", "lowshelf=g=6:f=200", sig("sine", 100), 48000, 2, 48000,
         {"expect": "rms_ratio_vs_anull_db:ch=0:min=3.5:max=8.5"}),
        ("F1", "f1_highshelf", "highshelf=g=6:f=4000", sig("sine", 6000), 48000, 2, 48000,
         {"expect": "rms_ratio_vs_anull_db:ch=0:min=3.5:max=8.5"}),
        ("F1", "f1_lowpass", "lowpass=f=500", sig("sine", 1000), 48000, 2, 48000,
         {"expect": "rms_ratio_vs_anull_db:ch=0:min=-30:max=-6"}),
        ("F1", "f1_highpass", "highpass=f=500", sig("sine", 100), 48000, 2, 48000,
         {"expect": "rms_ratio_vs_anull_db:ch=0:min=-30:max=-6"}),
        ("F2", "f2_alimiter", "alimiter=limit=0.5:level=0", sig("noise", 0, 0.9), 48000, 2, 48000,
         {"expect": "bounded:max=0.55", "lifecycle": 1}),
        ("F2", "f2_acompressor", "acompressor=threshold=0.1:ratio=4",
         sig("noise", 0, 0.9), 48000, 2, 48000,
         {"expect": "rms_ratio_vs_anull_db:ch=0:min=-30:max=-3",
          "note": "no lookahead: attack transient passes near-input peaks, so the assertion is energy reduction, not a peak bound"}),
        ("F2", "f2_agate", "agate=threshold=0.001", sig("sine"), 48000, 2, 48000,
         {"expect": "frames_equal"}),
        ("F3", "f3_pan_balance", "pan=stereo|c0=0.5*c0|c1=0.5*c1", sig("sine"),
         48000, 2, 48000, {"expect": "amplitude_db:db=-6.0206:tol=0.6:in_amp=0.5"}),
        ("F3", "f3_pan_downmix", "pan=mono|c0=0.5*c0+0.5*c1", sig("sine"),
         48000, 2, 48000, {"want_channels": 1}),
        ("F3", "f3_channelmap_swap", "channelmap=1|0", sig("impulse", 0, 1.0),
         48000, 2, 64, {"expect": "cross_channel:src=0:dst=1:min_rms=0.05:src_free=1"}),
        ("F3", "f3_crossfeed", "crossfeed", sig("impulse", 0, 1.0), 48000, 2, 8192,
         {"expect": "cross_channel:src=0:dst=1:min_rms=1e-4", "lifecycle": 1}),
        ("F3", "f3_stereowiden", "stereowiden=delay=20:feedback=0.3:crossfeed=0.3",
         sig("sine"), 48000, 2, 48000, {"expect": "frames_equal"}),
        ("F4", "f4_aecho", "aecho=0.8:0.9:40:0.5", sig("impulse", 0, 1.0),
         48000, 2, 8192, {"lifecycle": 1}),
        ("F4", "f4_chorus", "chorus=0.5:0.9:50|60:0.4|0.32:0.25|0.4:2|1.3",
         sig("sine"), 48000, 2, 48000, {}),
        ("F4", "f4_flanger", "flanger", sig("sine"), 48000, 2, 48000, {}),
        ("F4", "f4_aphaser", "aphaser", sig("sine"), 48000, 2, 48000, {}),
        ("F4", "f4_tremolo", "tremolo=f=4:d=0.5", sig("sine"), 48000, 2, 48000,
         {"expect": "frames_equal"}),
        ("F4", "f4_vibrato", "vibrato=f=4:d=0.5", sig("sine"), 48000, 2, 48000,
         {"expect": "frames_equal"}),
        ("F5", "f5_afade", "afade=t=in:st=0:d=1", sig("sine"), 48000, 2, 48000,
         {"expect": "frames_equal"}),
        ("F5", "f5_adelay", "adelay=20|20", sig("sine"), 48000, 2, 48000,
         {"expect": "duration_ratio:ratio=1.02:tol=0.01"}),
        ("F5", "f5_loudnorm", "loudnorm=I=-20:TP=-2:LRA=7", sig("sine"),
         48000, 2, 48000, {"lifecycle": 1}),
        ("F5", "f5_dynaudnorm", "dynaudnorm", sig("sine"), 48000, 2, 48000, {}),
        ("F6", "f6_firequalizer", "firequalizer=gain=if(between(f,800,1250),6,0)",
         sig("sine", 1000), 48000, 2, 48000,
         {"expect": "rms_ratio_vs_anull_db:ch=0:min=3.0:max=9.0"}),
        ("F6", "f6_surround", "surround", sig("sine"), 48000, 2, 48000,
         {"want_channels": 6}),
        ("F7", "f7_atempo", "atempo=0.5", sig("sine"), 48000, 1, 96000,
         {"expect": "duration_ratio:ratio=2.0:tol=0.02", "lifecycle": 1}),
    ]


def scenario_checks(stage_tier_ids: list, stage_filters: set,
                    extra_filter: str | None = None) -> list:
    """Registration + graph smokes for everything this stage enables."""
    caps = caps_doc()
    checks = []
    for f in sorted(stage_filters):
        checks.append({"kind": "present", "name": f})
    enabled_tiers = set(stage_tier_ids)
    absent = []
    for t in caps["tiers"]:
        if t["id"] not in enabled_tiers:
            absent += [f for f in t["filters"] if f not in stage_filters]
    for e in caps["external_optional"]:
        absent.append(e["filter"])
    for c in caps["considered_not_enabled"]:
        absent.append(c["filter"])
    for f in sorted(set(absent)):
        if f not in [c["name"] for c in checks if c["kind"] == "present"]:
            checks.append({"kind": "absent", "name": f})

    enabled = set(stage_tier_ids)
    chosen = []
    for row in graph_smoke_table():
        if row[0] in enabled:
            chosen.append(row)
    if extra_filter:
        for row in graph_smoke_table():
            if row[2].split("=")[0] == extra_filter and row not in chosen:
                chosen.append(row)
    for tier, cid, chain, signal, rate, ch, frames, kw in chosen:
        c = {"kind": "graph", "id": cid, "chain": chain, "signal": signal,
             "rate": rate, "ch": ch, "frames": frames, "block": 1024}
        c.update(kw)
        checks.append(c)
    # multi-input filters (review P1-1 Option A): minimal functional
    # 2-input smokes for every such filter this stage enables
    all_filters = set(stage_filters) | ({extra_filter} if extra_filter else set())
    for mf, spec in MULTI_INPUT_SMOKE.items():
        if mf in all_filters:
            c = {"kind": "graph2", "id": f"mi_{mf}", "chain": spec["chain"],
                 "signal": spec["signal"], "signal_b": spec["signal_b"],
                 "rate": 48000, "ch": spec["ch"], "ch_b": spec["ch_b"],
                 "frames": spec["frames"], "frames_b": spec["frames_b"],
                 "block": 1024}
            checks.append(c)
    return checks


def write_scenario(path: Path, checks: list) -> Path:
    lines = []
    for c in checks:
        if c["kind"] in ("present", "absent"):
            lines.append(f"kind={c['kind']} name={c['name']}")
            continue
        parts = [f"kind={c['kind']}", f"id={c['id']}", f"chain={c['chain']}",
                 f"signal={c['signal']}", f"rate={c['rate']}", f"ch={c['ch']}",
                 f"frames={c['frames']}", f"block={c['block']}"]
        for k in ("sink_fmt", "want_fmt", "want_rate", "want_channels",
                  "expect", "lifecycle", "signal_b", "ch_b", "frames_b"):
            if k in c:
                parts.append(f"{k}={c[k]}")
        lines.append(" ".join(parts))
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("\n".join(lines) + "\n")
    return path


# --------------------------------------------------------------------------
# one ladder/probe stage end-to-end
# --------------------------------------------------------------------------

def stage_identity(stage: str, tier_ids: list, extra_filters=()) -> str:
    """Hash of the stage's derived DIRECT profile (the experiment-defining
    identity for cache-freshness decisions)."""
    prof = derive_profile(stage, tier_ids, extra_filters, adaptation=False)
    return hashlib.sha256(
        json.dumps(prof, sort_keys=True).encode()).hexdigest()


def run_stage(stage: str, tier_ids: list, force: bool = False) -> dict:
    d = stage_dir(stage)
    res_file = stage_result_file(stage)
    manifest_rel = f"build/minimize/{stage}/manifest.json"
    no_avf = stage == "avf-c0"

    # cache freshness (review P1-2/§23): reuse cached evidence only when the
    # experiment-defining inputs, the FFmpeg pin and the stage identity all
    # still match; otherwise the evidence is STALE and the stage reruns.
    if res_file.is_file() and not force:
        cached = jload(res_file)
        cur = experiment_inputs()
        if (cached.get("experiment_inputs", {}).get("combined_sha256")
                == cur["combined_sha256"]
                and cached.get("experiment_inputs", {}).get("ffmpeg_pin")
                == cur["ffmpeg_pin"]
                and cached.get("stage_identity_sha256")
                == stage_identity(stage, tier_ids)):
            print(f"[{stage}] evidence fresh ({res_file.name}), skipping")
            return cached
        print(f"[{stage}] cached evidence STALE (input/identity hash "
              f"mismatch), rerunning")

    # ---- pass A: DIRECT closure — configure-required deps only ----
    write_profile(stage, tier_ids)
    run_oracle(stage, tier_ids, force=force)
    manifest_a = jload(d / "manifest.json")
    filters_a = set(manifest_a and
                    jload(d / "profile.json")["components"].get("filter", []))
    scenario_a = write_scenario(d / "scenario.kv",
                                scenario_checks(tier_ids, filters_a))
    xmake_build(manifest_rel, gc=False, lto=False, no_avfilter=no_avf)
    smoke_a = run_probe(scenario_a, d / "smoke-direct.json")
    failures = graph_config_failures(smoke_a)

    # ---- pass B (only on discovery): minimal format adaptation ----
    adaptation_needed = bool(failures)
    if adaptation_needed:
        print(f"[{stage}] discovered format-adaptation requirement: "
              f"{[f['id'] for f in failures]} -> adding aresample")
        write_profile(stage, tier_ids, adaptation=True)
        run_oracle(stage, tier_ids, force=True, rewrite_profile=False)
        smoke_a = None  # re-run the plain evidence on the effective closure

    profile = jload(d / "profile.json")
    stage_filters = set(profile["components"].get("filter", []))
    manifest = jload(d / "manifest.json")
    config_ev = parse_config_evidence(stage)
    tier_ids_eff = list(tier_ids)

    # registration gates from generated provenance + config
    reg_gate = {
        "intended_filters": sorted(stage_filters),
        "config_enabled": config_ev["filters_enabled_config"],
        "filter_list_registered": config_ev["filter_list_registered"],
        "always_present": sorted(FILTER_ALWAYS_PRESENT),
        "intended_subset_of_config": (
            stage_filters - (FILTER_ALWAYS_PRESENT
                             if config_ev["avfilter_in_closure"] else set())
            <= set(config_ev["filters_enabled_config"])),
        "filter_list_matches_config": (
            set(config_ev["filter_list_registered"])
            == (set(config_ev["filters_enabled_config"]) | FILTER_ALWAYS_PRESENT
                if config_ev["avfilter_in_closure"] else set())),
    }

    # ---- xmake replay: plain (final = effective closure) ----
    scenario = write_scenario(d / "scenario.kv",
                              scenario_checks(tier_ids_eff, stage_filters))
    plain_map = d / "plain.map"
    xmake_build(manifest_rel, gc=False, lto=False, link_map=plain_map,
                no_avfilter=no_avf)
    plain_archive_bytes = ARCHIVE.stat().st_size
    plain_probe_sha = sha256_file(PROBE)
    xmake_syms = ff_symbols_archive(ARCHIVE)
    oracle_syms = oracle_archive_set(stage)
    sym_gate = {
        "xmake_ff_symbols": len(xmake_syms),
        "oracle_ff_symbols": len(oracle_syms),
        "equal": xmake_syms == oracle_syms,
        "missing_in_xmake": sorted(oracle_syms - xmake_syms)[:20],
        "extra_in_xmake": sorted(xmake_syms - oracle_syms)[:20],
    }

    smoke_plain = run_probe(scenario, d / "smoke-plain.json")
    plain_ldd = ldd_probe()

    # ---- xmake replay: shipping (-Os + sections + gc) ----
    ship_manifest = projected_manifest(stage, "shipping")
    ship_map = d / "shipping.map"
    xmake_build(f"build/minimize/{stage}/manifest-shipping.json", gc=True,
                lto=False, link_map=ship_map, no_avfilter=no_avf)
    shipping_sizes = binary_sizes(PROBE, d, "probe")
    shipping_archive_bytes = ARCHIVE.stat().st_size
    smoke_ship = run_probe(scenario, d / "smoke-shipping.json")
    shipping_ldd = ldd_probe()
    members = archive_members(ARCHIVE)
    live = parse_live_map(ship_map, members,
                          jload(d / "manifest-shipping.json"))

    # residual discovery failures on the effective closure are hard errors
    residual = graph_config_failures(smoke_ship)

    # ---- xmake replay: lto (representative points only) ----
    lto_data = None
    if stage in LTO_STAGES:
        projected_manifest(stage, "lto")
        xmake_build(f"build/minimize/{stage}/manifest-lto.json", gc=True,
                    lto=True, no_avfilter=no_avf)
        lto_data = {"sizes": binary_sizes(PROBE, d, "probe-lto"),
                    "archive_bytes": ARCHIVE.stat().st_size}

    req = profile.get("configure_required_deps", {})
    data = {
        "stage": stage,
        "tier": tier_ids_eff[-1] if tier_ids_eff else None,
        "cumulative_tiers": tier_ids_eff,
        "capabilities": [c["capability"] for tid in tier_ids_eff
                         for c in next(t for t in caps_doc()["tiers"]
                                       if t["id"] == tid)["product_capabilities"]],
        "profile": {
            "configure_args": manifest["configure_args"],
            "configure_warnings": manifest.get("configure_warnings", []),
            "profile_sha256": manifest["profile_sha256"],
            "configure_required_deps": req,
            "format_adaptation_added":
                profile.get("format_adaptation_added", False),
        },
        "stage_identity_sha256": stage_identity(stage, tier_ids),
        "experiment_inputs": experiment_inputs(),
        "source": git_info(),
        "direct_closure": {
            "manifest_units": manifest_a["closure"]["translation_units"],
            "per_library": per_library_units(manifest_a),
            "profile_sha256": manifest_a["profile_sha256"],
            "graph_config_failures": [f["id"] for f in failures],
        },
        "format_adaptation": {
            "required": adaptation_needed,
            "added": ["aresample"] if adaptation_needed else [],
            "added_libs": ["libswresample"] if adaptation_needed else [],
            "class": ("graph-format-adaptation" if adaptation_needed else
                      ("configure-required" if req.get("swresample") and
                       any(v == ["swresample"] for v in req.values()) else
                      "none")),
            "configure_required_deps": req,
            "discovered_by": ("avfilter_graph_config failure on canonical "
                              "flt input") if adaptation_needed else None,
        },
        "config_evidence": config_ev,
        "registration_gate": reg_gate,
        "symbol_gate": sym_gate,
        "manifest": {
            "units": manifest["closure"]["translation_units"],
            "per_library": per_library_units(manifest),
            "generated_units": manifest["closure"]["generated_sources"],
            "config_root": manifest["config_root"],
        },
        "plain": {
            "archive_bytes": plain_archive_bytes,
            "probe_sha256": plain_probe_sha,
            "ldd": plain_ldd,
            "smoke_verdict": smoke_plain["summary"]["verdict"],
            "smoke_fail": smoke_plain["summary"]["fail"],
        },
        "shipping": {
            "archive_bytes": shipping_archive_bytes,
            "probe_sizes": shipping_sizes,
            "ldd": shipping_ldd,
            "smoke_verdict": smoke_ship["summary"]["verdict"],
            "smoke_fail": smoke_ship["summary"]["fail"],
            "live": live,
        },
        "lto": lto_data,
        "smoke_detail": {"plain": smoke_plain, "shipping": smoke_ship,
                         "direct": smoke_a},
        "residual_config_failures": [f["id"] for f in residual],
    }

    gates = gates_for(data)
    data["gates"] = gates
    jwrite(res_file, data)
    (d / "smoke-plain.json").write_text(json.dumps(smoke_plain, indent=1) + "\n")
    (d / "smoke-shipping.json").write_text(json.dumps(smoke_ship, indent=1) + "\n")
    print(f"[{stage}] gates: "
          f"{'PASS' if all(g['verdict'] == 'PASS' for g in gates.values()) else 'FAIL'}")
    return data


def gates_for(data: dict) -> dict:
    g = {}
    g["registration"] = {
        "verdict": "PASS" if data["registration_gate"]["intended_subset_of_config"]
                              and data["registration_gate"]["filter_list_matches_config"]
                   else "FAIL"}
    g["symbols"] = {"verdict": "PASS" if data["symbol_gate"]["equal"] else "FAIL"}
    g["smoke_plain"] = {"verdict": data["plain"]["smoke_verdict"],
                        "fail": data["plain"]["smoke_fail"]}
    g["smoke_shipping"] = {"verdict": data["shipping"]["smoke_verdict"],
                           "fail": data["shipping"]["smoke_fail"]}
    g["external_libs"] = {
        "verdict": "PASS" if not data["config_evidence"]["external_libs_enabled"]
                   else "FAIL",
        "external": data["config_evidence"]["external_libs_enabled"]}
    g["no_residual_config_failures"] = {
        "verdict": "PASS" if not data.get("residual_config_failures")
                   else "FAIL",
        "failures": data.get("residual_config_failures", [])}
    return g


# --------------------------------------------------------------------------
# add-one probes
# --------------------------------------------------------------------------

def tx_units(manifest: dict) -> list:
    """libavutil transform/FFT foundation units — the shared FFT cost."""
    return sorted(u["path"] for u in manifest["units"]
                  if re.search(r"libavutil/.*?(tx|fft|avfft)", u["path"]))


def probe_verdict(data: dict) -> str:
    """Recompute a probe's verdict from its raw graph evidence (fail-closed;
    the stored smoke string is never trusted)."""
    graphs = [g for g in data.get("graph_evidence", [])]
    if not graphs:
        return "FAIL"
    return "PASS" if all(g.get("ok") for g in graphs) else "FAIL"


def run_probe_stage(flt: str, force: bool = False) -> dict:
    stage = f"avf-p-{flt}"
    res_file = stage_result_file(stage)
    manifest_rel = f"build/minimize/{stage}/manifest.json"

    # cache freshness (review P1-2/§23)
    if res_file.is_file() and not force:
        cached = jload(res_file)
        cur = experiment_inputs()
        if (cached.get("experiment_inputs", {}).get("combined_sha256")
                == cur["combined_sha256"]
                and cached.get("experiment_inputs", {}).get("ffmpeg_pin")
                == cur["ffmpeg_pin"]
                and cached.get("stage_identity_sha256")
                == stage_identity(stage, ["F0"], {flt})):
            print(f"[{stage}] evidence fresh, skipping")
            return cached
        print(f"[{stage}] cached evidence STALE, rerunning")

    # base = codec + F0; probe filter adds ONLY its configure-required deps
    d = stage_dir(stage)

    def build_and_smoke():
        ship_manifest = projected_manifest(stage, "shipping")
        ship_map = d / "shipping.map"
        xmake_build(f"build/minimize/{stage}/manifest-shipping.json",
                    gc=True, lto=False, link_map=ship_map)
        sizes = binary_sizes(PROBE, d, "probe")
        scenario = write_scenario(
            d / "scenario.kv",
            scenario_checks(["F0"], stage_filters_now(), extra_filter=flt))
        smoke = run_probe(scenario, d / "smoke-shipping.json")
        return projected_manifest, sizes, smoke

    def stage_filters_now():
        return set(jload(d / "profile.json")["components"].get("filter", []))

    # ---- pass A: DIRECT closure ----
    write_profile(stage, ["F0"], extra_filters={flt})
    run_oracle(stage, ["F0"], force=force, rewrite_profile=False)
    manifest_a = jload(d / "manifest.json")
    _, sizes_a, smoke_a = build_and_smoke()
    failures = graph_config_failures(smoke_a)

    # ---- pass B (only on discovery): minimal format adaptation ----
    adaptation = bool(failures)
    if adaptation:
        print(f"[{stage}] discovered format-adaptation requirement "
              f"({[f['id'] for f in failures]}) -> adding aresample+swresample")
        write_profile(stage, ["F0"], extra_filters={flt}, adaptation=True)
        run_oracle(stage, ["F0"], force=True, rewrite_profile=False)
        _, sizes, smoke = build_and_smoke()
        manifest = jload(d / "manifest.json")
    else:
        sizes, smoke, manifest = sizes_a, smoke_a, manifest_a

    config_ev = parse_config_evidence(stage)
    stage_filters = set(jload(d / "profile.json")["components"].get("filter", []))

    # multi-input functional status (review P1-1): graph2 rows actually ran
    # for afir/acrossfade/headphone; anything else is typed honestly
    mi_rows = [r for r in smoke.get("results", [])
               if r.get("kind") == "graph2" and r.get("id") == f"mi_{flt}"]
    if mi_rows:
        mi_status = ("FUNCTIONAL_SMOKE_PASS (2-input graph2: config+process"
                     "+drain+finite+non-empty)" if mi_rows[0].get("ok")
                     else "FUNCTIONAL_SMOKE_FAIL")
    elif flt in MULTI_INPUT_SMOKE:
        mi_status = "REGISTRATION_PASS|CLOSURE_PASS|FUNCTIONAL_SMOKE_NOT_RUN"
    else:
        mi_status = None  # single-input filter: normal graph rows cover it

    base1 = jload(RESULTS / "f0-framework.json")
    tx_base = tx_units(jload(stage_dir("avf-c1") / "manifest.json"))
    tx_direct, tx_b = tx_units(manifest_a), tx_units(manifest)
    # first FFT/avtx user = brings transform foundation the F0 base lacks
    fft_first_user = len(tx_b) > len(tx_base)
    fft_foundation_in_direct = len(tx_direct) > len(tx_base)
    delta_eff = delta_stage(base1, {
        "manifest": {"units": manifest["closure"]["translation_units"]},
        "shipping": {"archive_bytes": ARCHIVE.stat().st_size,
                     "probe_sizes": sizes}})
    data = {
        "stage": stage,
        "probe_filter": flt,
        "filters": sorted(stage_filters),
        "stage_identity_sha256": stage_identity(stage, ["F0"], {flt}),
        "experiment_inputs": experiment_inputs(),
        "source": git_info(),
        "config_evidence": config_ev,
        "discovery": {
            "configure_required_libs": configure_required_libs(
                stage_filters - {"aresample"}),
            "direct_closure": {
                "manifest_units": manifest_a["closure"]["translation_units"],
                "manifest_per_library": per_library_units(manifest_a),
                "profile_sha256": manifest_a["profile_sha256"],
                "graph_config_failures": [f["id"] for f in failures],
                "graph_error_sample": next(
                    (f.get("error") for f in failures), None),
                "xz_bytes": sizes_a["xz_bytes"],
                "usable_with_canonical_flt_input": not failures,
            },
            "format_adaptation": {
                "required": adaptation,
                "added": ["aresample"] if adaptation else [],
                "added_libs": ["libswresample"] if adaptation else [],
                "class": ("graph-format-adaptation" if adaptation
                          else "none"),
                "discovered_by": ("avfilter_graph_config failure on "
                                  "canonical flt input") if adaptation
                                 else None,
            },
            "effective_closure": {
                "manifest_units": manifest["closure"]["translation_units"],
                "manifest_per_library": per_library_units(manifest),
                "auto_inserted": sorted({a for g in graph_evidence(smoke)
                                         for a in g["auto_inserted"]}),
                "xz_bytes": sizes["xz_bytes"],
            },
        },
        "cost_split": {
            "direct_xz_bytes": sizes_a["xz_bytes"],
            "adaptation_xz_bytes": sizes["xz_bytes"] - sizes_a["xz_bytes"],
            "adaptation_units": manifest["closure"]["translation_units"]
                                - manifest_a["closure"]["translation_units"],
            "note": "direct = codec+F0+filter(+configure-required deps); "
                    "adaptation = discovered minimal conversion capability; "
                    "the old +xz figure mixed both",
        },
        "fft_foundation": {
            "first_user": fft_first_user,
            "foundation_in_direct_closure": fft_foundation_in_direct,
            "units": tx_b,
            "note": ("first FFT/avtx user: its DIRECT closure already "
                     "carries the shared transform foundation; later FFT "
                     "users pay only the incremental filter cost") if
                    fft_first_user else "no new FFT foundation units",
        },
        "multi_input_status": mi_status,
        "manifest_units": manifest["closure"]["translation_units"],
        "manifest_per_library": per_library_units(manifest),
        "shipping": {"archive_bytes": ARCHIVE.stat().st_size,
                     "probe_sizes": sizes,
                     "ldd": ldd_probe()},
        "smoke_verdict": smoke["summary"]["verdict"],
        "gate_verdict": probe_verdict({
            "graph_evidence": graph_evidence(smoke)}),
        "delta_vs_avf-c1": delta_eff,
        "graph_evidence": graph_evidence(smoke),
    }
    jwrite(res_file, data)
    return data


def delta_stage(prev: dict, cur: dict) -> dict:
    pv = prev["manifest"]["units"]
    cv = cur["manifest"]["units"]
    ps = prev["shipping"]["probe_sizes"]
    cs = cur["shipping"]["probe_sizes"]
    return {
        "units": cv - pv,
        "stripped_bytes": cs["stripped_bytes"] - ps["stripped_bytes"],
        "xz_bytes": cs["xz_bytes"] - ps["xz_bytes"],
    }


def graph_evidence(smoke: dict) -> list:
    """Which graphs required graph-inserted conversion / what formats won.
    Includes graph2 (multi-input functional) rows — they carry gate
    weight: a multi-input filter must pass its functional smoke, not
    just registration (review P1-1)."""
    out = []
    for r in smoke["results"]:
        if r.get("kind") not in ("graph", "graph2"):
            continue
        out.append({"id": r["id"], "kind": r.get("kind"),
                    "chain": r["chain"],
                    "auto_inserted": r.get("auto_inserted", []),
                    "negotiated": r.get("negotiated"),
                    "ok": r["ok"]})
    return out


# --------------------------------------------------------------------------
# manifest union (task §11/§12)
# --------------------------------------------------------------------------

def run_union() -> dict:
    import ffmpeg_manifest_union as fmu
    data = fmu.union(stage_dir("avf-c0") / "manifest.json",
                     stage_dir("avf-c8") / "manifest.json")
    # task §18: the SAME predicate as the wrapper, including the pin check
    data["verdict"] = fmu.union_verdict(data)
    data["experiment_inputs"] = experiment_inputs()
    jwrite(RESULTS / "manifest-union.json", data)
    return data


# --------------------------------------------------------------------------
# aggregation + report
# --------------------------------------------------------------------------

def load_stage_data() -> list:
    caps = caps_doc()
    out = []
    for stage, tier, tiers in ladder():
        f = stage_result_file(stage)
        if f.is_file():
            out.append(jload(f))
    return out


def load_probe_data() -> list:
    return [jload(RESULTS / f"probe-{f}.json") for f in PROBE_FILTERS
            if (RESULTS / f"probe-{f}.json").is_file()]


def compute_summary(stages: list, probes: list) -> dict:
    """Derive the fail-closed summary from RAW evidence (review P0-2).
    Gate predicates are recomputed here from section fields; serialized
    verdict strings are never trusted."""
    # 1. ladder stage gates — recomputed
    stage_gates = {}
    for st in stages:
        fresh = gates_for(st)
        stage_gates[st["stage"]] = {k: g.get("verdict")
                                    for k, g in fresh.items()}
    ladder_ok = all(all(v == "PASS" for v in g.values())
                    for g in stage_gates.values())

    # 2. add-one probe gates — recomputed from graph evidence
    probe_gates = {p["probe_filter"]: probe_verdict(p) for p in probes}
    probes_ok = all(v == "PASS" for v in probe_gates.values())

    # 3. manifest union verdict (shared predicate incl. pin)
    ufile = RESULTS / "manifest-union.json"
    union_data = jload(ufile) if ufile.is_file() else {
        "verdict": "FAIL", "validations": {}}
    union_ok = union_data.get("verdict") == "PASS"
    pin_ok = bool(union_data.get("validations", {})
                  .get("combined_same_ffmpeg_pin"))
    cur_pin = jload(ROOT / "bench" / "ffmpeg-pin.json")
    pin_ok = pin_ok and all(
        s.get("experiment_inputs", {}).get("ffmpeg_pin") == cur_pin
        for s in stages) if stages else False

    # 4. license policy: every avfilter-bearing stage stays expected LGPL
    license_rows = {st["stage"]: (st.get("config_evidence") or {})
                    .get("license") for st in stages}
    license_ok = bool(stages) and all(
        l and l.startswith(EXPECTED_LICENSE_PREFIX)
        for l in license_rows.values())

    # 5. external dependency policy: no optional externals enabled, and the
    # linked probe's dynamic deps stay inside the allowed libc/libm set
    externals = {st["stage"]: (st.get("config_evidence") or {})
                 .get("external_libs_enabled", []) for st in stages}
    ldd_ok = True
    unexpected_libs = {}
    for st in stages:
        for dep in (st.get("shipping") or {}).get("ldd", []):
            if dep.get("lib") not in ALLOWED_DYNAMIC_LIBS:
                unexpected_libs.setdefault(st["stage"], []).append(dep["lib"])
        if unexpected_libs:
            ldd_ok = False
    deps_ok = (not any(externals.values())) and ldd_ok

    marginal = {"experiment": "e10-c0", "ladder": [], "probes": []}
    c1_xz = next((st["shipping"]["probe_sizes"]["xz_bytes"]
                  for st in stages if st["stage"] == "avf-c1"), None)
    prev = None
    for st in stages:
        row = {
            "stage": st["stage"], "tier": st.get("tier"),
            "capabilities": st["capabilities"],
            "filters": st["registration_gate"]["intended_filters"],
            "units": st["manifest"]["units"],
            "units_delta": None if prev is None else
                st["manifest"]["units"] - prev["manifest"]["units"],
            "stripped_bytes": st["shipping"]["probe_sizes"]["stripped_bytes"],
            "stripped_delta": None if prev is None else
                st["shipping"]["probe_sizes"]["stripped_bytes"] - prev["shipping"]["probe_sizes"]["stripped_bytes"],
            "xz_bytes": st["shipping"]["probe_sizes"]["xz_bytes"],
            "xz_delta": None if prev is None else
                st["shipping"]["probe_sizes"]["xz_bytes"] - prev["shipping"]["probe_sizes"]["xz_bytes"],
            "live_units": st["shipping"]["live"]["live_units"],
            "live_bytes": st["shipping"]["live"]["live_bytes_total"],
            "new_external_libs": st["config_evidence"]["external_libs_enabled"],
            "format_adaptation": st.get("format_adaptation", {}).get("required", False),
            "direct_units": st.get("direct_closure", {}).get("manifest_units"),
        }
        marginal["ladder"].append(row)
        prev = st
    for p in probes:
        d = p.get("discovery", {})
        eff = d.get("effective_closure", {})
        marginal["probes"].append({
            "filter": p["probe_filter"],
            "units": p["manifest_units"],
            "units_delta_vs_c1": p["delta_vs_avf-c1"]["units"],
            "stripped_delta_vs_c1": p["delta_vs_avf-c1"]["stripped_bytes"],
            "xz_delta_vs_c1": p["delta_vs_avf-c1"]["xz_bytes"],
            "direct_delta_xz_vs_c1": (d.get("direct_closure", {}).get(
                "xz_bytes") - c1_xz) if c1_xz else None,
            "adaptation_xz_bytes": p.get("cost_split", {}).get(
                "adaptation_xz_bytes"),
            "adaptation_class": d.get("format_adaptation", {}).get("class"),
            "auto_inserted_conversion": eff.get("auto_inserted", []),
            "fft_first_user": p.get("fft_foundation", {}).get("first_user"),
            "fft_units": p.get("fft_foundation", {}).get("units", []),
            "multi_input_status": p.get("multi_input_status"),
            "gate": probe_verdict(p),
        })

    predicates = {
        "ladder_stage_gates": "PASS" if ladder_ok else "FAIL",
        "add_one_probe_gates": "PASS" if probes_ok else "FAIL",
        "manifest_union": "PASS" if union_ok else "FAIL",
        "ffmpeg_pin_equal": "PASS" if pin_ok else "FAIL",
        "license_policy": "PASS" if license_ok else "FAIL",
        "external_dependency_policy": "PASS" if deps_ok else "FAIL",
    }
    all_pass = all(v == "PASS" for v in predicates.values())
    summary = {
        "experiment": "e10-c0-libavfilter-capability-min",
        "predicates": predicates,
        "verdict": "PASS" if all_pass else "FAIL",
        "verdict_semantics": "PASS requires: every ladder stage gate, every "
                             "add-one probe gate, manifest-union verdict, "
                             "FFmpeg pin equality, license policy and "
                             "external-dependency policy — all derived from "
                             "raw evidence, never from stored verdicts",
        "stages": stage_gates,
        "probe_gates": probe_gates,
        "multi_input_filters": {p["probe_filter"]: p.get("multi_input_status")
                                for p in probes
                                if p.get("multi_input_status")},
        "ladder": marginal["ladder"],
        "probes": marginal["probes"],
        "union": union_data,
        "license": license_rows,
        "unexpected_dynamic_libs": unexpected_libs,
        "scope_statements": {
            "production_code_changed": False,
            "production_build_semantics_changed": False,
            "dsp_backend_selected": False,
            "pr_merged": False,
        },
        # provenance authority = input hashes (review §24): git pointers are
        # supplementary only and live in the raw per-stage results recorded
        # at measurement time; embedding HEAD here made the derived summary
        # self-invalidate on every commit
        "provenance": {
            "experiment_inputs": experiment_inputs(),
            "host": platform.platform(),
        },
    }
    return {"marginal": marginal, "summary": summary}


def compute_aggregate() -> dict:
    """Pure recomputation of every derived authority object. No writes."""
    stages = load_stage_data()
    probes = load_probe_data()
    out = compute_summary(stages, probes)
    marginal, summary = out["marginal"], out["summary"]

    shipping = {"experiment": "e10-c0", "method": "final linked product-shaped "
                "probe (qn_avfilter_cap_probe); stripped + xz -9; archive bytes "
                "diagnostic only", "stages": [
                    {"stage": st["stage"],
                     "archive_bytes": st["shipping"]["archive_bytes"],
                     "linked_raw": st["shipping"]["probe_sizes"]["raw_bytes"],
                     "stripped": st["shipping"]["probe_sizes"]["stripped_bytes"],
                     "xz": st["shipping"]["probe_sizes"]["xz_bytes"],
                     "lto": st.get("lto")} for st in stages]}
    live_doc = {"experiment": "e10-c0", "note": "compiled closure vs linked "
                "live closure (gc-sections link map); live units <= compiled",
                "stages": [{"stage": st["stage"],
                            "compiled_units": st["manifest"]["units"],
                            "live_units": st["shipping"]["live"]["live_units"],
                            "live_bytes": st["shipping"]["live"]["live_bytes_total"],
                            "discarded_bytes": st["shipping"]["live"]["discarded_bytes_total"],
                            "compiled_by_library": st["shipping"]["live"]["compiled_units_by_library"],
                            "live_by_library": st["shipping"]["live"]["live_units_by_library"]}
                           for st in stages]}
    smoke = {"experiment": "e10-c0", "stages": [
        {"stage": st["stage"],
         "plain": {"verdict": st["plain"]["smoke_verdict"],
                   "fail": st["plain"]["smoke_fail"]},
         "shipping": {"verdict": st["shipping"]["smoke_verdict"],
                      "fail": st["shipping"]["smoke_fail"]},
         "auto_inserted_anywhere": sorted({a for r in
             st["smoke_detail"]["shipping"]["results"] if r.get("kind") in ("graph", "graph2")
             for a in r.get("auto_inserted", [])}),
         "negotiated_formats": {r["id"]: r["negotiated"] for r in
             st["smoke_detail"]["shipping"]["results"] if r.get("kind") in ("graph", "graph2")}}
        for st in stages]}
    prov = {"experiment": "e10-c0",
            "filters": dependency_provenance(stages, probes),
            "notes": "classification per filter: configure-required (dep rule "
                     "in pinned configure), graph-required (auto-inserted at "
                     "graph config), format-adaptation (discovered by graph "
                     "config failure), shared-foundation, external-optional"}
    caps = caps_doc()
    pin = jload(ROOT / "bench" / "ffmpeg-pin.json")
    cap_manifest = {
        "version": caps["version"],
        "experiment": "e10-c0",
        "capability_manifest": "bench/dsp-capabilities.json",
        "capability_manifest_sha256": sha256_file(CAPS),
        "ffmpeg": pin,
        "stages": {st["stage"]: {
            "filters": st["registration_gate"]["intended_filters"],
            "license": st["config_evidence"]["license"],
            "units": st["manifest"]["units"],
            "gates": {k: g.get("verdict") for k, g in st["gates"].items()},
        } for st in stages},
    }
    return {"marginal": marginal, "summary": summary, "shipping": shipping,
            "live_doc": live_doc, "smoke": smoke, "prov": prov,
            "cap_manifest": cap_manifest}


def aggregate() -> dict:
    out = compute_aggregate()
    jwrite(RESULTS / "marginal-cost.json", out["marginal"])
    jwrite(RESULTS / "shipping.json", out["shipping"])
    jwrite(RESULTS / "live-sections.json", out["live_doc"])
    jwrite(RESULTS / "correctness-smoke.json", out["smoke"])
    jwrite(RESULTS / "dependency-provenance.json", out["prov"])
    jwrite(RESULTS / "capability-manifest.json", out["cap_manifest"])
    jwrite(RESULTS / "summary.json", out["summary"])
    print(f"summary written (verdict {out['summary']['verdict']})")
    return out["summary"]


def dependency_provenance(stages: list, probes: list) -> dict:
    caps = caps_doc()
    conv_by_graph = {}
    for s in stages:
        for r in s["smoke_detail"]["shipping"]["results"]:
            if r.get("kind") == "graph":
                conv_by_graph[r["id"]] = r.get("auto_inserted", [])
    ladder_conv = sorted({a for conv in conv_by_graph.values() for a in conv})
    out = {}
    for t in caps["tiers"]:
        for pc in t["product_capabilities"]:
            for f in pc.get("filters", []):
                cl = ["product-required"]
                if f == "aresample":
                    cl = ["graph-required: auto-inserted when a link's format "
                          "lists cannot merge (pinned formats.c conversion_filter="
                          "aresample; configure aresample_filter_deps=swresample)"]
                out[f] = {"tier": t["id"], "capability": pc["capability"],
                          "classifications": cl}
    for p in probes:
        f = p["probe_filter"]
        if f not in out:
            continue
        conv = sorted({a for g in p["graph_evidence"] for a in g["auto_inserted"]})
        if conv:
            out[f]["classifications"].append(
                f"format-adaptation: probe graph inserted {conv}")
        else:
            out[f]["classifications"].append(
                "no graph-inserted conversion observed in linear flt chain")
        out[f]["probe_stage"] = p["stage"]
    for e in caps["external_optional"]:
        out[e["filter"]] = {"tier": None, "capability": e["product_capability"],
                            "classifications": [f"external-optional ({e['external']})"]}
    for c in caps["considered_not_enabled"]:
        out[c["filter"]] = {"tier": None, "capability": None,
                            "classifications": [f"not-justified: {c['reason']}"]}
    out["_ladder_graph_inserted"] = {
        "auto_inserted_anywhere": ladder_conv,
        "per_graph": conv_by_graph,
        "shared_foundation": {
            "swresample": "enters at F1 via aresample (format adaptation) and "
                          "pan (configure dep); it is shared foundation for "
                          "every later tier, not a per-filter cost",
        },
    }
    return out


# --------------------------------------------------------------------------
# report tables (generated; doc must not carry hand-copied numbers)
# --------------------------------------------------------------------------

def fmt_int(n) -> str:
    return "-" if n is None else f"{n:,}"


def render_tables() -> str:
    summary = jload(RESULTS / "summary.json")
    union = summary.get("union") or {}
    lines = []
    lines.append("### 边际成本阶梯（machine authority：`summary.json.ladder`）")
    lines.append("")
    lines.append("| stage | tier | filters | compiled TU | +TU | live TU | live bytes | stripped | +stripped | xz | +xz |")
    lines.append("|---|---|---|---:|--:|--:|--:|--:|--:|--:|--:|")
    for r in summary["ladder"]:
        lines.append(
            f"| {r['stage']} | {r.get('tier') or 'codec-only'} "
            f"| {len(r['filters'])} | {r['units']} | {fmt_int(r['units_delta'])} "
            f"| {r['live_units']} | {fmt_int(r['live_bytes'])} "
            f"| {fmt_int(r['stripped_bytes'])} | {fmt_int(r['stripped_delta'])} "
            f"| {fmt_int(r['xz_bytes'])} | {fmt_int(r['xz_delta'])} |")
    lines.append("")
    lines.append("### 单项探针（base = codec + F0；machine authority：`summary.json.probes`）")
    lines.append("")
    lines.append("| probe | +TU vs c1 | direct Δxz vs c1 | adaptation Δxz | "
                 "effective Δxz vs c1 | 适配类别 | 图内自动插入转换 | gate |")
    lines.append("|---|--:|--:|--:|--:|---|---|---|")
    for p in summary["probes"]:
        conv = ",".join(p["auto_inserted_conversion"]) or "none"
        lines.append(
            f"| {p['filter']} | {p['units_delta_vs_c1']} "
            f"| {fmt_int(p.get('direct_delta_xz_vs_c1'))} "
            f"| {fmt_int(p.get('adaptation_xz_bytes'))} "
            f"| {fmt_int(p['xz_delta_vs_c1'])} "
            f"| {p.get('adaptation_class')} | {conv} | {p.get('gate')} |")
    lines += ["",
              "读法：`direct xz` = 只含 filter 本体 + configure-required "
              "依赖的闭包（适配失败时的可用性见 gate）；`adaptation xz` = "
              "由图实例化失败**发现**的最小转换能力（aresample+swresample）"
              "增量。两者不得合并为一个模糊边际数。",
              "",
              "多输入 filter（afir/acrossfade/headphone）由 2-input graph2 "
              "功能性 smoke 覆盖（config+process+drain+finite+non-empty）。",
              ""]
    lines.append("### 闭包记账（machine authority：`manifest-union.json`）")
    lines.append("")
    lines.append(f"| codec-only | filter-only | shared | combined | 配置宏差异数 | 校验 |")
    lines.append("|--:|--:|--:|--:|--:|---|")
    v = union.get("validations", {})
    lines.append(f"| {union.get('codec_only_units')} | {union.get('filter_only_units')} "
                 f"| {union.get('shared_units')} | {union.get('combined_units')} "
                 f"| {union.get('config_macro_diff_count')} "
                 f"| codec⊆combined={v.get('codec_subset_of_combined')} "
                 f"flag conflicts={v.get('shared_flag_conflicts')} |")
    lines.append("")
    lines.append("### gate 总表（machine authority：`summary.json.stages`）")
    lines.append("")
    lines.append("| stage | gates |")
    lines.append("|---|---|")
    for st, gd in summary["stages"].items():
        allp = all(v == "PASS" for v in gd.values())
        bad = {k: v for k, v in gd.items() if v != "PASS"}
        lines.append(f"| {st} | {'PASS' if allp else 'FAIL ' + str(bad)} |")
    lines += ["",
              f"**顶层 verdict：{summary['verdict']}** "
              f"（谓词：{'; '.join(f'{k}={v}' for k, v in summary['predicates'].items())}）",
              ""]
    return "\n".join(lines)


MARK_BEGIN = "<!-- BEGIN GENERATED C0 TABLES -->"
MARK_END = "<!-- END GENERATED C0 TABLES -->"


def write_report() -> None:
    text = DOC.read_text()
    begin = text.index(MARK_BEGIN)
    end = text.index(MARK_END)
    text = text[:begin + len(MARK_BEGIN)] + "\n" + render_tables() + text[end:]
    DOC.write_text(text)
    print(f"tables regenerated in {DOC.relative_to(ROOT)}")


def check() -> int:
    """STRICTLY READ-ONLY (review P0-2/§19): recompute every derived
    authority object in memory from raw stage evidence, compare with the
    committed aggregates, validate all top-level predicates and the
    report tables. Never writes a file."""
    ok = True

    def fail(msg):
        nonlocal ok
        ok = False
        print("FAIL:", msg, file=sys.stderr)

    # 0. every result must be bound to the current experiment inputs
    cur = experiment_inputs()
    for f in sorted(RESULTS.glob("*.json")):
        if f.name in ("summary.json", "marginal-cost.json", "shipping.json",
                      "live-sections.json", "correctness-smoke.json",
                      "dependency-provenance.json", "capability-manifest.json",
                      "manifest-union.json"):
            continue
        d = jload(f)
        if d.get("experiment_inputs", {}).get("combined_sha256") \
                != cur["combined_sha256"]:
            fail(f"{f.name}: stale experiment-input hash")

    # 1. stage gates recomputed from raw fields (not stored verdicts)
    stages = load_stage_data()
    if len(stages) < len(list(ladder())):
        fail("missing ladder stage evidence")
    for st in stages:
        fresh = gates_for(st)
        stored = st.get("gates", {})
        for k, g in fresh.items():
            if stored.get(k, {}).get("verdict") != g["verdict"]:
                fail(f"{st['stage']}.gates.{k}: stored "
                     f"{stored.get(k, {}).get('verdict')} != derived "
                     f"{g['verdict']}")

    # 2. derived aggregates recomputed in memory vs committed
    out = compute_aggregate()
    committed = {
        "marginal-cost.json": out["marginal"],
        "shipping.json": out["shipping"],
        "live-sections.json": out["live_doc"],
        "correctness-smoke.json": out["smoke"],
        "dependency-provenance.json": out["prov"],
        "capability-manifest.json": out["cap_manifest"],
        "summary.json": out["summary"],
    }
    for name, want in committed.items():
        have = jload(RESULTS / name)
        if have != want:
            fail(f"{name}: drift vs recomputed authority")

    # 3. top-level predicates must all hold
    for k, v in out["summary"]["predicates"].items():
        if v != "PASS":
            fail(f"predicate {k} = {v}")

    # 4. report tables in sync (in-memory render, no writes)
    text = DOC.read_text()
    begin = text.index(MARK_BEGIN)
    end = text.index(MARK_END)
    have = text[begin + len(MARK_BEGIN):end].strip("\n")
    if have != render_tables().strip("\n"):
        fail("doc tables differ from generated (run --report)")

    if ok:
        print("E10-C0 authority tree: ALL CHECKS PASS (read-only)")
        return 0
    return 1


# --------------------------------------------------------------------------
# main
# --------------------------------------------------------------------------

def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--stage")
    ap.add_argument("--force", action="store_true")
    ap.add_argument("--probes", action="store_true")
    ap.add_argument("--union", action="store_true")
    ap.add_argument("--aggregate", action="store_true")
    ap.add_argument("--report", action="store_true")
    ap.add_argument("--check", action="store_true")
    args = ap.parse_args()

    if args.check:
        return check()   # read-only: never aggregates/writes
    if args.report:
        write_report()
        return 0
    if args.union:
        run_union()
        return 0
    if args.aggregate:
        aggregate()
        return 0

    RESULTS.mkdir(parents=True, exist_ok=True)
    if args.stage:
        m = re.fullmatch(r"avf-p-(.+)", args.stage)
        if m:
            run_probe_stage(m.group(1), force=args.force)
        else:
            tier_map = {s: tl for s, _, tl in ladder()}
            if args.stage not in tier_map:
                raise SystemExit(f"unknown stage {args.stage}; ladder = {list(tier_map)}")
            run_stage(args.stage, tier_map[args.stage], force=args.force)
        return 0
    if args.probes:
        for f in PROBE_FILTERS:
            run_probe_stage(f, force=args.force)
        return 0

    for stage, _, tiers in ladder():
        run_stage(stage, tiers, force=args.force)
    return 0


if __name__ == "__main__":
    sys.exit(main())
