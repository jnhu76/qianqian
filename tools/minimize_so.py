#!/usr/bin/env python3
"""S6: measure the SHIPPED codec core as a real shared library.

Motivation (PR #6 review): "530 KB linked core" was actually the raw
qn_pcm_dump executable, which still contains host code (main, file IO, QPCM
transport) and was reported unstripped. The honest shipped-core question is:

    what does a dynamic library exposing ONLY the five SongCore entry points
    weigh, with every FFmpeg/internal symbol forced local?

Pipeline
--------
1. Derive a PIC variant of the accepted minimal closure's projected manifest
   (one codegen dimension: -fPIC; closure and opt level unchanged).
2. Clean-rebuild via Xmake with that manifest, then run the corpus
   oracle-equivalence gate — PIC is a codegen change, so it must re-pass.
3. Compile SongCore itself as PIC and link

       libqianqian_songcore.so = songcore_pic.o + libqianqian_av.a
                                + version script (tools/songcore_version.map)
                                + -Wl,--gc-sections

4. HARD GATES:
   - dynamic exports are exactly the five song_* entry points (plus recorded
     linker-defined noise such as __bss_start), each versioned QIANQIAN_1.0;
   - a consumer binary linked only against the .so drives the full contract
     (open/probe/read/seek/close) on an MP3 and a FLAC fixture.
5. Record raw / stripped / stripped+xz sizes into build/minimize/<stage>/so.json.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CONTRACT = ["song_open", "song_probe", "song_read_pcm", "song_seek", "song_close"]
# Linker-defined symbols that may survive a version script's `local: *;`,
# plus the version-name marker entry nm lists for QIANQIAN_1.0 itself.
LINKER_NOISE = {"__bss_start", "_edata", "_end", "__TMC_END__", "_init", "_fini",
                "__data_start", "QIANQIAN_1.0", "data_start"}
SMOKE_FILES = ["corpus/fixtures/mp3-short.mp3", "corpus/fixtures/flac-24-96.flac"]


def run(cmd: list[str], *, check=True, binary=False) -> subprocess.CompletedProcess:
    print("+", " ".join(map(str, cmd)), flush=True)
    p = subprocess.run(list(map(str, cmd)), cwd=ROOT, capture_output=True, text=not binary)
    if check and p.returncode:
        err = p.stderr if not binary else p.stderr.decode("utf-8", "replace")
        raise SystemExit(f"command failed ({p.returncode}): {' '.join(map(str, cmd))}\n{err[-4000:]}")
    return p


def sha256_file(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--stage", default="s6-shipped-so")
    ap.add_argument("--from-stage", default="s3-pthreads",
                    help="stage whose manifest-projected.json defines the accepted minimal closure")
    args = ap.parse_args()

    stage_dir = ROOT / "build" / "minimize" / args.stage
    stage_dir.mkdir(parents=True, exist_ok=True)

    # --- 1. PIC projection of the accepted minimal closure (one dimension)
    run([sys.executable, "tools/minimize_flags.py",
         "--stage", args.stage, "--from-stage", args.from_stage, "--add-flag=-fPIC"])

    # --- 2. clean rebuild with the PIC manifest + corpus oracle gate
    run(["xmake", "f", "-m", "release",
         f"--av_manifest=build/minimize/{args.stage}/manifest-projected.json",
         "--gc_sections=n", "--lto=n", "-y"])
    shutil.rmtree(ROOT / "build" / "xmake", ignore_errors=True)
    shutil.rmtree(ROOT / "build" / "artifacts", ignore_errors=True)
    run(["xmake", "build", "qn_pcm_dump"])
    verify = run([sys.executable, "tools/verify_xmake_core.py"], check=False)
    report = json.loads((ROOT / "build" / "ffmpeg-xmake" / "verify" / "report.json").read_text())
    if verify.returncode or report.get("verdict") != "PASS":
        raise SystemExit(f"S6 gate FAILED: PIC archive diverges from oracle ({report.get('verdict')})")

    av_archive = ROOT / "build" / "artifacts" / "libqianqian_av.a"

    # --- 3. link the shipped-core shared library
    manifest = json.loads((stage_dir / "manifest-projected.json").read_text())
    ff_includes = [
        "-I", str(ROOT / manifest["config_root"]),  # generated config headers win
        "-I", str(ROOT / manifest["source_root"]),
    ]
    songcore_pic = stage_dir / "songcore_pic.o"
    run(["gcc", "-fPIC", "-O2", "-I", "include", *ff_includes,
         "-c", "src/songcore_ffmpeg.c", "-o", songcore_pic])
    so = stage_dir / "libqianqian_songcore.so"
    p = run(["gcc", "-shared", "-o", so, songcore_pic, av_archive,
             "-Wl,--version-script=tools/songcore_version.map", "-Wl,--gc-sections",
             "-lm", "-lpthread"], check=False)
    if p.returncode:
        raise SystemExit(
            "S6 FAILED: shared-library link rejected (non-PIC relocations in closure?).\n"
            + p.stderr[-3000:])

    # --- 4a. export gate: exactly the five entry points, versioned
    # (--defined-only: without it, undefined libc imports pollute the set)
    dyn = run(["nm", "-D", "--defined-only", "--with-symbol-versions", str(so)]).stdout
    exported = {}
    for line in dyn.splitlines():
        m = re.search(r"\s([A-Za-z_][A-Za-z0-9_.@]*)$", line.strip())
        if m:
            name = m.group(1)
            base = name.split("@")[0]
            exported[base] = name
    missing = [s for s in CONTRACT if s not in exported]
    unexpected = sorted(set(exported) - set(CONTRACT) - LINKER_NOISE)
    unversioned = [s for s in CONTRACT if "@@QIANQIAN_1.0" not in exported.get(s, "")]
    if missing or unexpected or unversioned:
        raise SystemExit(
            "S6 export gate FAILED: "
            f"missing={missing} unexpected={unexpected} unversioned={unversioned}")

    # --- 4b. functional smoke: consumer linked ONLY against the .so
    consumer = stage_dir / "songcore_so_consumer"
    run(["gcc", "-O2", "-Wall", "-I", "include", "-o", consumer,
         "tools/songcore_so_consumer.c", f"-L{stage_dir}", "-lqianqian_songcore",
         f"-Wl,-rpath,{stage_dir}", "-lm", "-lpthread"])
    smoke = {}
    for rel in SMOKE_FILES:
        p = run([consumer, rel], check=False)
        try:
            data = json.loads(p.stdout.strip().splitlines()[-1])
        except Exception:
            data = {"parse_error": p.stdout[-200:]}
        data["exit_code"] = p.returncode
        smoke[rel] = data
        if p.returncode != 0:
            raise SystemExit(f"S6 smoke FAILED for {rel}: {data}")

    # --- 5. size table: raw / stripped / stripped+xz (shipping = stripped)
    def xz_bytes(path: Path) -> int:
        return len(subprocess.run(["xz", "-c", str(path)], capture_output=True,
                                  check=True).stdout)

    raw = so.stat().st_size
    with tempfile.TemporaryDirectory() as td:
        stripped_copy = Path(td) / so.name
        shutil.copy2(so, stripped_copy)
        run(["strip", str(stripped_copy)])
        stripped = stripped_copy.stat().st_size
        stripped_xz = xz_bytes(stripped_copy)
    raw_xz = xz_bytes(so)
    dynsym_count = run(["nm", "-D", str(so)]).stdout.count("\n")

    result = {
        "schema": 1,
        "stage": args.stage,
        "closure_from": args.from_stage,
        "manifest": f"build/minimize/{args.stage}/manifest-projected.json",
        "pic_codegen_gate": {"corpus_verdict": report.get("verdict"),
                             "cases": report.get("oracle_vs_xmake_cases")},
        "so": str(so.relative_to(ROOT)),
        "so_sha256": sha256_file(so),
        "exported_symbols": sorted(exported),
        "exported_count": len(exported),
        "dynsym_entries": dynsym_count,
        "smoke": smoke,
        "sizes": {
            "libqianqian_songcore_so_bytes": raw,
            "libqianqian_songcore_so_xz_bytes": raw_xz,
            "libqianqian_songcore_so_stripped_bytes": stripped,
            "libqianqian_songcore_so_stripped_xz_bytes": stripped_xz,
            "libqianqian_av_a_bytes": av_archive.stat().st_size,
        },
    }
    (stage_dir / "so.json").write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
    print(json.dumps({k: result[k] for k in ("exported_symbols", "sizes", "smoke")}, indent=2))
    print(f"wrote {stage_dir / 'so.json'}")


if __name__ == "__main__":
    main()
