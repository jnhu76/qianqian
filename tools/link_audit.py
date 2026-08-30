#!/usr/bin/env python3
"""S1 link-reachability audit for libqianqian_av.a.

Answers: which FFmpeg archive members does a link that references the FULL
SongCore public contract actually pull in, and which symbols cause it?

Method
------
1. Parse the ar archive directly (member index disambiguates duplicate
   basenames such as options.c.o existing in libavcodec AND libavformat).
2. Extract every member and read its symbol table with nm: defined
   (global/weak/common) and undefined (strong) symbols.
3. Simulate GNU ld archive resolution: start from the strong undefined
   symbols of the non-archive inputs (probe + songcore), then repeatedly
   scan the archive in member order, pulling any member that defines a
   currently-undefined symbol, rescanning until fixpoint.
4. Corroborate with a real ld run using -Wl,--trace -Wl,-Map.
5. Prove the simulation: build a reduced archive containing only the pulled
   members, link the probe against it, and compare the resulting ELF against
   the full-archive link (size, section sizes, dynamic symbol set).
6. Map pulled members to manifest translation units and emit machine-readable
   evidence under build/minimize/s1/.

This is a candidate generator only. Link reachability says nothing about
data-dependent runtime paths; those are owned by the corpus/PCM/seek gates.
"""
from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import struct
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MANIFEST = ROOT / "build" / "ffmpeg-xmake" / "manifest.json"

DEFINED_TYPES = set("TtDdBbRrGgWwVvCu")  # 'u' = GNU unique global
PULLING_TYPES = set("TDBRWVGuC")  # global/weak/common definitions satisfy refs


def run(cmd: list[str], **kw) -> subprocess.CompletedProcess:
    p = subprocess.run(cmd, capture_output=True, text=True, **kw)
    if p.returncode:
        raise SystemExit(f"command failed ({p.returncode}): {' '.join(map(str, cmd))}\n{p.stderr[-4000:]}")
    return p


def parse_archive(path: Path) -> list[dict]:
    """Parse ar format ourselves: member order + raw member files.

    GNU ar format: 8-byte '!<arch>\n' magic, then 60-byte headers. Names are
    either 'name/' or '/<offset>' into the '// ' long-name table.
    """
    data = path.read_bytes()
    if data[:8] != b"!<arch>\n":
        raise SystemExit(f"not an ar archive: {path}")
    members = []
    pos = 8
    long_names = b""
    while pos < len(data):
        if len(data) - pos < 60:
            break
        hdr = data[pos:pos + 60]
        name_raw = hdr[0:16].decode("ascii", "replace").rstrip()
        size = int(hdr[48:58].decode().strip())
        body = data[pos + 60:pos + 60 + size]
        if name_raw == "//":
            long_names = body
        elif re.fullmatch(r"/\d+", name_raw):
            off = int(name_raw[1:])
            end = long_names.find(b"\n", off)
            name = long_names[off:end].decode("ascii", "replace").rstrip("/")
        else:
            name = name_raw.rstrip("/")
        if name not in ("", "/", "/SYM64/", "__.SYMDEF"):  # skip symbol index
            members.append({"index": len(members), "member": name + ".o" if not name.endswith(".o") else name,
                            "size": size, "offset": pos + 60})
        pos += 60 + size + (size & 1)
    return members


def nm_symbols(member_bytes: bytes, workdir: Path, idx: int) -> tuple[set, set, set]:
    """Return (defined, undefined_strong, weak_undefined) symbol sets."""
    obj = workdir / f"m{idx:04d}.o"
    obj.write_bytes(member_bytes)
    out = run(["nm", str(obj)]).stdout
    defined, undefined, weak_undef = set(), set(), set()
    for line in out.splitlines():
        parts = line.split()
        if len(parts) == 2:  # undefined symbol: type, name (no value column)
            typ, name = parts
            if typ == "U":
                undefined.add(name)
            elif typ == "w":
                weak_undef.add(name)
            continue
        if len(parts) != 3:
            continue
        _, typ, name = parts
        if typ == "U":
            undefined.add(name)
        elif typ == "w":
            weak_undef.add(name)
        elif typ in DEFINED_TYPES:
            defined.add(name)
    return defined, undefined, weak_undef


