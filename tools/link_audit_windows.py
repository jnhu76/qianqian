#!/usr/bin/env python3
"""Windows/COFF link-reachability audit for libqianqian_av.a.

The PE counterpart of tools/link_audit.py (E08 Windows phase). Answers:
which FFmpeg archive members does a link that references the FULL SongCore
public contract actually pull in — with per-member identity, not per-name
guesswork (COFF archives legitimately contain duplicate basenames such as
three different utils.c.o; name-keyed audits silently merge them).

Method
------
1. Parse the ar container directly (offset-identified members; the ar
   format is shared between GNU and llvm archives).
2. Extract every member under a UNIQUE name (mNNNN_<member>) and read its
   symbol table with llvm-nm. Only global/weak definitions can satisfy an
   undefined reference and pull a member; locals and weak-undefined never
   trigger a pull.
3. Simulate first-definer archive resolution over [libsongcore.a,
   libqianqian_av.a] in link order, starting from the undefined roots of
   the contract probe (libc-free; llvm-nm asserts its undefined set is
   EXACTLY the five song_* entry points).
4. Prove the simulation against the REAL linker by content: link the probe
   once against the full archive and once against a reduced archive
   containing ONLY the simulated members, strip debug info from both PEs
   (DWARF legitimately embeds archive paths), then require identical
   load-bearing section tables (.text/.rdata/.data/.pdata VMA+size) and
   identical per-section SHA256. An extra silently-pulled member would
   change .text/.rdata; a missed member would fail to link at all.
   (.reloc/.buildid are excluded: pure link-order metadata.)
5. Corroborate with the real lld -Map: every `member.obj` contributing
   sections must be among the simulated pulled members (name-granular —
   duplicate basenames make the map ambiguous, which is exactly why the
   offset-level simulation above is the hard gate).
6. Map pulled members to manifest translation units 1:1 (xmake archives in
   manifest order; asserted per member, so duplicate basenames are
   disambiguated by position) and emit the same reachable-objects.json
   schema as the Linux audit — the input schema of
   tools/minimize_manifest.py.

This is a candidate generator only. Link reachability says nothing about
data-dependent runtime paths; those are owned by the corpus/PCM/seek gates.
"""
from __future__ import annotations

import argparse
import json
import re
import struct
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys_guard = str(ROOT / "tools")
if sys_guard not in __import__("sys").path:
    __import__("sys").path.insert(0, sys_guard)

from link_audit import CONTRACT_SYMBOLS, GLOBAL_DEF_TYPES, run  # noqa: E402


def parse_archive_coff(path: Path) -> list[dict]:
    """COFF-flavored ar parsing (llvm-ar output).

    Two deviations from the GNU layout that tools/link_audit.py handles:
    - the '//' long-name table entries are NUL-separated (not newline);
    - members keep their compiler object names ('aacdec.c.obj'), so no
      '.o' suffix is appended — names stay exactly as stored.
    Members are identified by archive position (index), which keeps
    duplicate basenames distinct."""
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
            end_n, end_z = long_names.find(b"\n", off), long_names.find(b"\x00", off)
            ends = [e for e in (end_n, end_z) if e >= 0]
            end = min(ends) if ends else len(long_names)
            name = long_names[off:end].decode("ascii", "replace").rstrip("\x00").rstrip("/")
        else:
            name = name_raw.rstrip("/")
        if name not in ("", "/", "/SYM64/", "__.SYMDEF"):  # skip symbol index
            members.append({"index": len(members), "member": name,
                            "size": size, "offset": pos + 60})
        pos += 60 + size + (size & 1)
    return members

# load-bearing PE sections: content must be identical between the full and
# reduced-archive links; debug sections are stripped first (they embed the
# archive's path), .reloc/.buildid are link-metadata
LOAD_BEARING = (".text", ".rdata", ".data", ".pdata")


def nm_symbols(obj: Path, nm: str) -> tuple[set, set, set]:
    """(global/weak defined, undefined strong, weak undefined) via llvm-nm."""
    out = run([nm, str(obj)]).stdout
    defined, undefined, weak_undef = set(), set(), set()
    for line in out.splitlines():
        parts = line.split()
        if len(parts) == 2:
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


def pe_sections(path: Path) -> dict[str, dict]:
    """Parse the PE section table: name -> VMA / raw size / content sha256."""
    import hashlib
    data = path.read_bytes()
    pe_off = struct.unpack_from("<I", data, 0x3C)[0]
    if data[pe_off:pe_off + 4] != b"PE\0\0":
        raise SystemExit(f"not a PE file: {path}")
    coff = pe_off + 4
    num_sections = struct.unpack_from("<H", data, coff + 2)[0]
    opt_size = struct.unpack_from("<H", data, coff + 16)[0]
    table = coff + 20 + opt_size
    out = {}
    for i in range(num_sections):
        base = table + 40 * i
        name = data[base:base + 8].rstrip(b"\0").decode("ascii", "replace")
        vsize, vma, rawsize, rawptr = struct.unpack_from("<IIII", data, base + 8)
        out[name] = {
            "virtual_address": vma,
            "virtual_size": vsize,
            "raw_size": rawsize,
            "sha256": hashlib.sha256(data[rawptr:rawptr + rawsize]).hexdigest() if rawsize else "",
        }
    return out


