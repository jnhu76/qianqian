#!/usr/bin/env python3
"""Windows x86_64 target-specific Common Formats core (issue #8 phase 2).

Runs ONLY after the Linux ladder is frozen. Every artifact here derives from
a WINDOWS configure oracle (cross-run with LLVM-MinGW from WSL against the
same pinned FFmpeg); the Linux manifest is never reused.

Toolchain: llvm-mingw (clang + lld + llvm binutils), pinned by directory;
target triple x86_64-w64-mingw32, UCRT runtime. The PE binaries are
executed natively on Windows through WSL interop (`./tool.exe` from WSL
launches the Windows loader) — correctness never comes from Wine or from
unexecuted artifacts.

Pipeline (--all), mirroring the Linux ladder's TU projection:

  --import     Windows configure/Make oracle -> target-specific manifest
  --build      Xmake mingw replay of the FULL closure -> libqianqian_av.a
  --audit      COFF link-reachability (offset-identified members, llvm-nm
               semantics) + reduced-archive PE content proof + lld -Map
               corroboration   (tools/link_audit_windows.py)
  --project    manifest projection to the reachable closure
               (tools/minimize_manifest.py)
  --projected  clean rebuild of the projected closure ONLY, with a hard
               per-TU verification that xmake compiled exactly the projected
               units (no more, no less), then re-audit to fixpoint
  --dll        shipping variants (qianqian_songcore.dll) built from the
               projected closure: -Os and -Os+LTO, each with export/import
               hard gates
  --correct    Windows-native correctness per DLL variant: full applicable
               corpus — clean cases under the STRICT/LAPPED/UNSUPPORTED seek
               contract, degraded (truncated/malformed) cases under typed
               boundedness gates — plus wide-path / >2GiB host gates

    python3 tools/common_windows.py --all
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tools"))
from common_corpus import (  # noqa: E402
    STAGE_CAPABILITIES, fixture_path, load_cases, sha256_file,
)
import common_import  # noqa: E402

SDK = Path(os.environ.get("LLVM_MINGW_SDK", Path.home() / "toolchains/llvm-mingw"))
STAGE = "win-c6"
STAGE_DIR = ROOT / "build" / "minimize" / STAGE
CONTRACT = ["song_open", "song_probe", "song_read_pcm", "song_seek", "song_close"]
FORBIDDEN_DLL = re.compile(r"(avcodec|avformat|avutil|swresample|swscale|avfilter)", re.I)

FULL_MANIFEST = STAGE_DIR / "manifest.json"
PROJECTED_MANIFEST = STAGE_DIR / "manifest-projected.json"

# Seek contract per format family (gate-owned minimum semantics; calibrate
# only observes). STRICT requires exact suffix equality; LAPPED accepts
# codec-lapping suffix differences (MP3 bit reservoir, AAC MDCT 50% overlap,
# CELT overlap-add) but still demands seek success + bounded resume + PCM +
# clean EOF; UNSUPPORTED (raw ADTS) accepts the typed seek failure while
# decode/EOF gates still hold.
SEEK_TIER = {"flac": "STRICT", "alac": "STRICT", "wav": "STRICT", "vorbis": "STRICT",
             "mp3": "LAPPED", "aac": "LAPPED", "opus": "LAPPED"}
TYPED_OUTCOMES = {"OPEN_FAILED", "PROBE_FAILED", "DECODE_ERROR", "DEGRADED_EOF",
                  "CAPPED_OUTPUT"}


def sdk_env() -> dict:
    if not (SDK / "bin").is_dir():
        raise SystemExit(f"llvm-mingw SDK not found at {SDK}; set LLVM_MINGW_SDK")
    e = dict(os.environ)
    e["PATH"] = f"{SDK / 'bin'}:{os.environ['PATH']}"
    return e


def run(cmd: list[str], *, check=True, binary=False, env=None, timeout=None) -> subprocess.CompletedProcess:
    print("+", " ".join(map(str, cmd)), flush=True)
    # every child needs the llvm-mingw tools on PATH (xmake resolves the
    # pinned x86_64-w64-mingw32-* wrappers through it)
    e = sdk_env()
    if env:
        e.update(env)
    p = subprocess.run(list(map(str, cmd)), cwd=ROOT, capture_output=True,
                       text=not binary, env=e, timeout=timeout)
    if check and p.returncode:
        err = p.stderr if not binary else p.stderr.decode("utf-8", "replace")
        out = p.stdout if not binary else p.stdout.decode("utf-8", "replace")
        detail = (err.strip() or out.strip())[-4000:]
        raise SystemExit(f"command failed ({p.returncode}): {' '.join(map(str, cmd))}\n{detail}")
    return p


def last_json(stdout: str):
    lines = [l for l in stdout.splitlines() if l.strip()]
    return json.loads(lines[-1])


def import_windows() -> None:
    """Windows configure/Make oracle -> target-specific compile manifest."""
    run([sys.executable, "tools/common_import.py", "--stage", STAGE,
         "--profile", "bench/profiles/c5-opus.json", "--force",
         "--configure-extra=--enable-cross-compile",
         "--configure-extra=--cross-prefix=x86_64-w64-mingw32-",
         "--configure-extra=--target-os=mingw32",
         "--configure-extra=--arch=x86_64"],
        env=sdk_env())
    manifest = json.loads(FULL_MANIFEST.read_text())
    print(f"windows closure: {manifest['closure']['translation_units']} TUs; "
          f"toolchain: {manifest['toolchain']}")
    # identity: the cross toolchain must be pinned in the recorded CC, and
    # the mingw32 target must be part of the recorded configure arguments
    if "mingw32" not in manifest["toolchain"]["cc"]:
        raise SystemExit(f"oracle CC is not the mingw cross compiler: {manifest['toolchain']}")
    if "--target-os=mingw32" not in manifest["configure_args"]:
        raise SystemExit("configure_args missing --target-os=mingw32")
    if "ARCH_X86_64=yes" not in (STAGE_DIR / "oracle" / "ffbuild" / "config.mak").read_text():
        raise SystemExit("oracle did not configure for x86_64")


def _xmake_configure(av_manifest: Path, *, lto: bool) -> None:
    """Replay a manifest through Xmake's mingw platform.

    The x86_64 tools are pinned explicitly: llvm-mingw ships several
    target wrappers (incl. arm64ec-*-uwp), and xmake's SDK autodetect
    otherwise picks the wrong one (which rejects FFmpeg's inline asm).

    The persisted project config directory is wiped first: xmake keeps the
    config under .xmake/<plat>/<arch>/ but reuses the PREVIOUS platform's
    directory when switching (-p mingw over an existing linux config wrote
    .xmake/linux/...), after which `xmake build` reads the (missing) mingw
    path, falls back to option defaults, and fails with 'closure missing'."""
    shutil.rmtree(ROOT / ".xmake", ignore_errors=True)
    run(["xmake", "f", "-p", "mingw", "--sdk=" + str(SDK), "-m", "release",
         "--cc=x86_64-w64-mingw32-gcc", "--cxx=x86_64-w64-mingw32-g++",
         "--ld=x86_64-w64-mingw32-gcc", "--ar=x86_64-w64-mingw32-ar",
         f"--av_manifest={av_manifest}",
         "--gc_sections=n", "--lto=" + ("y" if lto else "n"), "-y"])
    # the replayed closure must be the configured one, not an option default
    conf = next((ROOT / ".xmake").rglob("xmake.conf"), None)
    if conf is None or "mingw" not in str(conf) or \
            Path(json_value(conf, "av_manifest")).resolve() != Path(av_manifest).resolve():
        raise SystemExit(f"xmake config did not persist av_manifest={av_manifest} "
                         f"(looked at {conf})")
    shutil.rmtree(ROOT / "build" / "xmake", ignore_errors=True)
    shutil.rmtree(ROOT / "build" / "artifacts", ignore_errors=True)
    run(["xmake", "build", "qn_pcm_dump"])
    for a in ("libqianqian_av.a", "libsongcore.a", "qn_pcm_dump.exe"):
        if not (ROOT / "build/artifacts" / a).is_file():
            raise SystemExit(f"missing Windows artifact {a}")


def json_value(conf: Path, key: str) -> str | None:
    import re
    m = re.search(rf'{key}\s*=\s*"([^"]*)"', conf.read_text())
    return m.group(1) if m else None


def build_windows() -> None:
    """Replay the FULL Windows manifest through Xmake (audit input)."""
    _xmake_configure(FULL_MANIFEST, lto=False)


def audit_windows(manifest: Path, out_name: str) -> dict:
    """Offset-level COFF reachability + reduced-archive PE content proof."""
    outdir = STAGE_DIR / out_name
    run([sys.executable, "tools/link_audit_windows.py",
         "--archive", "build/artifacts/libqianqian_av.a",
         "--songcore", "build/artifacts/libsongcore.a",
         "--manifest", str(manifest.relative_to(ROOT)),
         "--out", str(outdir.relative_to(ROOT))], env=sdk_env())
    return json.loads((outdir / "report.json").read_text())


def compiled_ffmpeg_objects() -> set[str]:
    """Source paths of every object xmake actually compiled for qianqian_av.

    Object dirs mirror the source tree:
      build/xmake/.objs/qianqian_av/<plat>/<arch>/<mode>/<root>/<rel>.o
    where <root> is the FFmpeg source root or the stage's oracle (generated
    config sources). Returned as source paths with the trailing .o removed.
    """
    objs_root = ROOT / "build/xmake/.objs/qianqian_av"
    if not objs_root.is_dir():
        raise SystemExit("missing xmake object tree; build first")
    out = set()
    for o in objs_root.rglob("*.o"):
        # .objs/qianqian_av/<plat>/<arch>/<mode>/<project-relative source path>.o
        parts = o.relative_to(objs_root).parts
        if len(parts) < 5:
            raise SystemExit(f"unrecognized object path: {o}")
        out.add("/".join(parts[3:])[:-2])  # strip trailing .o
    return out


def verify_compiled_units(manifest_path: Path) -> int:
    """Hard gate: xmake compiled EXACTLY this manifest's units."""
    manifest = json.loads(manifest_path.read_text())
    expected = set()
    for unit in manifest["units"]:
        root = (manifest["config_root"] if unit["origin"] == "generated"
                else manifest["source_root"])
        expected.add(f"{root}/{unit['path']}")
    actual = compiled_ffmpeg_objects()
    missing = sorted(expected - actual)
    extra = sorted(actual - expected)
    if missing or extra:
        raise SystemExit(
            f"compiled-TU verification FAILED for {manifest_path.name}: "
            f"{len(missing)} expected-but-not-compiled {missing[:6]}, "
            f"{len(extra)} compiled-but-not-expected {extra[:6]}")
    print(f"compiled-TU verification: exactly {len(actual)} units "
          f"({manifest_path.name})")
    return len(actual)


