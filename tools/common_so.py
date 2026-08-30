#!/usr/bin/env python3
"""Size-minimal shared-core build (libqianqian_songcore.so) for a ladder stage.

For stage `N` (c0..c5) this derives a PIC + -Os + function/data-sections
projection of the stage's accepted closure, clean-rebuilds it through Xmake,
re-runs the full behavior gate on the PIC archive, then links

    libqianqian_songcore.so = songcore_pic.o + libqianqian_av.a
                              + version script + -Wl,--gc-sections

and hard-gates the dynamic export table to exactly the five SongCore entry
points. A consumer binary linked ONLY against the .so drives the full
contract across one clean fixture per format family.

Running every ladder stage through the same .so recipe yields the shipping
bytes delta per capability increment; the final C6 additionally gets an
-Os+LTO variant.

    python3 tools/common_so.py --stage c5-so --from-stage c5 [--lto]
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
sys.path.insert(0, str(ROOT / "tools"))
from common_corpus import fixture_path, load_cases, STAGE_CAPABILITIES  # noqa: E402

CONTRACT = ["song_open", "song_probe", "song_read_pcm", "song_seek", "song_close"]
LINKER_NOISE = {"__bss_start", "_edata", "_end", "__TMC_END__", "_init", "_fini",
                "__data_start", "QIANQIAN_1.0", "data_start"}
SMOKE_CASES = [
    "mp3-cbr-id3v23", "flac-16-44-stereo",
    "aac-lc-44-stereo",
    "alac-16-44-stereo", "wav-s16le-44-stereo",
    "vorbis-44-stereo", "opus-48-stereo",
    # NOTE: aac-adts-44-stereo is deliberately not smoked: FFmpeg's raw ADTS
    # demuxer has no seek implementation (av_seek_frame fails by design).
    # Recorded as a capability finding; ADTS decode/PCM is still gated in
    # full at the stage level.
]


def run(cmd: list[str], *, check=True, binary=False) -> subprocess.CompletedProcess:
    print("+", " ".join(map(str, cmd)), flush=True)
    p = subprocess.run(list(map(str, cmd)), cwd=ROOT, capture_output=True, text=not binary)
    if check and p.returncode:
        err = p.stderr if not binary else p.stderr.decode("utf-8", "replace")
        raise SystemExit(f"command failed ({p.returncode}): {' '.join(map(str, cmd))}\n{err[-4000:]}")
    return p


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for block in iter(lambda: f.read(1 << 20), b""):
            h.update(block)
    return h.hexdigest()


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--stage", required=True, help="e.g. c5-so")
    ap.add_argument("--from-stage", required=True, help="stage whose projected closure is used")
    ap.add_argument("--lto", action="store_true")
    args = ap.parse_args()

    stage_dir = ROOT / "build" / "minimize" / args.stage
    stage_dir.mkdir(parents=True, exist_ok=True)

    # --- 1. PIC + size codegen projection of the accepted closure
    flags = ["--add-flag=-fPIC", "--add-flag=-ffunction-sections",
             "--add-flag=-fdata-sections", "--replace-opt", "Os"]
    if args.lto:
        flags += ["--add-flag=-flto"]
    run([sys.executable, "tools/minimize_flags.py", "--stage", args.stage,
         "--from-stage", args.from_stage, *flags])

    # --- 2. clean rebuild + full behavior gate on the PIC archive
    run(["xmake", "f", "-m", "release",
         f"--av_manifest=build/minimize/{args.stage}/manifest-projected.json",
         "--gc_sections=n", "--lto=" + ("y" if args.lto else "n"), "-y"])
    shutil.rmtree(ROOT / "build" / "xmake", ignore_errors=True)
    shutil.rmtree(ROOT / "build" / "artifacts", ignore_errors=True)
    run(["xmake", "build", "qn_pcm_dump"])
    run([sys.executable, "tools/common_gate.py", "--stage", args.stage])

    av_archive = ROOT / "build" / "artifacts" / "libqianqian_av.a"

    # --- 3. link the shipped-core shared library
    manifest = json.loads((stage_dir / "manifest-projected.json").read_text())
    ff_includes = ["-I", str(ROOT / manifest["config_root"]),
                   "-I", str(ROOT / manifest["source_root"])]
    opt = "-Os" if not args.lto else "-Os -flto"
    songcore_pic = stage_dir / "songcore_pic.o"
    run(["gcc", "-fPIC", *opt.split(), "-ffunction-sections", "-fdata-sections",
         "-I", "include", *ff_includes,
         "-c", "src/songcore_ffmpeg.c", "-o", str(songcore_pic)])
    so = stage_dir / "libqianqian_songcore.so"
    p = run(["gcc", "-shared", *opt.split(), "-o", str(so), str(songcore_pic),
             str(av_archive),
             "-Wl,--version-script=tools/songcore_version.map", "-Wl,--gc-sections",
             "-lm", "-lpthread"], check=False)
    if p.returncode:
        raise SystemExit(f"{args.stage}: shared-library link rejected.\n" + p.stderr[-3000:])

    # --- 4a. export gate: exactly the five entry points, versioned
    dyn = run(["nm", "-D", "--defined-only", "--with-symbol-versions", str(so)]).stdout
    exported = {}
    for line in dyn.splitlines():
        m = re.search(r"\s([A-Za-z_][A-Za-z0-9_.@]*)$", line.strip())
        if m:
            exported[m.group(1).split("@")[0]] = m.group(1)
    missing = [s for s in CONTRACT if s not in exported]
    unexpected = sorted(set(exported) - set(CONTRACT) - LINKER_NOISE)
    unversioned = [s for s in CONTRACT if "@@QIANQIAN_1.0" not in exported.get(s, "")]
    if missing or unexpected or unversioned:
        raise SystemExit(f"{args.stage} export gate FAILED: "
                         f"missing={missing} unexpected={unexpected} unversioned={unversioned}")

    # --- 4b. functional smoke: consumer linked ONLY against the .so
    consumer = stage_dir / "songcore_so_consumer"
    run(["gcc", "-O2", "-Wall", "-I", "include", "-o", str(consumer),
         "tools/songcore_so_consumer.c", f"-L{stage_dir}", "-lqianqian_songcore",
         f"-Wl,-rpath,{stage_dir}", "-lm", "-lpthread"])
    base_stage = args.from_stage.split("-so")[0]
    cases = {c["id"]: c for c in load_cases(STAGE_CAPABILITIES[base_stage])}
    smoke = {}
    for cid in SMOKE_CASES:
        if cid not in cases:
            continue  # capability not present at this stage
        p = run([str(consumer), str(fixture_path(cases[cid]))], check=False)
        try:
            data = json.loads(p.stdout.strip().splitlines()[-1])
        except Exception:
            data = {"parse_error": p.stdout[-200:]}
        data["exit_code"] = p.returncode
        smoke[cid] = data
        if p.returncode != 0:
            raise SystemExit(f"{args.stage} smoke FAILED for {cid}: {data}")

    # --- 5. size table + identity
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
    ldd = run(["ldd", str(so)], check=False).stdout.strip()
    if "avcodec" in ldd or "avformat" in ldd or "avutil" in ldd:
        raise SystemExit(f"{args.stage}: .so dynamically depends on system FFmpeg: {ldd}")

    api = sorted(s for s in exported if s in CONTRACT)
    result = {
        "schema": 1,
        "stage": args.stage,
        "closure_from": args.from_stage,
        "lto": args.lto,
        "manifest": f"build/minimize/{args.stage}/manifest-projected.json",
        "pic_codegen_gate": {"stage": args.stage},
        "so": str(so.relative_to(ROOT)),
        "so_sha256": sha256_file(so),
        "exported_api_symbols": api,
        "exported_api_count": len(api),
        "defined_dynsym_entries": sorted(exported),
        "defined_dynsym_count": len(exported),
        "smoke": smoke,
        "dynamic_dependencies": ldd,
        "sizes": {
            "libqianqian_songcore_so_bytes": raw,
            "libqianqian_songcore_so_xz_bytes": raw_xz,
            "libqianqian_songcore_so_stripped_bytes": stripped,
            "libqianqian_songcore_so_stripped_xz_bytes": stripped_xz,
            "libqianqian_av_a_bytes": av_archive.stat().st_size,
        },
    }
    (stage_dir / "so.json").write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
    print(json.dumps({k: result[k] for k in
                      ("exported_api_symbols", "defined_dynsym_count", "sizes")},
                     indent=2, sort_keys=True))
    print(f"wrote {stage_dir / 'so.json'}")


if __name__ == "__main__":
    main()
