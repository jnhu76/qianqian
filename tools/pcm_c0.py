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
PROBE_NEEDS_ARESAMPLE = {f: True for f in PROBE_FILTERS}
PROBE_NEEDS_ARESAMPLE["atempo"] = False  # packed-flt candidate; graph proves it
LTO_STAGES = {"avf-c0", "avf-c1", "avf-c4", "avf-c8"}

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


def derive_profile(stage: str, tier_ids: list) -> dict:
    base = jload(CODEC_PROFILE)
    caps = caps_doc()
    by_id = {t["id"]: t for t in caps["tiers"]}
    p = json.loads(json.dumps(base))  # deep copy
    p["profile"] = stage
    p["experiment"] = "e10-c0"
    p["derived_from"] = CODEC_PROFILE.name
    p["description"] = f"E10-C0 ladder: codec closure + tiers {tier_ids or '[]'}"
    filters: set = set()
    aresample = swr = False
    for tid in tier_ids:
        t = by_id[tid]
        filters |= set(t["filters"])
        aresample |= bool(t["aresample"])
        swr |= bool(t["swresample"])
    enable = list(p["libraries"]["enable"])
    disable = list(p["libraries"]["disable"])
    if tier_ids:
        enable += ["avfilter"] + (["swresample"] if swr else [])
        disable = [x for x in disable if x not in ("avfilter", "swresample")]
        # --disable-everything only disables components; libraries stay on by
        # default, so swresample must stay explicitly disabled when no tier
        # declares the format-adaptation foundation (c1 leak: 9 swr TUs)
        if not swr:
            disable.append("swresample")
        if aresample:
            filters.add("aresample")
    p["libraries"] = {"enable": sorted(set(enable)), "disable": sorted(set(disable))}
    p.setdefault("components", {})["filter"] = sorted(filters)
    return p


def write_profile(stage: str, tier_ids: list) -> Path:
    d = stage_dir(stage)
    d.mkdir(parents=True, exist_ok=True)
    pf = d / "profile.json"
    pf.write_text(json.dumps(derive_profile(stage, tier_ids), indent=1) + "\n")
    return pf


# --------------------------------------------------------------------------
# oracle (upstream configure/Make, import-time only)
# --------------------------------------------------------------------------

def run_oracle(stage: str, tier_ids: list, force: bool = False,
               rewrite_profile: bool = True) -> None:
    d = stage_dir(stage)
    if (d / "manifest.json").is_file() and not force:
        print(f"[{stage}] oracle manifest exists, skipping import")
        return
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
    cfg = ["xmake", "f", "-o", BUILDIR, "-m", "release",
           f"--av_manifest={manifest_rel}", "-y"]
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