def project_windows() -> dict:
    """Derive the projected manifest from the full-archive audit evidence."""
    reach = STAGE_DIR / "reachability" / "reachable-objects.json"
    run([sys.executable, "tools/minimize_manifest.py",
         "--stage", STAGE,
         "--audit", str(reach.relative_to(ROOT)),
         "--base-manifest", str(FULL_MANIFEST.relative_to(ROOT)),
         "--out", str(PROJECTED_MANIFEST.relative_to(ROOT))])
    return json.loads(PROJECTED_MANIFEST.read_text())


def build_projected_fixpoint(max_iterations: int = 4) -> dict:
    """Clean-rebuild ONLY the projected TUs, then re-audit to fixpoint.

    First-definer pull sets can shrink when dead members disappear from the
    archive, so project -> rebuild -> re-audit repeats until the projected
    closure equals the archive's pulled set. Each rebuild is verified per TU
    against the xmake object tree."""
    for iterations in range(1, max_iterations + 1):
        if iterations > 1:
            # shrink the projection using the latest projected-archive audit
            run([sys.executable, "tools/minimize_manifest.py",
                 "--stage", STAGE,
                 "--audit", str((STAGE_DIR / "reachability-projected" /
                                 "reachable-objects.json").relative_to(ROOT)),
                 "--base-manifest", str(FULL_MANIFEST.relative_to(ROOT)),
                 "--out", str(PROJECTED_MANIFEST.relative_to(ROOT))])
        manifest = json.loads(PROJECTED_MANIFEST.read_text())
        projected_units = len(manifest["units"])
        print(f"projected rebuild iteration {iterations}: {projected_units} TUs")
        _xmake_configure(PROJECTED_MANIFEST, lto=False)
        verify_compiled_units(PROJECTED_MANIFEST)
        audit_windows(PROJECTED_MANIFEST, "reachability-projected")
        pulled = json.loads((STAGE_DIR / "reachability-projected" /
                             "report.json").read_text())["pulled_manifest_units"]
        print(f"projected-archive audit: {pulled}/{projected_units} units pulled")
        if pulled == projected_units:
            evidence = {
                "iterations": iterations,
                "projected_units": projected_units,
                "pulled_units": pulled,
                "fixpoint": True,
            }
            (STAGE_DIR / "projection.json").write_text(
                json.dumps(evidence, indent=2, sort_keys=True) + "\n")
            return evidence
    raise SystemExit("projection did not reach fixpoint "
                     f"after {max_iterations} iterations")


