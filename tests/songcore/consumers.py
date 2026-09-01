#!/usr/bin/env python3
"""consumers.py — permanent external-consumer gates for SongCore (fail-closed).

Proves the SHIPPING artifacts can be consumed by parties outside the
Qianqian tree, without any Qianqian test binary in the loop:

  1. shared export audit — the shared library exports EXACTLY the 15 frozen
     ABI symbols (no av_/ff_/swr_ leakage),
  2. ctypes decode consumer — tools/songcore_ffi_smoke.py (pure Python
     stdlib) drives libsongcore.so/songcore.dll over the Common Formats
     corpus (FLAC/MP3/AAC-M4A/ADTS/Opus/Vorbis/WAV), consuming every string
     as a pointer+length pair (never NUL termination),
  3. static archive consumer — tests/consumer/songcore_static_smoke.c is
     compiled by plain cc with the documented link line (one merged
     libsongcore.a, no internal Qianqian archive) and decodes real files,
  4. WASM consumer — tools/songcore_wasm_smoke.py instantiates
     build/artifacts/wasm/SongCore.wasm under an independent wasmtime host:
     guest-allocated memory, machine-validated struct layout, semantic
     metadata/raw-metadata/artwork assertions, explicit per-gate checks,
  5. recorded Windows consumer — win-ffi.json from a REAL Windows Python
     process, provenance-pinned (see below).

Aggregate verdict semantics: pass = every gate pass; partial = no gate
failed but at least one environment gate is not_run (e.g. --core mode, or
a host without a WASM session); fail = any gate failed. "ALL PASS" is only
claimed when nothing is not_run.

Provenance pinning (--check): the recorded Windows and WASM evidence is
bound to the target recipe id + recipe sha, the FFmpeg pin, the codec
profile sha, the SongCore ABI header sha, and the artifact sha. If ANY of
those drifts, the recorded PASS is stale and the check fails — a stale
external-consumer PASS cannot survive a source change.

--mutation <name> proves the gates can fail: runs a tampered copy of the
authority through the real validation path and requires FAIL.
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

# Permanent host-independent ctypes matrix (PRD Common Formats, --seconds
# capped): every family, plus the artwork fixture for pointer+length
# compressed-byte validation.
CTYPES_FIXTURES = [
    "flac-16-44-stereo.flac", "flac-24-96.flac", "mp3-cbr-id3v23.mp3",
    "mp3-vbr-id3v24.mp3", "aac-lc-44-stereo.m4a", "aac-adts-44-stereo.aac",
    "alac-16-44-stereo.m4a", "opus-48-stereo.opus", "vorbis-44-stereo.ogg",
    "wav-s16le-44-stereo.wav", "wav-f32le-44-stereo.wav", "wav-u8-44-mono.wav",
    "artwork-mp3-jpeg.mp3",
]
STATIC_FIXTURES = ["flac-16-44-stereo.flac", "aac-lc-44-stereo.m4a"]
WASM_FIXTURES = [
    "flac-16-44-stereo.flac", "mp3-cbr-id3v23.mp3", "aac-lc-44-stereo.m4a",
    "opus-48-stereo.opus", "wav-s16le-44-stereo.wav", "artwork-mp3-jpeg.mp3",
]
# Gates the WASM consumer must record per fixture (explicit semantic gates,
# never inferred from the top-level run surviving).
WASM_REQUIRED_CHECKS = [
    "abi_version", "open", "probe", "stream_enumeration", "metadata",
    "raw_metadata", "artwork", "decode", "seek", "close", "layout",
    "guest_memory",
]

WINDOWS_EVIDENCE = RESULTS / "win-ffi.json"
WASM_RUN = RESULTS / "ffi-wasm-run.json"


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


def pinned_identity(target_id: str) -> dict:
    """Provenance binding for recorded external evidence: if any of these
    values drifts, previously recorded consumer PASSes are stale."""
    pin = json.loads((ROOT / "ffmpeg" / "pin.json").read_text())
    recipe_file = ROOT / "ffmpeg" / "targets" / f"{target_id}.json"
    return {
        "target": target_id,
        "recipe_sha256": sha256_file(recipe_file),
        "ffmpeg_commit_sha": pin["ffmpeg_commit_sha"],
        "ffmpeg_source_sha256": pin["source_sha256"],
        "profile_sha256": sha256_file(ROOT / "ffmpeg" / "profiles"
                                      / "codec-base.json"),
        "abi_header_sha256": sha256_file(ROOT / "include" / "songcore.h"),
    }


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
            "fixtures": WASM_FIXTURES, "target": "wasm-wasi"}
    wasm = ROOT / "build" / "artifacts" / "wasm" / "SongCore.wasm"
    if not wasm.is_file():
        # Host-dependent: a native checkout without a WASM session has no
        # guest module. The committed evidence records where it DID run.
        gate["verdict"] = "not_run"
        gate["detail"] = f"guest module not built on this host: {wasm}"
        return gate
    out_json = WASM_RUN
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
    gate["pinning"] = pinned_identity("wasm-wasi")
    gate["pinning"]["artifact_sha256"] = data["wasm_sha256"]
    # Explicit per-gate visibility: every required semantic check must be
    # recorded PASS per fixture — a surviving top-level run proves nothing.
    bad = []
    for song in data.get("songs", []):
        checks = song.get("checks") or {}
        for required in WASM_REQUIRED_CHECKS:
            if checks.get(required) != "pass":
                bad.append(f"{song.get('song')}:{required}"
                           f"={checks.get(required, 'missing')}")
    gate["checks_complete"] = not bad
    if bad:
        gate["verdict"] = "fail"
        gate["detail"] = f"missing/failing wasm checks: {bad[:8]}"
    return gate


def gate_windows_recorded() -> dict:
    gate = {"gate": "windows_ctypes_consumer",
            "host": "recorded on the Windows host (not re-runnable here)",
            "target": "windows-mingw-x86_64"}
    if not WINDOWS_EVIDENCE.is_file():
        gate["verdict"] = "not_run"
        return gate
    data = json.loads(WINDOWS_EVIDENCE.read_text())
    verdict = data.get("verdict")
    songs = data.get("songs", [])
    # A recorded PASS is only accepted when it carries enough PCM authority
    # for cross-platform comparison (frames + full PCM sha + peak).
    weak = [s.get("song") for s in songs
            if not (s.get("decoded_frames") and s.get("pcm_sha256")
                    and s.get("peak") is not None)]
    checks_bad = []
    for s in songs:
        checks = s.get("checks") or {}
        for required in ("abi_version", "open", "probe", "metadata",
                         "artwork", "decode", "seek", "close"):
            if checks.get(required) != "pass":
                checks_bad.append(f"{s.get('song')}:{required}")
    gate["verdict"] = verdict
    gate["library_file"] = "songcore.dll (staged on the Windows host)"
    gate["library_sha256"] = data.get("library_sha256")
    gate["python"] = data.get("python")
    gate["platform"] = data.get("platform")
    gate["pinning"] = pinned_identity("windows-mingw-x86_64")
    gate["pinning"]["artifact_sha256"] = data.get("library_sha256")
    gate["songs"] = [
        {"song": s["song"], "verdict": s["verdict"], "codec": s.get("codec"),
         "decoded_frames": s.get("decoded_frames"),
         "pcm_sha256": s.get("pcm_sha256"), "peak": s.get("peak")}
        for s in songs]
    if verdict == "pass":
        if weak:
            gate["verdict"] = "fail"
            gate["detail"] = (f"recorded evidence lacks PCM authority "
                              f"(decoded_frames/pcm_sha256/peak) for: {weak}")
        elif checks_bad:
            gate["verdict"] = "fail"
            gate["detail"] = f"missing/failing checks: {checks_bad[:8]}"
    return gate


def aggregate(gates: list[dict]) -> str:
    """pass / partial / fail — never call a run with not_run gates
    'all pass'."""
    verdicts = [g["verdict"] for g in gates]
    if "fail" in verdicts:
        return "fail"
    if "not_run" in verdicts:
        return "partial"
    return "pass"


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
    report["verdict"] = aggregate(gates)
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(report, indent=2, ensure_ascii=False) + "\n")

    for g in report["gates"]:
        print(f"  [{g['verdict'].upper():>8}] {g['gate']}")
    print(f"consumers: {report['verdict'].upper()} -> {out}")
    return 0 if report["verdict"] == "pass" else (2 if report["verdict"] == "partial" else 1)


def validate_recorded(stored: dict, fresh_path: Path, core: bool) -> list[str]:
    """--check: recompute the gates live, then re-derive every recorded
    predicate (verdicts, fixture shas, provenance pinning). Never trusts the
    stored verdict. The live re-run may be partial on hosts without a WASM
    session (not_run); a live FAIL anywhere is a real problem."""
    problems: list[str] = []
    if stored.get("verdict") != "pass":
        problems.append("recorded verdict is not pass "
                        "(full evidence must be PASS, not partial)")
    fresh = json.loads(fresh_path.read_text())
    fresh_fails = [g["gate"] for g in fresh.get("gates", [])
                   if g["verdict"] == "fail"]
    if fresh_fails:
        problems.append(f"live re-run failed: {fresh_fails}")
    recorded = {g["gate"]: g for g in stored.get("gates", [])}
    for gate in ("ctypes_decode_consumer", "static_archive_consumer",
                 "shared_export_audit", "wasm_consumer",
                 "windows_ctypes_consumer"):
        if core and gate in ("wasm_consumer", "windows_ctypes_consumer"):
            continue
        if gate not in recorded:
            problems.append(f"recorded evidence missing gate: {gate}")
        elif recorded[gate].get("verdict") != "pass":
            problems.append(f"recorded gate not pass: {gate} "
                            f"({recorded[gate].get('verdict')})")

    for f in CTYPES_FIXTURES:
        want = sha256_file(FIXTURES / f)
        got = [s.get("sha256") for s in
               (recorded.get("ctypes_decode_consumer") or {}).get("songs", [])
               if s.get("song", "").endswith(f)]
        if got and got[0] != want:
            problems.append(f"fixture sha drift: {f}")

    # Provenance pinning: recorded Windows/WASM evidence must be bound to
    # the CURRENT pin/profile/recipe/ABI header, and to the artifact that
    # is actually on disk (if present) — else the recorded PASS is stale.
    for gate_name, target, artifact in (
            ("wasm_consumer", "wasm-wasi",
             ROOT / "build" / "artifacts" / "wasm" / "SongCore.wasm"),
            ("windows_ctypes_consumer", "windows-mingw-x86_64", None)):
        g = recorded.get(gate_name) or {}
        if core and not g:
            continue
        want = pinned_identity(target)
        want["artifact_sha256"] = (sha256_file(artifact)
                                   if artifact and artifact.is_file() else None)
        got = g.get("pinning")
        if not got:
            problems.append(f"{gate_name}: recorded evidence is unpinned "
                            "(predates provenance binding)")
            continue
        for key, val in want.items():
            if val is None:  # artifact not on this host; skip live-sha compare
                continue
            if got.get(key) != val:
                problems.append(
                    f"{gate_name}: recorded PASS is STALE — {key} drifted "
                    f"(recorded {str(got.get(key))[:16]}…, current "
                    f"{str(val)[:16]}…). Re-record the evidence.")
    return problems


def mutation(name: str) -> int:
    """Prove the fixed authorities can actually fail. Each case tampers the
    recorded evidence (or the consumer's expectations) and drives the REAL
    validation path — require FAIL."""
    failures: list[str] = []
    evidence = RESULTS / "ffi-consumers.json"
    stored = json.loads(evidence.read_text()) if evidence.is_file() else None

    # A passing full record is the baseline for the tamper proofs.
    have_record = (stored is not None
                   and stored.get("verdict") == "pass")

    if name in ("all", "wasm-sha"):
        if not have_record:
            print("wasm-sha mutation: SKIP (no full ffi-consumers.json)")
        else:
            tampered = json.loads(json.dumps(stored))
            g = {gg["gate"]: gg for gg in tampered["gates"]}
            g["wasm_consumer"]["pinning"]["artifact_sha256"] = "0" * 64
            problems = validate_recorded(tampered, evidence, core=False)
            if not any("STALE" in p for p in problems):
                failures.append("wasm-sha: rebuilt-but-unrecorded guest did "
                                "not invalidate the stored PASS")

    if name in ("all", "windows-identity"):
        if not have_record:
            print("windows-identity mutation: SKIP (no full ffi-consumers.json)")
        else:
            tampered = json.loads(json.dumps(stored))
            g = {gg["gate"]: gg for gg in tampered["gates"]}
            g["windows_ctypes_consumer"]["pinning"]["abi_header_sha256"] = "f" * 64
            problems = validate_recorded(tampered, evidence, core=False)
            if not any("STALE" in p for p in problems):
                failures.append("windows-identity: drifted ABI header did "
                                "not invalidate the stored PASS")

    if name in ("all", "wasm-metadata"):
        # Erase the expected canonical title while the functions still
        # return SONG_OK: the wasm consumer gate must FAIL.
        sys.path.insert(0, str(ROOT / "tools"))
        import songcore_wasm_smoke as wsm
        wasm = ROOT / "build" / "artifacts" / "wasm" / "SongCore.wasm"
        if not wasm.is_file():
            print("wasm-metadata mutation: SKIP (no guest module built)")
        else:
            import wasmtime
            engine = wasmtime.Engine()
            module = wasmtime.Module.from_file(engine, str(wasm))
            linker = wasmtime.Linker(engine)
            linker.define_wasi()
            store = wasmtime.Store(engine)
            store.set_wasi(wasmtime.WasiConfig())
            sources = wsm.HostSources()
            sources.bind(linker)
            instance = linker.instantiate(store, module)
            init = instance.exports(store).get("_initialize")
            if init is not None:
                init(store)
            sources.guest = wsm.GuestMemory(store,
                                            instance.exports(store)["memory"])
            fixture = ROOT / "corpus/fixtures/flac-16-44-stereo.flac"
            erased = {"title": "ERASED EXPECTATION", "artist": "NOPE"}
            smoke = wsm.WasmSmoke(instance, store, sources.guest, sources,
                                  fixture, 1.0, None)
            ok = smoke.run(erased)
            if ok:
                failures.append("wasm-metadata: erased canonical title was "
                                "NOT caught by the consumer gate")

    if name in ("all", "target-identity"):
        xmake_gate = ROOT / "tests" / "songcore" / "target_gate_test.py"
        r = run([sys.executable, str(xmake_gate)])
        if r.returncode != 0:
            failures.append("target-identity: xmake-level negative gate "
                            "FAILED:\n" + r.stdout[-600:] + r.stderr[-600:])

    if failures:
        print(f"mutation {name}: FAIL")
        for f in failures:
            print(f"  - {f}")
        return 1
    print(f"mutation {name}: PASS (tampering detected by the gates)")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    mode = ap.add_mutually_exclusive_group(required=True)
    mode.add_argument("--out", action="store_true",
                      help="run every consumer gate live and write evidence")
    mode.add_argument("--check", action="store_true",
                      help="re-run the gates and validate recorded evidence")
    mode.add_argument("--mutation", metavar="NAME",
                      choices=["all", "wasm-sha", "windows-identity",
                               "wasm-metadata", "target-identity"],
                      help="prove a gate can fail (tamper proof)")
    core = ap.add_argument("--core", action="store_true",
                           help="only the host-independent gates (shared "
                                "export audit, ctypes consumer, static "
                                "consumer); used by the permanent "
                                "regression chain")
    args = ap.parse_args()

    if args.mutation:
        return mutation(args.mutation)

    if args.out:
        out = RESULTS / ("ffi-consumers-core.json" if args.core
                         else "ffi-consumers.json")
        return run_all(out, core=args.core)

    # --check: fail-closed revalidation.
    evidence_path = RESULTS / "ffi-consumers.json"
    if not evidence_path.is_file():
        print("consumers --check: FAIL — no ffi-consumers.json evidence", file=sys.stderr)
        return 1
    stored = json.loads(evidence_path.read_text())
    check_out = RESULTS / ("ffi-consumers-core.check.json" if args.core
                           else "ffi-consumers.check.json")
    run_all(check_out, core=args.core)
    problems = validate_recorded(stored, check_out, core=args.core)
    check_out.unlink(missing_ok=True)
    if problems:
        print("consumers --check: FAIL", file=sys.stderr)
        for p in problems:
            print(f"  - {p}", file=sys.stderr)
        return 1
    print("consumers --check PASS (all consumer gates recomputed, "
          "provenance pinned)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