def sig(name, freq=1000.0, amp=0.5):
    return f"{name}:{freq}:{amp}"


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
        ("F2", "f2_alimiter", "alimiter=limit=0.5", sig("noise", 0, 0.9), 48000, 2, 48000,
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
    return checks


def write_scenario(path: Path, checks: list) -> Path:
    lines = []
    for c in checks:
        if c["kind"] in ("present", "absent"):
            lines.append(f"kind={c['kind']} name={c['name']}")
            continue
        parts = [f"kind=graph", f"id={c['id']}", f"chain={c['chain']}",
                 f"signal={c['signal']}", f"rate={c['rate']}", f"ch={c['ch']}",
                 f"frames={c['frames']}", f"block={c['block']}"]
        for k in ("sink_fmt", "want_fmt", "want_rate", "want_channels",
                  "expect", "lifecycle"):
            if k in c:
                parts.append(f"{k}={c[k]}")
        lines.append(" ".join(parts))
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("\n".join(lines) + "\n")
    return path


# --------------------------------------------------------------------------
# one ladder/probe stage end-to-end
# --------------------------------------------------------------------------

def run_stage(stage: str, tier_ids: list, force: bool = False) -> dict:
    d = stage_dir(stage)
    res_file = stage_result_file(stage)
    manifest_rel = f"build/minimize/{stage}/manifest.json"
    if res_file.is_file() and not force:
        print(f"[{stage}] evidence exists ({res_file.name}), skipping")
        return jload(res_file)

    run_oracle(stage, tier_ids, force=force)
    manifest = jload(d / "manifest.json")
    config_ev = parse_config_evidence(stage)

    profile = jload(d / "profile.json")
    stage_filters = set(profile["components"].get("filter", []))
    tier_ids_eff = list(tier_ids)

    # registration gates from generated provenance + config
    reg_gate = {
        "intended_filters": sorted(stage_filters),
        "config_enabled": config_ev["filters_enabled_config"],
        "filter_list_registered": config_ev["filter_list_registered"],
        "always_present": sorted(FILTER_ALWAYS_PRESENT),
        "intended_subset_of_config": (
            stage_filters - FILTER_ALWAYS_PRESENT
            <= set(config_ev["filters_enabled_config"])),
        "filter_list_matches_config": (
            set(config_ev["filter_list_registered"])
            == set(config_ev["filters_enabled_config"]) | FILTER_ALWAYS_PRESENT),
    }

    # ---- xmake replay: plain ----
    plain_map = d / "plain.map"
    no_avf = stage == "avf-c0"
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

    scenario = write_scenario(d / "scenario.kv",
                              scenario_checks(tier_ids_eff, stage_filters))
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
    live = parse_live_map(ship_map, members, jload(d / "manifest-shipping.json"))

    # ---- xmake replay: lto (representative points only) ----
    lto_data = None
    if stage in LTO_STAGES:
        projected_manifest(stage, "lto")
        xmake_build(f"build/minimize/{stage}/manifest-lto.json", gc=True,
                    lto=True, no_avfilter=no_avf)
        lto_data = {"sizes": binary_sizes(PROBE, d, "probe-lto"),
                    "archive_bytes": ARCHIVE.stat().st_size}

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
        "smoke_detail": {"plain": smoke_plain, "shipping": smoke_ship},
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
    return g


# --------------------------------------------------------------------------
# add-one probes
# --------------------------------------------------------------------------

def run_probe_stage(flt: str, force: bool = False) -> dict:
    stage = f"avf-p-{flt}"
    res_file = stage_result_file(stage)
    if res_file.is_file() and not force:
        print(f"[{stage}] evidence exists, skipping")
        return jload(res_file)

    # base = codec + F0; probe = base + single filter (+declared adaptation)
    base_tiers = ["F0"]
    d = stage_dir(stage)
    pf = write_profile(stage, base_tiers)
    p = jload(pf)
    p["profile"] = stage
    p["components"]["filter"] = sorted(set(p["components"]["filter"]) | {flt})
    if PROBE_NEEDS_ARESAMPLE.get(flt, True):
        p["components"]["filter"] = sorted(set(p["components"]["filter"]) | {"aresample"})
        libs = set(p["libraries"]["enable"]) | {"swresample"}
        p["libraries"] = {"enable": sorted(libs),
                          "disable": [x for x in p["libraries"]["disable"]
                                      if x not in ("avfilter", "swresample")]}
    pf.write_text(json.dumps(p, indent=1) + "\n")

    run_oracle(stage, base_tiers, force=force, rewrite_profile=False)
    manifest = jload(d / "manifest.json")
    config_ev = parse_config_evidence(stage)
    stage_filters = set(p["components"]["filter"])

    # shipping authority only (marginal delta = shipping vs avf-c1 shipping)
    ship_manifest = projected_manifest(stage, "shipping")
    ship_map = d / "shipping.map"
    xmake_build(f"build/minimize/{stage}/manifest-shipping.json", gc=True,
                lto=False, link_map=ship_map)
    sizes = binary_sizes(PROBE, d, "probe")
    smoke = run_probe(write_scenario(d / "scenario.kv",
                                     scenario_checks(base_tiers, stage_filters,
                                                     extra_filter=flt)),
                      d / "smoke-shipping.json")
    members = archive_members(ARCHIVE)
    live = parse_live_map(ship_map, members, jload(d / "manifest-shipping.json"))

    base1 = jload(RESULTS / "f0-framework.json")
    data = {
        "stage": stage,
        "probe_filter": flt,
        "declared_needs_aresample": PROBE_NEEDS_ARESAMPLE.get(flt, True),
        "filters": sorted(stage_filters),
        "config_evidence": config_ev,
        "manifest_units": manifest["closure"]["translation_units"],
        "manifest_per_library": per_library_units(manifest),
        "shipping": {"archive_bytes": ARCHIVE.stat().st_size,
                     "probe_sizes": sizes,
                     "live": live},
        "smoke_verdict": smoke["summary"]["verdict"],
        "delta_vs_avf-c1": delta_stage(base1, {
            "manifest": {"units": manifest["closure"]["translation_units"]},
            "shipping": {"archive_bytes": ARCHIVE.stat().st_size,
                         "probe_sizes": sizes},
        }),
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
    """Which graphs required graph-inserted conversion / what formats won."""
    out = []
    for r in smoke["results"]:
        if r.get("kind") != "graph":
            continue
        out.append({"id": r["id"], "chain": r["chain"],
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
    data["verdict"] = ("PASS" if data["validations"]["codec_subset_of_combined"]
                       and data["validations"]["shared_flag_conflicts"]
                       else "FAIL")
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


def aggregate() -> dict:
    stages = load_stage_data()
    probes = load_probe_data()
    c0 = next((s for s in stages if s["stage"] == "avf-c0"), None)

    marginal = {"experiment": "e10-c0", "ladder": [], "probes": []}
    prev = None
    for s in stages:
        row = {
            "stage": s["stage"], "tier": s.get("tier"),
            "capabilities": s["capabilities"],
            "filters": s["registration_gate"]["intended_filters"],
            "units": s["manifest"]["units"],
            "units_delta": None if prev is None else
                s["manifest"]["units"] - prev["manifest"]["units"],
            "stripped_bytes": s["shipping"]["probe_sizes"]["stripped_bytes"],
            "stripped_delta": None if prev is None else
                s["shipping"]["probe_sizes"]["stripped_bytes"] - prev["shipping"]["probe_sizes"]["stripped_bytes"],
            "xz_bytes": s["shipping"]["probe_sizes"]["xz_bytes"],
            "xz_delta": None if prev is None else
                s["shipping"]["probe_sizes"]["xz_bytes"] - prev["shipping"]["probe_sizes"]["xz_bytes"],
            "live_units": s["shipping"]["live"]["live_units"],
            "live_bytes": s["shipping"]["live"]["live_bytes_total"],
            "new_external_libs": s["config_evidence"]["external_libs_enabled"],
        }
        marginal["ladder"].append(row)
        prev = s
    for p in probes:
        marginal["probes"].append({
            "filter": p["probe_filter"],
            "units": p["manifest_units"],
            "units_delta_vs_c1": p["delta_vs_avf-c1"]["units"],
            "stripped_delta_vs_c1": p["delta_vs_avf-c1"]["stripped_bytes"],
            "xz_delta_vs_c1": p["delta_vs_avf-c1"]["xz_bytes"],
            "auto_inserted_conversion": sorted({a for g in p["graph_evidence"]
                                                for a in g["auto_inserted"]}),
            "smoke": p["smoke_verdict"],
        })
    jwrite(RESULTS / "marginal-cost.json", marginal)

    shipping = {"experiment": "e10-c0", "method": "final linked product-shaped "
                "probe (qn_avfilter_cap_probe); stripped + xz -9; archive bytes "
                "diagnostic only", "stages": [
                    {"stage": s["stage"],
                     "archive_bytes": s["shipping"]["archive_bytes"],
                     "linked_raw": s["shipping"]["probe_sizes"]["raw_bytes"],
                     "stripped": s["shipping"]["probe_sizes"]["stripped_bytes"],
                     "xz": s["shipping"]["probe_sizes"]["xz_bytes"],
                     "lto": s.get("lto")} for s in stages]}
    jwrite(RESULTS / "shipping.json", shipping)

    live_doc = {"experiment": "e10-c0", "note": "compiled closure vs linked "
                "live closure (gc-sections link map); live units <= compiled",
                "stages": [{"stage": s["stage"],
                            "compiled_units": s["manifest"]["units"],
                            "live_units": s["shipping"]["live"]["live_units"],
                            "live_bytes": s["shipping"]["live"]["live_bytes_total"],
                            "discarded_bytes": s["shipping"]["live"]["discarded_bytes_total"],
                            "compiled_by_library": s["shipping"]["live"]["compiled_units_by_library"],
                            "live_by_library": s["shipping"]["live"]["live_units_by_library"]}
                           for s in stages]}
    jwrite(RESULTS / "live-sections.json", live_doc)

    smoke = {"experiment": "e10-c0", "stages": [
        {"stage": s["stage"],
         "plain": {"verdict": s["plain"]["smoke_verdict"],
                   "fail": s["plain"]["smoke_fail"]},
         "shipping": {"verdict": s["shipping"]["smoke_verdict"],
                      "fail": s["shipping"]["smoke_fail"]},
         "auto_inserted_anywhere": sorted({a for r in
             s["smoke_detail"]["shipping"]["results"] if r.get("kind") == "graph"
             for a in r.get("auto_inserted", [])}),
         "negotiated_formats": {r["id"]: r["negotiated"] for r in
             s["smoke_detail"]["shipping"]["results"] if r.get("kind") == "graph"}}
        for s in stages]}
    jwrite(RESULTS / "correctness-smoke.json", smoke)

    prov = {"experiment": "e10-c0",
            "filters": dependency_provenance(stages, probes),
            "notes": "classification per filter: configure-required (dep rule "
                     "in pinned configure), graph-required (auto-inserted at "
                     "graph config), format-adaptation (native formats differ "
                     "from flt), shared-foundation, external-optional"}
    jwrite(RESULTS / "dependency-provenance.json", prov)

    caps = caps_doc()
    pin = jload(ROOT / "bench" / "ffmpeg-pin.json")
    cap_manifest = {
        "version": caps["version"],
        "experiment": "e10-c0",
        "capability_manifest": "bench/dsp-capabilities.json",
        "capability_manifest_sha256": sha256_file(CAPS),
        "ffmpeg": pin,
        "stages": {s["stage"]: {
            "filters": s["registration_gate"]["intended_filters"],
            "license": s["config_evidence"]["license"],
            "units": s["manifest"]["units"],
            "gates": {k: g["verdict"] for k, g in s["gates"].items()},
        } for s in stages},
    }
    jwrite(RESULTS / "capability-manifest.json", cap_manifest)

    all_gates = []
    for s in stages:
        for k, g in s["gates"].items():
            all_gates.append((s["stage"], k, g.get("verdict")))
    summary = {
        "experiment": "e10-c0-libavfilter-capability-min",
        "verdict": "PASS" if all(v == "PASS" for _, _, v in all_gates) else "FAIL",
        "stages": {s["stage"]: {"gates": {k: g.get("verdict") for k, g in s["gates"].items()}}
                   for s in stages},
        "ladder": marginal["ladder"],
        "probes": marginal["probes"],
        "union": jload(RESULTS / "manifest-union.json") if (RESULTS / "manifest-union.json").is_file() else None,
        "license": stages[-1]["config_evidence"]["license"] if stages else None,
        "scope_statements": {
            "production_code_changed": False,
            "dsp_backend_selected": False,
            "pr_merged": False,
        },
        "provenance": {
            "git_branch": must(["git", "rev-parse", "--abbrev-ref", "HEAD"]).strip(),
            "git_commit": must(["git", "rev-parse", "HEAD"]).strip(),
            "host": platform.platform(),
        },
    }
    jwrite(RESULTS / "summary.json", summary)
    return summary


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
    lines.append("| probe | +TU vs c1 | +stripped vs c1 | +xz vs c1 | 图内自动插入转换 | smoke |")
    lines.append("|---|--:|--:|--:|---|---|")
    for p in summary["probes"]:
        conv = ",".join(p["auto_inserted_conversion"]) or "none"
        lines.append(f"| {p['filter']} | {p['units_delta_vs_c1']} "
                     f"| {fmt_int(p['stripped_delta_vs_c1'])} "
                     f"| {fmt_int(p['xz_delta_vs_c1'])} | {conv} | {p['smoke']} |")
    lines.append("")
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
        allp = all(v == "PASS" for v in gd["gates"].values())
        lines.append(f"| {st} | {'PASS' if allp else 'FAIL ' + str({k: v for k, v in gd['gates'].items() if v != 'PASS'})} |")
    lines.append("")
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
    ok = True
    summary = jload(RESULTS / "summary.json")
    fresh = aggregate()
    if summary["stages"] != fresh["stages"] or summary["ladder"] != fresh["ladder"] \
            or summary["probes"] != fresh["probes"]:
        print("DRIFT: summary.json stale vs stage evidence", file=sys.stderr)
        ok = False
    text = DOC.read_text()
    begin = text.index(MARK_BEGIN)
    end = text.index(MARK_END)
    rendered = render_tables()
    have = text[begin + len(MARK_BEGIN):end].rstrip("\n")
    if have != rendered.rstrip("\n"):
        print("DRIFT: doc tables differ from generated (run --report)", file=sys.stderr)
        ok = False
    if ok:
        print("E10-C0 authority tree: ALL CHECKS PASS")
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
        return check()
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