def _dll_export_import_gates(dll: Path, out_dir: Path) -> list[str]:
    objdump = str(SDK / "bin" / "x86_64-w64-mingw32-objdump")
    pe = run([objdump, "-p", str(dll)]).stdout
    exports = []
    in_export = False
    for line in pe.splitlines():
        if line.startswith("Export Table:"):
            in_export = True
            continue
        if in_export:
            # rows look like: '       1   0x1eca  song_open'
            m = re.match(r"^\s+\d+\s+0x[0-9a-f]+\s+(\S+)$", line)
            if m:
                exports.append(m.group(1))
            elif exports:
                break
    missing = [s for s in CONTRACT if s not in exports]
    unexpected = sorted(set(exports) - set(CONTRACT))
    if missing or unexpected:
        raise SystemExit(f"DLL export gate FAILED: missing={missing} unexpected={unexpected}")
    imports = sorted(set(re.findall(r"^\s+DLL Name: (\S+)$", pe, re.M)))
    bad = [d for d in imports if FORBIDDEN_DLL.search(d)]
    if bad:
        raise SystemExit(f"DLL import gate FAILED: FFmpeg runtime dependency {bad}")
    (out_dir / "pe-info.txt").write_text(pe)
    return imports


def build_dll(*, lto: bool) -> dict:
    """qianqian_songcore.dll + import library, built from the PROJECTED closure.

    Hard requirement: the flags-projected manifest for the DLL derives from
    the projected manifest (never from the full closure); the rebuild is
    verified per TU before the DLL is linked."""
    artifacts = ROOT / "build/artifacts"
    cc = str(SDK / "bin" / "x86_64-w64-mingw32-clang")
    suffix = "-lto" if lto else ""
    dll_stage = f"{STAGE}-dll{suffix}"
    out_dir = STAGE_DIR / f"dll{suffix}"
    out_dir.mkdir(exist_ok=True)

    # size-oriented flag projection of the PROJECTED closure only
    flags = ["--replace-opt", "Os",
             "--add-flag=-ffunction-sections", "--add-flag=-fdata-sections"]
    if lto:
        flags += ["--add-flag=-flto"]
    dll_manifest = ROOT / f"build/minimize/{dll_stage}/manifest-projected.json"
    run([sys.executable, "tools/minimize_flags.py", "--stage", dll_stage,
         "--from-manifest", str(PROJECTED_MANIFEST.relative_to(ROOT)), *flags])
    _xmake_configure(dll_manifest, lto=lto)
    projected_tu = verify_compiled_units(dll_manifest)

    ff_includes = ["-I", str(ROOT / "build/minimize/win-c6/oracle"),
                   "-I", str(ROOT / "build/ffmpeg-src")]
    opt = ["-Os"] + (["-flto"] if lto else [])
    songcore_o = out_dir / "songcore.o"
    run([cc, *opt, "-ffunction-sections", "-fdata-sections", "-I", "include",
         *ff_includes, "-c", "src/songcore_ffmpeg.c", "-o", str(songcore_o)],
        env=sdk_env())
    dll = out_dir / "qianqian_songcore.dll"
    implib = out_dir / "qianqian_songcore.lib"
    av_archive = artifacts / "libqianqian_av.a"
    run([cc, "-shared", *opt, "-o", str(dll), str(songcore_o), str(av_archive),
         "tools/songcore_q.def", f"-Wl,--out-implib={implib}",
         "-Wl,--gc-sections", "-lbcrypt"], env=sdk_env())

    imports = _dll_export_import_gates(dll, out_dir)

    # --- identity + sizes
    def xz_bytes(path: Path) -> int:
        return len(subprocess.run(["xz", "-c", str(path)], capture_output=True,
                                  check=True).stdout)

    raw = dll.stat().st_size
    import tempfile
    with tempfile.TemporaryDirectory() as td:
        stripped_copy = Path(td) / dll.name
        shutil.copy2(dll, stripped_copy)
        run([str(SDK / "bin" / "x86_64-w64-mingw32-strip"), str(stripped_copy)])
        stripped = stripped_copy.stat().st_size
        stripped_xz = xz_bytes(stripped_copy)
    members_out = subprocess.run(
        [str(SDK / "bin" / "x86_64-w64-mingw32-llvm-ar"), "t", str(av_archive)],
        capture_output=True, text=True, check=True)
    result = {
        "schema": 2,
        "stage": dll_stage,
        "lto": lto,
        "dll": str(dll.relative_to(ROOT)),
        "dll_sha256": sha256_file(dll),
        "closure": {
            "projected_manifest": str(PROJECTED_MANIFEST.relative_to(ROOT)),
            "projected_manifest_sha256": sha256_file(PROJECTED_MANIFEST),
            "projected_units": projected_tu,
            "archive_members": len(members_out.stdout.splitlines()),
        },
        "exported_api_symbols": sorted(CONTRACT),
        "exported_api_count": len(CONTRACT),
        "import_table": imports,
        "sizes": {
            "dll_bytes": raw,
            "dll_xz_bytes": xz_bytes(dll),
            "dll_stripped_bytes": stripped,
            "dll_stripped_xz_bytes": stripped_xz,
            "implib_bytes": implib.stat().st_size,
            "libqianqian_av_a_bytes": av_archive.stat().st_size,
        },
    }
    (out_dir / "dll.json").write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
    print(json.dumps({"stage": dll_stage, "projected_units": projected_tu,
                      **result["sizes"]}, indent=2, sort_keys=True))
    return result


