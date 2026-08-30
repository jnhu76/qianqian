#!/usr/bin/env python3
"""S1 link-reachability audit for libqianqian_av.a.

Answers: which FFmpeg archive members does a link that references the FULL
SongCore public contract actually pull in, and which symbols cause it?

Method
------
1. Parse the ar archive directly (member index disambiguates duplicate
   basenames such as options.c.o existing in libavcodec AND libavformat).
2. Extract every member and read its symbol table with nm. Only
   global/weak/common/unique definitions (nm types ABCDGRSTVWu) can satisfy
   an undefined reference and cause an archive-member pull; local symbols
   (lowercase types) never trigger a pull in GNU ld and are ignored here.
3. Simulate GNU ld archive resolution: start from the strong undefined
   symbols of the non-archive inputs (probe + songcore), then repeatedly
   scan the archive in member order, pulling any member that DEFINES a
   currently-undefined symbol, rescanning until fixpoint.
4. Pin the probe: compile songcore_link_probe.c and assert with `nm -u`
   that the probe object's undefined set is EXACTLY the five contract
   entry points (the probe is libc-free, so the set must be exact).
5. Corroborate with a real ld run and HARD-GATE on it: parse the
   "Archive member included to satisfy reference by file (symbol)" section
   of the -Map file and require the real pulled-member multiset to equal
   the simulated one exactly.
6. Prove the simulation by content, not by shape: link the probe once
   against the full archive and once against a reduced archive containing
   only the pulled members, then require SHA256 equality of the resulting
   ELF files. If whole-file hashes differ (linker metadata), fall back to
   per-section SHA256 comparison of every section read from the ELF.
7. Map pulled members to manifest translation units and emit
   machine-readable evidence under build/minimize/<stage>/.

This is a candidate generator only. Link reachability says nothing about
data-dependent runtime paths; those are owned by the corpus/PCM/seek gates.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import re
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

# nm types that denote a DEFINITION able to satisfy an undefined reference:
# uppercase = global, plus weak defined (V/W), common (C) and GNU unique (u).
# Local symbols (t/d/b/r/s/g/v/w) can never pull an archive member.
GLOBAL_DEF_TYPES = set("ABCDGRSTVWu")
CONTRACT_SYMBOLS = {"song_open", "song_probe", "song_read_pcm", "song_seek", "song_close"}


def run(cmd: list[str], **kw) -> subprocess.CompletedProcess:
    p = subprocess.run(cmd, capture_output=True, text=True, **kw)
    if p.returncode:
        raise SystemExit(f"command failed ({p.returncode}): {' '.join(map(str, cmd))}\n{p.stderr[-4000:]}")
    return p


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


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
    """Return (global/weak/common defined, undefined_strong, weak_undefined)."""
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
        elif typ in GLOBAL_DEF_TYPES:
            defined.add(name)
    return defined, undefined, weak_undef


def parse_map_archive_members(map_text: str, archive_name: str) -> tuple[list[str], dict[str, str]]:
    """Extract the real pulled members of one archive from the ld -Map file.

    Section header: 'Archive member included to satisfy reference by file
    (symbol)'. Each pull entry is an UNINDENTED line '/path/libx.a(member.o)';
    the triggering reference follows on the next, indented line as
    '(symbol in file)'. Indented lines that themselves name an archive member
    are references, not pulls, and must not be counted.
    Returns (sorted member multiset as list, {member: trigger symbol}).
    """
    start = map_text.find("Archive member included to satisfy reference by file")
    if start < 0:
        raise SystemExit("ld -Map is missing the 'Archive member included' section; cannot gate")
    rest = map_text[start:]
    for end_marker in ("Discarded input sections", "Memory Configuration", "Linker script and memory map"):
        pos = rest.find(end_marker)
        if pos >= 0:
            rest = rest[:pos]
    members, triggers = [], {}
    lines = rest.splitlines()
    for i, line in enumerate(lines):
        m = re.match(r"^(\S+\.a)\(([^)]+)\)\s*$", line)
        if not m or not m.group(1).endswith(archive_name):
            continue
        member = m.group(2)
        members.append(member)
        for ref in lines[i + 1:i + 3]:
            t = re.match(r"^\s+\((\S+) in ", ref)
            if t:
                triggers.setdefault(member, t.group(1))
                break
    if not members:
        raise SystemExit(f"ld -Map lists no pulled members for {archive_name}; cannot gate")
    return sorted(members), triggers


def section_shas(binary: Path) -> dict[str, str]:
    """SHA256 of every named ELF section (fallback equality proof)."""
    out = run(["readelf", "-SW", str(binary)]).stdout
    result = {}
    data = binary.read_bytes()
    for m in re.finditer(
            r"\[\s*\d+\]\s+(\S+)\s+\S+\s+([0-9a-f]+)\s+([0-9a-f]+)\s+([0-9a-f]+)", out):
        name, _addr, off, size = m.group(1), int(m.group(2), 16), int(m.group(3), 16), int(m.group(4), 16)
        if size:
            result[name] = sha256_bytes(data[off:off + size])
    return result


def elf_fingerprint(binary: Path) -> dict:
    out = run(["readelf", "-SW", str(binary)]).stdout
    sections = {}
    for m in re.finditer(r"\[\s*\d+\]\s+(\S+)\s+\S+\s+([0-9a-f]+)\s+([0-9a-f]+)\s+([0-9a-f]+)", out):
        sections[m.group(1)] = int(m.group(4), 16)
    syms = {l.split()[-1] for l in run(["nm", str(binary)]).stdout.splitlines() if l.strip()}
    return {"bytes": binary.stat().st_size, "sections": sections, "symbols": syms}


def link_probe(output: Path, probe_obj: Path, songcore: Path, ffmpeg_archive: Path) -> None:
    p = subprocess.run(
        ["gcc", "-O2", "-o", str(output), str(probe_obj), str(songcore), str(ffmpeg_archive),
         "-lm", "-lpthread"], capture_output=True, text=True)
    if p.returncode:
        raise SystemExit(f"probe link failed ({output.name}): {p.stderr[-4000:]}")


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--archive", default="build/artifacts/libqianqian_av.a")
    ap.add_argument("--songcore", default="build/artifacts/libsongcore.a")
    ap.add_argument("--probe-src", default="tools/songcore_link_probe.c")
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

        # --- hard gate: the probe object must reference the FULL contract.
        # The probe is libc-free by design, so its undefined set must be
        # EXACTLY the five entry points; anything else means the fixture
        # drifted (a weakened pin would silently shrink the closure).
        probe_c = ROOT / args.probe_src
        probe_obj = work / "probe.o"
        run(["gcc", "-O2", "-I", str(ROOT / "include"), "-c", str(probe_c), "-o", str(probe_obj)])
        probe_syms = nm_symbols(probe_obj.read_bytes(), work, 99999)
        probe_undef = probe_syms[1]
        if probe_undef != CONTRACT_SYMBOLS:
            raise SystemExit(
                f"probe contract pin violated: probe.o undefined set is {sorted(probe_undef)}, "
                f"expected exactly {sorted(CONTRACT_SYMBOLS)}")
        inputs = [{"member": "songcore_link_probe.o", "defined": probe_syms[0], "undef": probe_undef}] + inputs

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

        # --- real ld evidence: -Map member multiset must equal the simulation
        map_bin = work / "link_probe_map"
        map_file = outdir / "linker.map"
        p = subprocess.run(
            ["gcc", "-O2", "-o", str(map_bin), str(probe_obj), str(songcore_path), str(archive_path),
             "-lm", "-lpthread", "-Wl,--trace", f"-Wl,-Map={map_file}"],
            capture_output=True, text=True)
        if p.returncode:
            raise SystemExit(f"probe link failed: {p.stderr[-4000:]}")
        map_text = map_file.read_text()
        (outdir / "ld.trace").write_text(
            "\n".join(sorted(l for l in (p.stdout + p.stderr).splitlines())) + "\n")
        real_members, real_triggers = parse_map_archive_members(map_text, archive_path.name)
        sim_members = sorted(m["member"] for m in pulled_members)
        if real_members != sim_members:
            only_real = sorted(set(real_members) - set(sim_members))
            only_sim = sorted(set(sim_members) - set(real_members))
            raise SystemExit(
                "S1 hard gate FAILED: real ld pulled-member multiset differs from simulation "
                f"(real-only={only_real}, simulated-only={only_sim})")
        (outdir / "ld-map-pulls.json").write_text(json.dumps({
            "archive": archive_path.name,
            "pulled_members": real_members,
            "trigger_symbols": real_triggers,
        }, indent=1) + "\n")

        # --- proof by content: reduced archive must link to the SAME ELF bytes
        reduced = work / "libqianqian_av_reduced.a"
        rdir = work / "reduced-members"
        rdir.mkdir()
        for m in pulled_members:
            body = archive_path.read_bytes()[m["offset"]:m["offset"] + m["size"]]
            (rdir / f"{m['index']:04d}_{m['member']}").write_bytes(body)
        run(["ar", "rcs", str(reduced), *map(str, sorted(rdir.iterdir()))])
        full_bin = work / "link_probe_full"
        reduced_bin = work / "link_probe_reduced"
        link_probe(full_bin, probe_obj, songcore_path, archive_path)
        link_probe(reduced_bin, probe_obj, songcore_path, reduced)

        full_sha = sha256_bytes(full_bin.read_bytes())
        reduced_sha = sha256_bytes(reduced_bin.read_bytes())
        sections_fallback = None
        if full_sha != reduced_sha:
            # Whole-file hashes may differ through linker metadata; the loadable
            # content itself must still be identical section by section.
            sec_full, sec_reduced = section_shas(full_bin), section_shas(reduced_bin)
            if sec_full != sec_reduced:
                diff = sorted(k for k in set(sec_full) | set(sec_reduced)
                              if sec_full.get(k) != sec_reduced.get(k))
                raise SystemExit(
                    "S1 hard gate FAILED: reduced-archive link is not content-identical "
                    f"(differing sections: {diff})")
            sections_fallback = True

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
        "schema": 2,
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
        "definition_semantics": "global/weak/common/unique only (nm ABCDGRSTVWu); locals cannot pull",
        "probe_contract_undefined": sorted(probe_undef),
        "ld_map_pulled_members": len(real_members),
        "ld_map_matches_simulation": True,
        "root_symbols": sorted(set().union(*reasons.values()) if reasons else []),
        "verification": {
            "elf_sha256_full_archive": full_sha,
            "elf_sha256_reduced_archive": reduced_sha,
            "elf_identical_whole_file": full_sha == reduced_sha,
            "sections_identical_fallback": sections_fallback,
        },
    }
    (outdir / "reachable-objects.json").write_text(json.dumps(reachable_objects, indent=1) + "\n")
    (outdir / "reachable-sources.json").write_text(json.dumps(reachable_sources, indent=1) + "\n")
    (outdir / "symbol-edges.json").write_text(json.dumps(symbol_edges, indent=1) + "\n")
    (outdir / "report.json").write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    print(json.dumps({k: report[k] for k in (
        "archive_members", "pulled_members", "pulled_member_bytes", "unpulled_member_bytes",
        "pulled_manifest_units", "pulled_unit_member_bytes",
        "ld_map_pulled_members", "ld_map_matches_simulation")}, indent=2))
    print(f"ELF equality: {'whole-file sha256' if full_sha == reduced_sha else 'per-section sha256 fallback'}")


if __name__ == "__main__":
    main()