def strip_debug(pe: Path, out: Path, strip: str) -> None:
    shutil.copy2(pe, out)
    run([strip, "--strip-debug", str(out)])


def member_unit_key(name: str) -> str:
    """Normalize an archive-member/object name to the manifest's unit key.

    Manifest units carry FFmpeg-make-style object names ('aacdec.o'); xmake's
    on-disk objects and archive members are platform-named ('aacdec.c.o' on
    linux/gcc, potentially 'aacdec.obj'-style under mingw). Strip the platform
    object suffix, then the source extension, then re-canonicalize."""
    stem = name
    for suf in (".obj", ".o"):
        if stem.endswith(suf):
            stem = stem[: -len(suf)]
            break
    for suf in (".c", ".s", ".S", ".m", ".h", ".cc", ".cpp"):
        if stem.endswith(suf):
            stem = stem[: -len(suf)]
            break
    return stem + ".o"


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--archive", default="build/artifacts/libqianqian_av.a")
    ap.add_argument("--songcore", default="build/artifacts/libsongcore.a")
    ap.add_argument("--probe-src", default="tools/songcore_link_probe.c")
    ap.add_argument("--manifest", required=True,
                    help="manifest.json whose units this archive replays")
    ap.add_argument("--out", required=True, help="output dir for audit evidence")
    args = ap.parse_args()

    outdir = ROOT / args.out
    outdir.mkdir(parents=True, exist_ok=True)
    archive_path = ROOT / args.archive
    songcore_path = ROOT / args.songcore
    manifest = json.loads((ROOT / args.manifest).read_text())

    import os
    nm = os.environ.get("WIN_NM", "x86_64-w64-mingw32-nm")
    cc = os.environ.get("WIN_CC", "x86_64-w64-mingw32-clang")
    ar = os.environ.get("WIN_AR", "x86_64-w64-mingw32-ar")
    strip = os.environ.get("WIN_STRIP", "x86_64-w64-mingw32-strip")

    with tempfile.TemporaryDirectory(prefix="win-audit-") as tmp:
        work = Path(tmp)

        ffmpeg_members = parse_archive_coff(archive_path)
        archive_bytes = archive_path.read_bytes()
        for m in ffmpeg_members:
            obj = work / f"m{m['index']:04d}"
            obj.write_bytes(archive_bytes[m["offset"]:m["offset"] + m["size"]])
            d, u, w = nm_symbols(obj, nm)
            m["defined"], m["undef"], m["weak_undef"] = d, u, w

        songcore_members = parse_archive_coff(songcore_path)
        songcore_raw = songcore_path.read_bytes()
        for m in songcore_members:
            obj = work / f"sc{m['index']:04d}"
            obj.write_bytes(songcore_raw[m["offset"]:m["offset"] + m["size"]])
            d, u, w = nm_symbols(obj, nm)
            m["defined"], m["undef"], m["weak_undef"] = d, u, w

        # --- contract pin: the probe must reference EXACTLY the five APIs.
        # __main is whitelisted: the mingw CRT convention emits a call to it
        # from main() for runtime initializers; it is resolved by the CRT
        # startup objects and can never pull an FFmpeg archive member.
        probe_c = ROOT / args.probe_src
        probe_obj = work / "probe.obj"
        run([cc, "-O2", "-I", str(ROOT / "include"), "-c", str(probe_c),
             "-o", str(probe_obj)])
        probe_defined, probe_undef, _ = nm_symbols(probe_obj, nm)
        probe_undef -= {"__main"}
        if probe_undef != CONTRACT_SYMBOLS:
            raise SystemExit(
                f"probe contract pin violated: probe undefined set is {sorted(probe_undef)}, "
                f"expected exactly {sorted(CONTRACT_SYMBOLS)}")
        inputs = [{"member": "songcore_link_probe.obj",
                   "defined": probe_defined, "undef": probe_undef}] + [
            {"member": m["member"], "defined": m["defined"], "undef": m["undef"]}
            for m in songcore_members]

        # --- simulate first-definer archive resolution (link order:
        # probe, libsongcore.a, libqianqian_av.a)
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
            for m in ffmpeg_members:
                if m["index"] in pulled:
                    continue
                trigger = {s for s in m["defined"] if s in undefined}
                if not trigger:
                    continue
                pulled[m["index"]] = m
                reasons[m["index"]] = trigger
                undefined -= m["defined"]
                undefined |= m["undef"] - m["defined"]
                changed = True

        pulled_members = [pulled[i] for i in sorted(pulled)]
        pulled_bytes = sum(m["size"] for m in pulled_members)

        # --- real-linker proof by content: full vs reduced-archive PEs
        def link(output: Path, av_archive: Path, *, mapfile: Path | None = None) -> None:
            cmd = [cc, "-O2", "-o", str(output), str(probe_obj),
                   str(songcore_path), str(av_archive), "-lbcrypt",
                   "-Wl,--gc-sections"]
            if mapfile:
                cmd.append("-Wl,-Map=" + str(mapfile))
            p = subprocess.run(cmd, capture_output=True, text=True)
            if p.returncode:
                raise SystemExit(f"probe link failed ({output.name}): {p.stderr[-4000:]}")

        reduced = work / "libqianqian_av_reduced.a"
        rdir = work / "reduced-members"
        rdir.mkdir()
        for m in pulled_members:
            body = archive_bytes[m["offset"]:m["offset"] + m["size"]]
            (rdir / f"{m['index']:04d}_{m['member']}").write_bytes(body)
        run([ar, "rcs", str(reduced), *map(str, sorted(rdir.iterdir()))])

        exe_full, exe_reduced = work / "probe_full.exe", work / "probe_reduced.exe"
        link(exe_full, archive_path, mapfile=outdir / "linker.map")
        link(exe_reduced, reduced)

        strip_full, strip_reduced = work / "full_stripped.exe", work / "reduced_stripped.exe"
        strip_debug(exe_full, strip_full, strip)
        strip_debug(exe_reduced, strip_reduced, strip)
        sec_full, sec_reduced = pe_sections(strip_full), pe_sections(strip_reduced)
        problems = []
        for name in LOAD_BEARING:
            a, b = sec_full.get(name), sec_reduced.get(name)
            if a is None or b is None:
                problems.append(f"missing section {name}")
                continue
            if (a["virtual_address"], a["raw_size"], a["sha256"]) != \
               (b["virtual_address"], b["raw_size"], b["sha256"]):
                problems.append(f"section {name} differs (full vs reduced archive)")
        if problems:
            raise SystemExit("Windows reachability gate FAILED: " + "; ".join(problems))

        # --- lld -Map corroboration (name-granular; duplicates make the map
        # ambiguous, so this is supporting evidence, not the hard gate)
        map_names = set(re.findall(r"^\S+\s+\S+\s+\S+\s+(\S+\.obj):\(",
                                   (outdir / "linker.map").read_text(), re.M))
        pulled_keys = {member_unit_key(m["member"]) for m in pulled_members}
        map_keys = {member_unit_key(n) for n in map_names}
        strays = sorted(map_keys - pulled_keys)
        if strays:
            raise SystemExit(f"Windows reachability gate FAILED: lld map contributes "
                             f"members outside the simulation: {strays[:8]}")

    # --- map members to manifest units 1:1 (xmake archives in manifest order)
    units = manifest["units"]
    if len(ffmpeg_members) != len(units):
        raise SystemExit(
            f"member/unit count drift: {len(ffmpeg_members)} members vs "
            f"{len(units)} manifest units")

    reachable_objects = []
    for m, unit in zip(ffmpeg_members, units):
        if member_unit_key(m["member"]) != member_unit_key(Path(unit["object"]).name):
            raise SystemExit(
                f"archive/manifest order drift at member {m['index']}: "
                f"{m['member']} vs {unit['object']}")
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

    dup_basenames: list[str] = []
    seen: dict[str, int] = {}
    for m in ffmpeg_members:
        seen[m["member"]] = seen.get(m["member"], 0) + 1
    dup_basenames = sorted(n for n, c in seen.items() if c > 1)

    report = {
        "schema": 2,
        "platform": "windows-x86_64 (llvm-mingw)",
        "archive": args.archive,
        "archive_bytes": archive_path.stat().st_size,
        "archive_members": len(ffmpeg_members),
        "duplicate_member_basenames": dup_basenames,
        "pulled_members": len(pulled_members),
        "pulled_member_bytes": pulled_bytes,
        "unpulled_member_bytes": sum(m["size"] for m in ffmpeg_members) - pulled_bytes,
        "manifest_translation_units": len(units),
        "pulled_manifest_units": sum(1 for r in reachable_objects if r["pulled"]),
        "definition_semantics": "global/weak only (llvm-nm COFF); locals/weak-undef cannot pull",
        "probe_contract_undefined": sorted(probe_undef),
        "verification": {
            "method": "reduced-archive link: load-bearing PE sections (.text/.rdata/.data/.pdata) "
                      "identical (VMA/size/sha256) after --strip-debug; stray lld -Map members rejected",
            "lld_map_distinct_member_names": len(map_names),
            "lld_map_names_within_simulation": True,
            "map_granularity_note": "duplicate basenames make the lld map ambiguous per member; "
                                    "the offset-level simulation + reduced-archive content proof is the hard gate",
        },
    }
    (outdir / "reachable-objects.json").write_text(json.dumps(reachable_objects, indent=1) + "\n")
    (outdir / "report.json").write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    print(json.dumps({k: report[k] for k in (
        "archive_members", "pulled_members", "pulled_member_bytes",
        "pulled_manifest_units", "duplicate_member_basenames")}, indent=2))
    print("PE equality: full-archive link == reduced-archive link "
          "(load-bearing sections, post-strip-debug)")


if __name__ == "__main__":
    main()