def correct_windows(dll_info: dict, tag: str = "") -> dict:
    """Windows-native correctness through the shipping DLL: FULL applicable
    corpus (clean + degraded) with typed gates, plus wide-path / >2GiB gates.

    Clean cases run the full contract and are gated per seek tier
    (STRICT/LAPPED/UNSUPPORTED). Degraded (truncated/malformed) cases run a
    bounded `--robust` classification: the host prints a typed outcome and
    exits 0; any crash exit code, hang (timeout), absurd frame count, or
    nondeterministic classification is a gate failure."""
    cc = str(SDK / "bin" / "x86_64-w64-mingw32-clang")
    out_dir = STAGE_DIR / "correctness"
    out_dir.mkdir(exist_ok=True)
    dll_dir = (ROOT / dll_info["dll"]).parent
    host = out_dir / "songcore_windows_host.exe"
    run([cc, "-O2", "-municode", "-I", "include", "-o", str(host),
         "tools/songcore_windows_host.c",
         f"-L{dll_dir}", "-lqianqian_songcore"], env=sdk_env())
    # run from the DLL directory so LoadLibrary resolves it
    cases = load_cases(STAGE_CAPABILITIES["c5"])

    def host_run(argslist: list[str], *, timeout: int = 120):
        try:
            p = subprocess.run([str(host), *argslist], cwd=str(dll_dir),
                               capture_output=True, text=True, timeout=timeout)
            return p, None
        except subprocess.TimeoutExpired:
            return None, "timeout"

    # fixtures must be reachable from the DLL dir: copy them next to it
    fix_dir = dll_dir / "fixtures"
    fix_dir.mkdir(exist_ok=True)
    results = {}
    skipped = []
    for case in cases:
        shutil.copy2(fixture_path(case), fix_dir / case["file"])
        if case["degraded"] or case["expect"].get("probe_may_fail"):
            # --- degraded: typed bounded classification, executed natively
            runs = []
            problems = []
            for attempt in (1, 2):
                p, err = host_run(["--robust", "fixtures/" + case["file"]])
                if err == "timeout":
                    problems.append(f"hang/timeout (attempt {attempt})")
                    continue
                if p.returncode != 0:
                    problems.append(f"crash/unexpected exit {p.returncode} (attempt {attempt})")
                    continue
                try:
                    obs = last_json(p.stdout)
                except Exception:
                    problems.append(f"bad output (attempt {attempt}): {p.stdout[-120:]}")
                    continue
                runs.append(obs)
            entry = {"mode": "robust", "runs": runs}
            if not problems and len(runs) == 2:
                a, b = runs
                entry["classification"] = a.get("classification")
                entry["frames"] = a.get("frames")
                entry["deterministic"] = (
                    a.get("classification") == b.get("classification")
                    and a.get("frames") == b.get("frames"))
                cls = entry["classification"]
                if cls not in TYPED_OUTCOMES:
                    problems.append(f"untyped outcome {cls!r}")
                elif not entry["deterministic"]:
                    problems.append("nondeterministic classification")
                elif case["expect"].get("probe_may_fail"):
                    if cls not in {"OPEN_FAILED", "PROBE_FAILED", "DECODE_ERROR",
                                   "DEGRADED_EOF"}:
                        problems.append(f"probe_may_fail case produced {cls}")
                else:
                    # truncated: bounded decode floor from the corpus manifest
                    if cls not in {"DECODE_ERROR", "DEGRADED_EOF"}:
                        problems.append(f"truncated case produced {cls}")
                    floor = case["expect"].get("min_samples")
                    if floor and (entry["frames"] or 0) < floor:
                        problems.append(f"frames {entry['frames']} below floor {floor}")
            entry["problems"] = problems
            results[case["id"]] = entry
            continue

        # --- clean case: full contract with tier-gated seeks
        p, err = host_run(["fixtures/" + case["file"]])
        if err == "timeout":
            results[case["id"]] = {"status": "timeout"}
            continue
        try:
            obs = last_json(p.stdout)
        except Exception:
            results[case["id"]] = {"status": "bad_output", "stdout_tail": p.stdout[-200:]}
            continue
        seeks = obs.get("seeks", [])
        cap = case["capability"]
        tier = "UNSUPPORTED" if case["file"].endswith(".aac") else SEEK_TIER[cap]
        entry = {
            "status": "ok" if p.returncode == 0 else "failed",
            "contract": tier,
            "sequential_ok": obs.get("sequential_ok"),
            "sequential_sha256": obs.get("sequential_sha256"),
            "seeks_clean_eof": all(s.get("clean_eof") for s in seeks),
            "seeks_suffix_exact": [bool(s.get("suffix_exact")) for s in seeks],
        }
        results[case["id"]] = entry
        problems = []
        if entry["status"] != "ok" or not entry["sequential_ok"]:
            problems.append("decode failed")
        for s in seeks:
            if tier == "UNSUPPORTED":
                if s.get("status") == "seek_failed":
                    continue  # the typed failure this tier accepts
                if s.get("status") != "done" or not s.get("clean_eof") or not s.get("frames"):
                    problems.append(f"seek @{s.get('target_us')} neither typed-failed "
                                    f"nor bounded clean-EOF decode")
            else:
                if s.get("status") != "done":
                    problems.append(f"seek @{s.get('target_us')} did not succeed ({tier})")
                elif not s.get("clean_eof"):
                    problems.append(f"seek @{s.get('target_us')} without clean EOF")
                elif not s.get("frames"):
                    problems.append(f"seek @{s.get('target_us')} produced no PCM")
                elif tier == "STRICT" and not s.get("suffix_exact"):
                    problems.append(f"seek @{s.get('target_us')} strict suffix mismatch")
        # lossless cross-platform byte equality
        if cap in ("alac", "wav", "flac"):
            want = case["expect"]["pcm"]["canonical_f32_sha256"]
            if entry["sequential_sha256"] != want:
                problems.append("lossless PCM differs from canonical sha")
        if problems:
            entry["problems"] = problems

    clean_results = {k: v for k, v in results.items() if v.get("mode") != "robust"}
    degraded_results = {k: v for k, v in results.items() if v.get("mode") == "robust"}
    failures = {k: v for k, v in results.items() if v.get("problems") or v.get("status") in ("timeout", "bad_output")}
    clean_n = len(clean_results)
    degraded_n = len(degraded_results)

    # unicode path gate (destination path is a wide literal inside the host)
    aac = next(c for c in cases if c["id"] == "aac-lc-44-stereo")
    shutil.copy2(fixture_path(aac), fix_dir / "unicode-src.m4a")
    p, err = host_run(["--unicode", "fixtures/unicode-src.m4a"])
    if err == "timeout":
        unicode_ok, unicode_obs = False, {"error": "timeout"}
    else:
        try:
            unicode_ok = p.returncode == 0
            unicode_obs = last_json(p.stdout)
        except Exception:
            unicode_ok, unicode_obs = False, {"stdout_tail": p.stdout[-200:]}

    # >2 GiB seek gate (synthetic virtual IO)
    p, err = host_run(["--largefile"])
    if err == "timeout":
        large_ok, large_obs = False, {"error": "timeout"}
    else:
        try:
            large_ok = p.returncode == 0
            large_obs = last_json(p.stdout)
        except Exception:
            large_ok, large_obs = False, {"stdout_tail": p.stdout[-200:]}

    report = {
        "schema": 2,
        "host": str(host.relative_to(ROOT)) if str(host).startswith(str(ROOT)) else str(host),
        "host_link": "qianqian_songcore.lib (shipping DLL import library)",
        "total_applicable": len(cases),
        "clean_cases": clean_n,
        "degraded_cases": degraded_n,
        "executed": clean_n + degraded_n,
        "skipped": skipped,
        "seek_tiers": {
            tier: sum(1 for v in clean_results.values() if v.get("contract") == tier)
            for tier in ("STRICT", "LAPPED", "UNSUPPORTED")},
        "per_case": results,
        "failures": failures,
        "unicode_path_gate": {"pass": unicode_ok, "observed": unicode_obs},
        "largefile_gate": {"pass": large_ok, "observed": large_obs},
    }
    verdict = "PASS" if (not failures and unicode_ok and large_ok and not skipped
                         and clean_n + degraded_n == len(cases)) else "FAIL"
    report["verdict"] = verdict
    (out_dir / f"correctness{tag}.json").write_text(
        json.dumps(report, indent=2, sort_keys=True) + "\n")
    print(json.dumps({k: report[k] for k in
                      ("verdict", "total_applicable", "clean_cases", "degraded_cases",
                       "seek_tiers", "unicode_path_gate", "largefile_gate")},
                     indent=2, sort_keys=True))
    if verdict != "PASS":
        raise SystemExit(f"Windows correctness FAILED: {list(failures)[:5]}")
    return report


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--import", dest="do_import", action="store_true")
    ap.add_argument("--build", dest="do_build", action="store_true")
    ap.add_argument("--audit", dest="do_audit", action="store_true")
    ap.add_argument("--project", dest="do_project", action="store_true")
    ap.add_argument("--projected", dest="do_projected", action="store_true",
                    help="clean rebuild of the projected closure + fixpoint re-audit")
    ap.add_argument("--dll", dest="do_dll", action="store_true",
                    help="both shipping variants (-Os, -Os+LTO) from the projected closure")
    ap.add_argument("--correct", dest="do_correct", action="store_true",
                    help="native correctness for both DLL variants (full applicable corpus)")
    ap.add_argument("--all", action="store_true")
    args = ap.parse_args()
    todo = args.all or not any((args.do_import, args.do_build, args.do_audit,
                                args.do_project, args.do_projected, args.do_dll,
                                args.do_correct))
    dll_variants: list[dict] = []
    if args.do_import or todo:
        import_windows()
    if args.do_build or todo:
        build_windows()
    if args.do_audit or todo:
        audit_windows(FULL_MANIFEST, "reachability")
    if args.do_project or todo:
        project_windows()
    if args.do_projected or todo:
        build_projected_fixpoint()
    if args.do_dll or todo:
        dll_variants.append(build_dll(lto=False))
        dll_variants.append(build_dll(lto=True))
    if args.do_correct or todo:
        if not dll_variants:
            for name in ("dll", "dll-lto"):
                p = STAGE_DIR / name / "dll.json"
                if p.is_file():
                    dll_variants.append(json.loads(p.read_text()))
        for info, tag in zip(dll_variants, ("", "-lto")):
            correct_windows(info, tag)


if __name__ == "__main__":
    main()
