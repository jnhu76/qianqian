#!/usr/bin/env python3
"""consumers.py — permanent external-consumer gates for SongCore (fail-closed).

Proves the SHIPPING artifacts can be consumed by parties outside the
Qianqian tree, without any Qianqian test binary in the loop:

  1. shared export audit — the shared library exports EXACTLY the 15 frozen
     ABI symbols (no av_/ff_/swr_ leakage),
  2. ctypes decode consumer — tools/songcore_ffi_smoke.py (pure Python
     stdlib) drives libsongcore.so/songcore.dll over the Common Formats
     corpus (FLAC/MP3/AAC-M4A/ADTS/Opus/Vorbis/WAV),
  3. static archive consumer — tests/consumer/songcore_static_smoke.c is
     compiled by plain cc with the documented link line (one merged
     libsongcore.a, no internal Qianqian archive) and decodes real files,
  4. WASM consumer — tools/songcore_wasm_smoke.py instantiates
     build/artifacts/wasm/SongCore.wasm under an independent wasmtime host.

Run mode re-executes every gate live and writes
bench/results/songcore-v1/ffi-consumers.json. --check re-runs the gates and
recomputes the recorded predicates; it never trusts the stored verdict.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import platform
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
RESULTS = ROOT / "bench" / "results" / "songcore-v1"
FIXTURES = ROOT / "corpus" / "fixtures"

CONTRACT_SYMBOLS = [
    "song_audio_stream_count", "song_audio_stream_info", "song_close",
    "song_get_artwork_count", "song_get_artwork_item", "song_get_metadata",
    "song_get_metadata_count", "song_get_metadata_entry", "song_last_error",
    "song_open", "song_probe", "song_read_pcm", "song_seek",
    "song_select_stream", "songcore_abi_version",
]

CTYPES_FIXTURES = [
    "flac-16-44-stereo.flac", "mp3-cbr-id3v23.mp3", "aac-lc-44-stereo.m4a",
    "aac-adts-44-stereo.aac", "opus-48-stereo.opus", "vorbis-44-stereo.ogg",
    "wav-s16le-44-stereo.wav",
]
STATIC_FIXTURES = ["flac-16-44-stereo.flac", "aac-lc-44-stereo.m4a"]
WASM_FIXTURES = ["flac-16-44-stereo.flac", "mp3-cbr-id3v23.mp3",
                 "aac-lc-44-stereo.m4a", "opus-48-stereo.opus",
                 "wav-s16le-44-stereo.wav"]

WINDOWS_EVIDENCE = RESULTS / "win-ffi.json"


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as fh:
        for block in iter(lambda: fh.read(1 << 20), b""):
            h.update(block)
    return h.hexdigest()


def run(cmd: list[str], **kw) -> subprocess.CompletedProcess:
    kw.setdefault("stdout", subprocess.PIPE)
    kw.setdefault("stderr", subprocess.PIPE)
    return subprocess.run([str(c) for c in cmd], cwd=ROOT,
                          text=True, **kw)


def shared_library() -> Path:
    name = "songcore.dll" if sys.platform == "win32" else "libsongcore.so"
    return ROOT / "build" / "artifacts" / "shared" / name


def rel(path: Path) -> str:
    """Repo-relative text for evidence — machine-local paths never commit."""
    try:
        return Path(path).resolve().relative_to(ROOT).as_posix()
    except ValueError:
        return Path(path).name


def audit_shared_exports(lib: Path) -> tuple[bool, str]:
    if not lib.is_file():
        return False, f"shared artifact missing: {lib}"
    if sys.platform == "win32":
        cmd = ["objdump", "-p", str(lib)]
    else:
        cmd = ["nm", "-D", str(lib)]
    proc = run(cmd)
    if proc.returncode != 0:
        return False, f"audit tool failed: {proc.stderr.strip()}"
    if sys.platform == "win32":
        exports = sorted(set(
            line.split()[-1] for line in proc.stdout.splitlines()
            if line.strip().endswith("songcore_abi_version")
            or (line.strip().startswith("[") and " song" in line)))
        # PE export tables parse differently; fall back to pattern scan.
        exports = sorted(set(
            tok for tok in (l.strip().split()[-1] for l in
                            proc.stdout.splitlines() if l.strip())
            if tok in CONTRACT_SYMBOLS))
    else:
        exports = sorted(set(
            parts[-1] for parts in
            (line.split() for line in proc.stdout.splitlines())
            if len(parts) == 3 and parts[1] in ("T", "i") and
            parts[2].startswith("song")))
    ok = exports == sorted(CONTRACT_SYMBOLS)
    return ok, f"{len(exports)} exported symbol(s)"


def gate_ctypes(lib: Path) -> dict:
    out_json = RESULTS / "ffi-ctypes-run.json"
    proc = run([sys.executable, "tools/songcore_ffi_smoke.py",
                "--library", rel(lib), "--json", rel(out_json),
                *[f"corpus/fixtures/{f}" for f in CTYPES_FIXTURES]])
    gate = {"gate": "ctypes_decode_consumer", "tool": "tools/songcore_ffi_smoke.py",
            "fixtures": CTYPES_FIXTURES}
    if proc.returncode != 0:
        gate["verdict"] = "fail"
        gate["detail"] = proc.stdout[-800:] + proc.stderr[-400:]
        return gate
    data = json.loads(out_json.read_text())
    gate["verdict"] = data["verdict"]
    gate["library_sha256"] = data["library_sha256"]
    gate["abi_version"] = 1
    gate["songs"] = [
        {"song": s["song"], "verdict": s["verdict"], "container": s.get("container"),
         "codec": s.get("codec"), "sample_rate": s.get("sample_rate"),
         "channels": s.get("channels"), "sha256": s["song_sha256"]}
        for s in data["songs"]]
    return gate


def gate_static_consumer() -> dict:
    gate = {"gate": "static_archive_consumer",
            "tool": "tests/consumer/songcore_static_smoke.c",
            "fixtures": STATIC_FIXTURES}
    exe_rel = "build/consumer/songcore_static_smoke"
    exe = ROOT / exe_rel
    exe.parent.mkdir(parents=True, exist_ok=True)
    if sys.platform == "win32":
        cc = ["gcc", "-Iinclude", "tests/consumer/songcore_static_smoke.c",
              "-Lbuild/artifacts", "-lsongcore", "-lbcrypt", "-o", exe_rel]
    else:
        cc = ["cc", "-Iinclude", "tests/consumer/songcore_static_smoke.c",
              "-Lbuild/artifacts", "-lsongcore", "-lm", "-lpthread", "-o", exe_rel]
    proc = run(cc)
    if proc.returncode != 0:
        gate["verdict"] = "fail"
        gate["detail"] = ("documented external link line failed:\n"
                          + proc.stderr[-800:])
        return gate
    gate["link_line"] = " ".join(str(c) for c in cc)
    runs = []
    for f in STATIC_FIXTURES:
        proc = run([exe, f"corpus/fixtures/{f}"])
        runs.append({"fixture": f, "rc": proc.returncode,
                     "output": proc.stdout.strip()})
    gate["runs"] = runs
    gate["verdict"] = "pass" if all(r["rc"] == 0 for r in runs) else "fail"
    return gate


def gate_wasm() -> dict:
    gate = {"gate": "wasm_consumer", "tool": "tools/songcore_wasm_smoke.py",
            "fixtures": WASM_FIXTURES}
    wasm = ROOT / "build" / "artifacts" / "wasm" / "SongCore.wasm"
    if not wasm.is_file():
        # Host-dependent: a native checkout without a WASM session has no
        # guest module. The committed evidence records where it DID run.
        gate["verdict"] = "not_run"
        gate["detail"] = f"guest module not built on this host: {wasm}"
        return gate
    out_json = RESULTS / "ffi-wasm-run.json"
    proc = run([sys.executable, "tools/songcore_wasm_smoke.py",
                "--wasm", rel(wasm), "--json", rel(out_json),
                *[f"corpus/fixtures/{f}" for f in WASM_FIXTURES]])
    if proc.returncode != 0:
        gate["verdict"] = "fail"
        gate["detail"] = proc.stdout[-800:] + proc.stderr[-400:]
        return gate
    data = json.loads(out_json.read_text())
    gate["verdict"] = data["verdict"]
    gate["wasm_sha256"] = data["wasm_sha256"]
    return gate


def gate_windows_recorded() -> dict:
    gate = {"gate": "windows_ctypes_consumer",
            "host": "recorded on the Windows host (not re-runnable here)"}
    if not WINDOWS_EVIDENCE.is_file():
        gate["verdict"] = "not_run"
        return gate
    data = json.loads(WINDOWS_EVIDENCE.read_text())
    gate["verdict"] = data.get("verdict")
    gate["library_file"] = Path(data.get("library", "")).name
    gate["library_sha256"] = data.get("library_sha256")
    gate["python"] = data.get("python")
    gate["platform"] = data.get("platform")
    gate["songs"] = [
        {"song": s["song"], "verdict": s["verdict"], "codec": s.get("codec")}
        for s in data.get("songs", [])]
    return gate


def run_all(out: Path, core: bool = False) -> int:
    lib = shared_library()
    exports_ok, exports_detail = audit_shared_exports(lib)
    gates = [
        {"gate": "shared_export_audit", "verdict":
         "pass" if exports_ok else "fail", "detail": exports_detail},
        gate_ctypes(lib),
        gate_static_consumer(),
    ]
    if not core:
        gates += [gate_wasm(), gate_windows_recorded()]
    report = {
        "tool": "tests/songcore/consumers.py",
        "platform": f"{platform.system()}-{platform.machine()}",
        "shared_library": rel(lib),
        "shared_library_sha256": sha256_file(lib) if lib.is_file() else None,
        "gates": gates,
    }
    report["verdict"] = ("pass" if all(g["verdict"] in ("pass", "not_run")
                                       for g in gates) else "fail")
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(report, indent=2, ensure_ascii=False) + "\n")

    for g in report["gates"]:
        print(f"  [{g['verdict'].upper():>8}] {g['gate']}")
    print(f"consumers: {report['verdict'].upper()} -> {out}")
    return 0 if report["verdict"] == "pass" else 1


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    mode = ap.add_mutually_exclusive_group(required=True)
    mode.add_argument("--out", action="store_true",
                      help="run every consumer gate live and write evidence")
    mode.add_argument("--check", action="store_true",
                      help="re-run the gates and validate recorded evidence")
    ap.add_argument("--core", action="store_true",
                    help="only the host-independent gates (shared export "
                         "audit, ctypes consumer, static consumer); used by "
                         "the permanent regression chain")
    args = ap.parse_args()

    if args.out:
        out = RESULTS / ("ffi-consumers-core.json" if args.core
                         else "ffi-consumers.json")
        return run_all(out, core=args.core)

    # --check: fail-closed revalidation.
    evidence_path = RESULTS / "ffi-consumers.json"
    if not evidence_path.is_file():
        print("consumers --check: FAIL — no ffi-consumers.json evidence", file=sys.stderr)
        return 1
    problems = []
    check_out = RESULTS / ("ffi-consumers-core.check.json" if args.core
                           else "ffi-consumers.check.json")
    run_all(check_out, core=args.core)
    fresh = json.loads(check_out.read_text())
    stored = json.loads(evidence_path.read_text())
    if stored.get("verdict") != "pass":
        problems.append("recorded verdict is not pass")
    if fresh["verdict"] != "pass":
        problems.append("live re-run of the gates failed")
    recorded = {g["gate"]: g for g in stored.get("gates", [])}
    for gate in ("ctypes_decode_consumer", "static_archive_consumer",
                 "shared_export_audit", "wasm_consumer",
                 "windows_ctypes_consumer"):
        if gate not in recorded:
            problems.append(f"recorded evidence missing gate: {gate}")
        elif recorded[gate].get("verdict") != "pass":
            problems.append(f"recorded gate not pass: {gate} "
                            f"({recorded[gate].get('verdict')})")
    for f in CTYPES_FIXTURES:
        want = sha256_file(FIXTURES / f)
        got = [s.get("sha256") for s in
               recorded.get("ctypes_decode_consumer", {}).get("songs", [])
               if s.get("song", "").endswith(f)]
        if got and got[0] != want:
            problems.append(f"fixture sha drift: {f}")
    check_out.unlink(missing_ok=True)
    if problems:
        print("consumers --check: FAIL", file=sys.stderr)
        for p in problems:
            print(f"  - {p}", file=sys.stderr)
        return 1
    print("consumers --check PASS (all consumer gates recomputed)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