def elf_fingerprint(binary: Path) -> dict:
    out = run(["readelf", "-SW", str(binary)]).stdout
    sections = {}
    for m in re.finditer(r"\[\s*\d+\]\s+(\S+)\s+\S+\s+([0-9a-f]+)\s+([0-9a-f]+)\s+([0-9a-f]+)", out):
        sections[m.group(1)] = int(m.group(4), 16)
    syms = {l.split()[-1] for l in run(["nm", str(binary)]).stdout.splitlines() if l.strip()}
    return {"bytes": binary.stat().st_size, "sections": sections, "symbols": syms}


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--archive", default="build/artifacts/libqianqian_av.a")
    ap.add_argument("--songcore", default="build/artifacts/libsongcore.a")
    ap.add_argument("--probe-src", default="tools/songcore_link_probe.c")
    ap.add_argument("--fixture", default="corpus/fixtures/mp3-short.mp3")
    ap.add_argument("--manifest", default="build/ffmpeg-xmake/manifest.json")
    ap.add_argument("--out", default="build/minimize/s1")
    args = ap.parse_args()

    outdir = ROOT / args.out
    outdir.mkdir(parents=True, exist_ok=True)
    archive_path = ROOT / args.archive
    songcore_path = ROOT / args.songcore

    with tempfile.TemporaryDirectory(prefix="s1-audit-") as tmp:
        work = Path(tmp)

        ffmpeg_members = parse_archive(archive_path)
        for m in ffmpeg_members:
            d, u, w = nm_symbols(archive_path.read_bytes()[m["offset"]:m["offset"] + m["size"]], work, m["index"])
            m["defined"], m["undef"], m["weak_undef"] = d, u, w

        songcore_members = parse_archive(songcore_path)
        for m in songcore_members:
            d, u, w = nm_symbols(songcore_path.read_bytes()[m["offset"]:m["offset"] + m["size"]], work, 10000 + m["index"])
            m["defined"], m["undef"], m["weak_undef"] = d, u, w
        inputs = [{"member": m["member"], "defined": m["defined"], "undef": m["undef"]} for m in songcore_members]

        # The probe forces the full contract; link it as an object input.
        probe_c = ROOT / args.probe_src
        probe_obj = work / "probe.o"
        run(["gcc", "-O2", "-I", str(ROOT / "include"), "-c", str(probe_c), "-o", str(probe_obj)])
        probe_syms = nm_symbols(probe_obj.read_bytes(), work, 99999)
        inputs = [{"member": "songcore_link_probe.o", "defined": probe_syms[0], "undef": probe_syms[1]}] + inputs

        # --- simulate ld over [songcore archive, ffmpeg archive] in link order
        # SongCore archive is processed first (xmake deps order), then ffmpeg.
        # A member of songcore referencing FFmpeg symbols leaves them undefined
        # for the ffmpeg archive pass.
        undefined: set[str] = set()
        defined: set[str] = set()
        for inp in inputs:
            undefined |= inp["undef"] - inp["defined"]
            defined |= inp["defined"]
        undefined -= defined

        pulled: dict[int, dict] = {}
        reasons: dict[int, set] = {}
        changed = True
        while changed:
            changed = False
            for ai, (arch_members, arch_bytes) in enumerate(
                [(songcore_members, songcore_path.read_bytes()), (ffmpeg_members, archive_path.read_bytes())]
            ):
                for m in arch_members:
                    if ai == 1 and m["index"] in pulled:
                        continue
                    trigger = {s for s in m["defined"] if s in undefined}
                    if not trigger:
                        continue
                    if ai == 1:
                        pulled[m["index"]] = m
                        reasons[m["index"]] = trigger
                    undefined -= m["defined"]
                    undefined |= m["undef"] - m["defined"]
                    changed = True

        pulled_members = [pulled[i] for i in sorted(pulled)]
        pulled_bytes = sum(m["size"] for m in pulled_members)

        # --- real ld corroboration + map/trace evidence
        probe_bin = work / "link_probe_full"
        trace_file = outdir / "ld.trace"
        map_file = outdir / "linker.map"
        link_cmd = [
            "gcc", "-O2", "-o", str(probe_bin), str(probe_obj),
            str(songcore_path), str(archive_path),
            "-lm", "-lpthread",
            "-Wl,--trace", f"-Wl,-Map={map_file}",
        ]
        p = subprocess.run(link_cmd, capture_output=True, text=True)
        if p.returncode:
            raise SystemExit(f"probe link failed: {p.stderr[-4000:]}")
        # --trace prints to stdout
        trace_lines = [l for l in (p.stdout + p.stderr).splitlines() if archive_path.name in l]
        trace_file.write_text("\n".join(sorted(trace_lines)) + "\n")

        traced_names = sorted(re.findall(rf"{archive_path.name}\(([^)]+)\)", "\n".join(trace_lines)))

        # --- proof: reduced archive must link to an identical ELF
        reduced = work / "libqianqian_av_reduced.a"
        rdir = work / "reduced-members"
        rdir.mkdir()
        for m in pulled_members:
            body = archive_path.read_bytes()[m["offset"]:m["offset"] + m["size"]]
            (rdir / f"{m['index']:04d}_{m['member']}").write_bytes(body)
        ar_files = sorted(rdir.iterdir())
        run(["ar", "rcs", str(reduced), *map(str, ar_files)])
        reduced_bin = work / "link_probe_reduced"
        p2 = subprocess.run(
            ["gcc", "-O2", "-o", str(reduced_bin), str(probe_obj), str(songcore_path), str(reduced), "-lm", "-lpthread"],
            capture_output=True, text=True)
        if p2.returncode:
            raise SystemExit(f"reduced link failed (simulation incomplete): {p2.stderr[-4000:]}")

        fp_full = elf_fingerprint(probe_bin)
        fp_reduced = elf_fingerprint(reduced_bin)
        sim_ok = (fp_full["bytes"] == fp_reduced["bytes"]
                  and fp_full["sections"] == fp_reduced["sections"]
                  and fp_full["symbols"] == fp_reduced["symbols"])

    # --- map members to manifest translation units
    # xmake archives members in manifest order; member basenames align 1:1
    # (verified below), which also disambiguates duplicate basenames.
    manifest = json.loads((ROOT / args.manifest).read_text())
    assert len(ffmpeg_members) == len(manifest["units"]), (
        f"member/unit count drift: {len(ffmpeg_members)} vs {len(manifest['units'])}")

    def member_base(m: dict) -> str:
        n = m["member"]
        for suf in (".c.o", ".s.o", ".S.o"):
            if n.endswith(suf):
                return n[: -len(suf)] + ".o"
        return n

    reachable_objects = []
    for m, unit in zip(ffmpeg_members, manifest["units"]):
        assert member_base(m) == Path(unit["object"]).name, (
            f"archive/manifest order drift at member {m['index']}: {m['member']} vs {unit['object']}")
        is_pulled = m["index"] in pulled
        reachable_objects.append({
            "member": m["member"],
            "member_index": m["index"],
            "member_bytes": m["size"],
            "pulled": is_pulled,
            "first_required_by": sorted(reasons[m["index"]]) if is_pulled else [],
            "unit_object": unit["object"],
            "unit_source": unit["path"],
            "unit_origin": unit["origin"],
        })

    # --- symbol edges between pulled members + roots
    definer: dict[str, str] = {}
    for m in pulled_members:
        for s in m["defined"]:
            definer.setdefault(s, m["member"])
    edges: dict[tuple[str, str], set] = {}
    for m in pulled_members:
        for s in m["undef"]:
            tgt = definer.get(s)
            if tgt and tgt != m["member"]:
                edges.setdefault((m["member"], tgt), set()).add(s)
    root_edges: dict[str, set] = {}
    for inp in inputs:
        for s in inp["undef"]:
            tgt = definer.get(s)
            if tgt:
                root_edges.setdefault(tgt, set()).add(inp["member"] + ":" + s)

    edges_list = [
        {"from": a, "to": b, "symbols": sorted(syms)}
        for (a, b), syms in sorted(edges.items())
    ]
    roots = [
        {"member": mem,
         "needed_by": [
             {"input": src, "symbols": sorted({v.split(":", 1)[1] for v in allsyms if v.startswith(src + ":")})}
             for src in sorted({v.split(":", 1)[0] for v in allsyms})
         ]}
        for mem, allsyms in sorted(root_edges.items())
    ]

    symbol_edges = {"roots": roots, "edges": edges_list}

    # reachable-sources.json: per manifest unit, pulled?
    by_unit = {}
    for e in reachable_objects:
        if e["unit_object"]:
            by_unit.setdefault(e["unit_object"], []).append(e)
    reachable_sources = []
    for u in manifest["units"]:
        entries = by_unit.get(u["object"], [])
        pulled_any = any(e["pulled"] for e in entries)
        reachable_sources.append({
            "object": u["object"],
            "source": u["path"],
            "origin": u["origin"],
            "pulled": pulled_any,
            "member_bytes": sum(e["member_bytes"] for e in entries),
        })

    total_unit_bytes = sum(r["member_bytes"] for r in reachable_sources if r["pulled"])
    report = {
        "schema": 1,
        "stage": "S1",
        "archive": args.archive,
        "archive_bytes": archive_path.stat().st_size,
        "archive_members": len(ffmpeg_members),
        "pulled_members": len(pulled_members),
        "pulled_member_bytes": pulled_bytes,
        "unpulled_member_bytes": sum(m["size"] for m in ffmpeg_members) - pulled_bytes,
        "manifest_translation_units": len(manifest["units"]),
        "pulled_manifest_units": sum(1 for r in reachable_sources if r["pulled"]),
        "pulled_unit_member_bytes": total_unit_bytes,
        "ld_trace_member_count": len(traced_names),
        "simulation_verified": sim_ok,
        "verification": {
            "full_link_bytes": fp_full["bytes"],
            "reduced_link_bytes": fp_reduced["bytes"],
            "sections_identical": fp_full["sections"] == fp_reduced["sections"],
            "symbols_identical": fp_full["symbols"] == fp_reduced["symbols"],
        },
        "root_symbols": sorted({s for m in pulled_members for s in []} | set().union(*reasons.values()) if reasons else []),
    }
    (outdir / "reachable-objects.json").write_text(json.dumps(reachable_objects, indent=1) + "\n")
    (outdir / "reachable-sources.json").write_text(json.dumps(reachable_sources, indent=1) + "\n")
    (outdir / "symbol-edges.json").write_text(json.dumps(symbol_edges, indent=1) + "\n")
    (outdir / "report.json").write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    print(json.dumps({k: report[k] for k in (
        "archive_members", "pulled_members", "pulled_member_bytes", "unpulled_member_bytes",
        "pulled_manifest_units", "pulled_unit_member_bytes", "ld_trace_member_count",
        "simulation_verified")}, indent=2))
    if not sim_ok:
        raise SystemExit("simulation verification FAILED: reduced-archive link differs from full link")


if __name__ == "__main__":
    main()
