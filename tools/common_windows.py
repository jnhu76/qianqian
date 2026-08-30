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

Stages:
  --import     Windows configure/Make oracle -> target-specific manifest
  --build      Xmake mingw replay -> libqianqian_av.a / qn_pcm_dump.exe
  --audit      COFF reachability simulation + lld -Map member-multiset gate
  --dll        qianqian_songcore.dll (+ import lib) with export/import-table
               hard gates (only the five song_* APIs; no FFmpeg DLL imports)
  --correct    Windows-native correctness: consumer .exe + qn_pcm_dump.exe +
               wide-path / >2GiB host gates across the full applicable corpus
  --all        everything above in order

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
    STAGE_CAPABILITIES, bench_json, fixture_path, load_cases, sha256_file,
    verify_fixtures,
)
import common_import  # noqa: E402

SDK = Path(os.environ.get("LLVM_MINGW_SDK", Path.home() / "toolchains/llvm-mingw"))
STAGE = "win-c6"
STAGE_DIR = ROOT / "build" / "minimize" / STAGE
CONTRACT = ["song_open", "song_probe", "song_read_pcm", "song_seek", "song_close"]
FORBIDDEN_DLL = re.compile(r"(avcodec|avformat|avutil|swresample|swscale|avfilter)", re.I)


def run(cmd: list[str], *, check=True, binary=False, env=None) -> subprocess.CompletedProcess:
    print("+", " ".join(map(str, cmd)), flush=True)
    e = dict(os.environ)
    if env:
        e.update(env)
    p = subprocess.run(list(map(str, cmd)), cwd=ROOT, capture_output=True,
                       text=not binary, env=e)
    if check and p.returncode:
        err = p.stderr if not binary else p.stderr.decode("utf-8", "replace")
        raise SystemExit(f"command failed ({p.returncode}): {' '.join(map(str, cmd))}\n{err[-4000:]}")
    return p


def sdk_env() -> dict:
    if not (SDK / "bin").is_dir():
        raise SystemExit(f"llvm-mingw SDK not found at {SDK}; set LLVM_MINGW_SDK")
    return {"PATH": f"{SDK / 'bin'}:{os.environ['PATH']}"}


def last_json(stdout: str):
    lines = [l for l in stdout.splitlines() if l.strip()]
    return json.loads(lines[-1])


def import_windows() -> None:
    """Windows configure/Make oracle -> target-specific compile manifest."""
    run([sys.executable, "tools/common_import.py", "--stage", STAGE,
         "--profile", "bench/profiles/c5-opus.json", "--force",
         "--configure-extra", "--enable-cross-compile",
         "--configure-extra", "--cross-prefix=x86_64-w64-mingw32-",
         "--configure-extra", "--target-os=mingw32",
         "--configure-extra", "--arch=x86_64"],
        env=sdk_env())
    manifest = json.loads((STAGE_DIR / "manifest.json").read_text())
    print(f"windows closure: {manifest['closure']['translation_units']} TUs; "
          f"toolchain: {manifest['toolchain']}")
    if manifest["toolchain"]["system"] != "linux":
        raise SystemExit("oracle toolchain identity unexpected")
    if "mingw32" not in (manifest["toolchain"].get("target_os") or ""):
        raise SystemExit(f"oracle target_os is not mingw32: {manifest['toolchain']}")


def build_windows() -> None:
    """Replay the Windows manifest through Xmake's mingw platform."""
    run(["xmake", "f", "-p", "mingw", "--sdk", str(SDK), "-m", "release",
         f"--av_manifest=build/minimize/{STAGE}/manifest.json",
         "--gc_sections=n", "--lto=n", "-y"])
    shutil.rmtree(ROOT / "build" / "xmake", ignore_errors=True)
    shutil.rmtree(ROOT / "build" / "artifacts", ignore_errors=True)
    run(["xmake", "build", "qn_pcm_dump"])
    for a in ("libqianqian_av.a", "libsongcore.a", "qn_pcm_dump.exe"):
        if not (ROOT / "build/artifacts" / a).is_file():
            raise SystemExit(f"missing Windows artifact {a}")


def parse_map_members(map_text: str, archive_stem: str) -> list[str]:
    """lld COFF -Map: archive members appear as '<stem>-<member>.o:(.section)'.
    Returns the member basename multiset that contributed to the image."""
    members = []
    for m in re.finditer(rf"^\S*\s+\S+\s+{re.escape(archive_stem)}-(\S+?)\.o:\(", map_text, re.M):
        members.append(m.group(1) + ".o")
    return sorted(members)


def audit_windows() -> None:
    """COFF reachability: simulate GNU/llvm ld first-definer pull over the
    archive, then hard-gate against the real lld -Map member multiset."""
    import link_audit as la

    archive = ROOT / "build/artifacts/libqianqian_av.a"
    songcore = ROOT / "build/artifacts/libsongcore.a"
    members = la.parse_archive(archive)
    with _tmpdir() as tmp:
        for m in members:
            d, u, w = la.nm_symbols(
                archive.read_bytes()[m["offset"]:m["offset"] + m["size"]], tmp, m["index"])
            m["defined"], m["undef"] = d, u
        sc_members = la.parse_archive(songcore)
        for m in sc_members:
            d, u, w = la.nm_symbols(
                songcore.read_bytes()[m["offset"]:m["offset"] + m["size"]], tmp, 10000 + m["index"])
            m["defined"], m["undef"] = d, u
        undefined, defined = set(), set()
        for m in sc_members:
            undefined |= m["undef"] - m["defined"]
            defined |= m["defined"]
        undefined -= defined
        pulled, changed = {}, True
        while changed:
            changed = False
            for m in members:
                if m["index"] in pulled:
                    continue
                if m["defined"] & undefined:
                    pulled[m["index"]] = m
                    undefined -= m["defined"]
                    undefined |= m["undef"] - m["defined"]
                    changed = True
        sim = sorted(m["member"] for m in pulled.values())

        # real lld evidence
        host = STAGE_DIR / "win-audit-main.c"
        host.write_text(
            'extern void *song_open(void *);extern void song_probe(void *);'
            'extern void song_read_pcm(void *);extern void song_seek(void *);'
            'extern void song_close(void *);\n'
            'int main(void){void *h=song_open(0);song_probe(h);song_read_pcm(h,0,0);'
            'song_seek(h,0);song_close(h);return 0;}\n')
        exe = STAGE_DIR / "win-audit.exe"
        mapf = STAGE_DIR / "linker.map"
        cc = str(SDK / "bin" / "x86_64-w64-mingw32-clang")
        run([cc, "-O2", "-I", "include", "-o", str(exe), str(host),
             str(songcore), str(archive), "-lm", "-Wl,-Map=" + str(mapf),
             "-Wl,--gc-sections"], env=sdk_env())
        real = parse_map_members(mapf.read_text(), archive.stem.replace("lib", "", 1))
        # map lists member basenames; compare as multisets against simulation
        from collections import Counter
        sim_ms = Counter(m["member"].rsplit("/", 1)[-1] for m in pulled.values())
        real_ms = Counter(real)
        if sim_ms != real_ms:
            only_real = sorted((real_ms - sim_ms).elements())
            only_sim = sorted((sim_ms - real_ms).elements())
            raise SystemExit(f"Windows reachability gate FAILED: lld map vs simulation "
                             f"(real-only={only_real[:8]}, sim-only={only_sim[:8]})")
        (STAGE_DIR / "win-reachability.json").write_text(json.dumps({
            "archive_members": len(members),
            "pulled_members": len(pulled),
            "lld_map_member_multiset_equal": True,
            "pulled": sim,
        }, indent=1) + "\n")
        print(f"windows reachability: {len(pulled)}/{len(members)} members pulled; "
              f"lld map multiset == simulation")


class _tmpdir:
    def __enter__(self):
        import tempfile
        self.d = tempfile.mkdtemp()
        return Path(self.d)

    def __exit__(self, *a):
        shutil.rmtree(self.d, ignore_errors=True)


def build_dll(*, lto: bool) -> dict:
    """qianqian_songcore.dll + import library with hard ABI/dependency gates.

    The DLL is the size-oriented shipping form: the Windows closure at
    -Os + function/data sections (+ LTO for the -lto variant), replayed
    through Xmake, then linked with the .def file so the export table is
    exactly the SongCore ABI."""
    artifacts = ROOT / "build/artifacts"
    cc = str(SDK / "bin" / "x86_64-w64-mingw32-clang")
    suffix = "-lto" if lto else ""
    dll_stage = f"{STAGE}-dll{suffix}"
    out_dir = STAGE_DIR / f"dll{suffix}"
    out_dir.mkdir(exist_ok=True)

    # size-oriented projection of the Windows closure
    flags = ["--replace-opt", "Os",
             "--add-flag=-ffunction-sections", "--add-flag=-fdata-sections"]
    if lto:
        flags += ["--add-flag=-flto"]
    run([sys.executable, "tools/minimize_flags.py", "--stage", dll_stage,
         "--from-stage", STAGE, *flags])
    run(["xmake", "f", "-p", "mingw", "--sdk", str(SDK), "-m", "release",
         f"--av_manifest=build/minimize/{dll_stage}/manifest-projected.json",
         "--gc_sections=n", "--lto=" + ("y" if lto else "n"), "-y"])
    shutil.rmtree(ROOT / "build" / "xmake", ignore_errors=True)
    shutil.rmtree(ROOT / "build" / "artifacts", ignore_errors=True)
    run(["xmake", "build", "qn_pcm_dump"])
    manifest = json.loads((ROOT / f"build/minimize/{dll_stage}/manifest-projected.json").read_text())
    av_archive = artifacts / "libqianqian_av.a"
    ff_includes = ["-I", str(ROOT / manifest["config_root"]),
                   "-I", str(ROOT / manifest["source_root"])]
    opt = ["-Os"] + (["-flto"] if lto else [])
    songcore_o = out_dir / "songcore.o"
    run([cc, *opt, "-ffunction-sections", "-fdata-sections", "-I", "include",
         *ff_includes, "-c", "src/songcore_ffmpeg.c", "-o", str(songcore_o)],
        env=sdk_env())
    dll = out_dir / "qianqian_songcore.dll"
    implib = out_dir / "qianqian_songcore.lib"
    run([cc, "-shared", *opt, "-o", str(dll), str(songcore_o), str(av_archive),
         "tools/songcore_q.def", f"-Wl,--out-implib={implib}",
         "-Wl,--gc-sections"], env=sdk_env())

    # --- export gate
    objdump = str(SDK / "bin" / "x86_64-w64-mingw32-objdump")
    pe = run([objdump, "-p", str(dll)]).stdout
    exports = []
    in_export = False
    for line in pe.splitlines():
        if "[Ordinal/Name Pointer] Table" in line:
            in_export = True
            continue
        if in_export:
            if line.startswith("\t\t"):
                exports.append(line.strip())
            else:
                in_export = False
    missing = [s for s in CONTRACT if s not in exports]
    unexpected = sorted(set(exports) - set(CONTRACT))
    if missing or unexpected:
        raise SystemExit(f"DLL export gate FAILED: missing={missing} unexpected={unexpected}")

    # --- import gate: no FFmpeg DLLs, record everything
    imports = sorted(set(re.findall(r"^\s+DLL Name: (\S+)$", pe, re.M)))
    bad = [d for d in imports if FORBIDDEN_DLL.search(d)]
    if bad:
        raise SystemExit(f"DLL import gate FAILED: FFmpeg runtime dependency {bad}")
    (out_dir / "pe-info.txt").write_text(pe)

    # --- identity + sizes
    def xz_bytes(path: Path) -> int:
        return len(subprocess.run(["xz", "-c", str(path)], capture_output=True,
                                  check=True).stdout)

    import tempfile
    raw = dll.stat().st_size
    with tempfile.TemporaryDirectory() as td:
        stripped_copy = Path(td) / dll.name
        shutil.copy2(dll, stripped_copy)
        run([str(SDK / "bin" / "x86_64-w64-mingw32-strip"), str(stripped_copy)])
        stripped = stripped_copy.stat().st_size
        stripped_xz = xz_bytes(stripped_copy)
    result = {
        "schema": 1,
        "stage": f"{STAGE}-dll{suffix}",
        "lto": lto,
        "dll": str(dll.relative_to(ROOT)),
        "dll_sha256": sha256_file(dll),
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
    print(json.dumps(result["sizes"], indent=2, sort_keys=True))
    return result


def correct_windows(dll_info: dict) -> dict:
    """Windows-native correctness through the shipping DLL.

    The host adapter (tools/songcore_windows_host.c) links ONLY the import
    library of qianqian_songcore.dll and drives the full contract natively:
    open/probe/head/decode-to-EOF/seek 25/50/75 via CreateFileW-based host
    IO. Lossless PCM must byte-match the canonical manifest shas; lossy PCM
    is recorded and, where it differs from Linux, tolerance-metriced in a
    dedicated step. Unicode-path and >2GiB-seek host gates close it out.
    """
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

    def host_run(argslist: list[str]):
        return subprocess.run([str(host), *argslist], cwd=str(dll_dir),
                              capture_output=True, text=True)

    # fixtures must be reachable from the DLL dir: copy them next to it
    fix_dir = dll_dir / "fixtures"
    fix_dir.mkdir(exist_ok=True)
    results = {}
    strict_by_cap = {"wav": True, "alac": True, "vorbis": True,
                     "aac": False, "opus": False, "mp3": False, "flac": True}
    for case in cases:
        if case["expect"].get("probe_may_fail") or case["degraded"]:
            continue
        shutil.copy2(fixture_path(case), fix_dir / case["file"])
        p = host_run(["fixtures/" + case["file"]])
        try:
            obs = last_json(p.stdout)
        except Exception:
            results[case["id"]] = {"status": "bad_output", "stdout_tail": p.stdout[-200:]}
            continue
        entry = {
            "status": "ok" if p.returncode == 0 else "failed",
            "sequential_ok": obs.get("sequential_ok"),
            "sequential_sha256": obs.get("sequential_sha256"),
            "seeks_clean_eof": all(s.get("clean_eof") for s in obs.get("seeks", [])),
            "seeks_suffix_exact": [bool(s.get("suffix_exact")) for s in obs.get("seeks", [])],
        }
        results[case["id"]] = entry
        cap = case["capability"]
        problems = []
        if entry["status"] != "ok" or not entry["sequential_ok"]:
            problems.append("decode failed")
        if not entry["seeks_clean_eof"]:
            problems.append("seek without clean EOF")
        if strict_by_cap[cap] and not all(entry["seeks_suffix_exact"]):
            problems.append("strict family suffix mismatch on Windows")
        # lossless cross-platform byte equality
        if cap in ("alac", "wav", "flac"):
            want = case["expect"]["pcm"]["canonical_f32_sha256"]
            if entry["sequential_sha256"] != want:
                problems.append("lossless PCM differs from canonical sha")
        if problems:
            results[case["id"]]["problems"] = problems
    failures = {k: v for k, v in results.items() if v.get("problems")}

    # unicode path gate (destination path is a wide literal inside the host)
    aac = next(c for c in cases if c["id"] == "aac-lc-44-stereo")
    shutil.copy2(fixture_path(aac), fix_dir / "unicode-src.m4a")
    p = host_run(["--unicode", "fixtures/unicode-src.m4a"])
    try:
        unicode_ok = p.returncode == 0
        unicode_obs = last_json(p.stdout)
    except Exception:
        unicode_ok, unicode_obs = False, {"stdout_tail": p.stdout[-200:]}

    # >2 GiB seek gate (synthetic virtual IO)
    p = host_run(["--largefile"])
    try:
        large_ok = p.returncode == 0
        large_obs = last_json(p.stdout)
    except Exception:
        large_ok, large_obs = False, {"stdout_tail": p.stdout[-200:]}

    report = {
        "schema": 1,
        "host": str(host.relative_to(ROOT)) if str(host).startswith(str(ROOT)) else str(host),
        "host_link": "qianqian_songcore.lib (shipping DLL import library)",
        "cases": len(results),
        "failures": failures,
        "unicode_path_gate": {"pass": unicode_ok, "observed": unicode_obs},
        "largefile_gate": {"pass": large_ok, "observed": large_obs},
    }
    verdict = "PASS" if (not failures and unicode_ok and large_ok) else "FAIL"
    report["verdict"] = verdict
    (out_dir / "correctness.json").write_text(
        json.dumps(report, indent=2, sort_keys=True) + "\n")
    print(json.dumps({k: report[k] for k in
                      ("verdict", "cases", "unicode_path_gate", "largefile_gate")},
                     indent=2, sort_keys=True))
    if verdict != "PASS":
        raise SystemExit(f"Windows correctness FAILED: {list(failures)[:5]}")
    return report


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--import", dest="do_import", action="store_true")
    ap.add_argument("--build", dest="do_build", action="store_true")
    ap.add_argument("--audit", dest="do_audit", action="store_true")
    ap.add_argument("--dll", dest="do_dll", action="store_true")
    ap.add_argument("--correct", dest="do_correct", action="store_true")
    ap.add_argument("--all", action="store_true")
    args = ap.parse_args()
    todo = args.all or not any((args.do_import, args.do_build, args.do_audit,
                                args.do_dll, args.do_correct))
    dll_info = None
    if args.do_import or todo:
        import_windows()
    if args.do_build or todo:
        build_windows()
    if args.do_audit or todo:
        audit_windows()
    if args.do_dll or todo:
        dll_info = build_dll(lto=False)
    if args.do_correct or todo:
        if dll_info is None:
            dll_info = json.loads(
                (STAGE_DIR / "dll" / "dll.json").read_text())
        correct_windows(dll_info)


if __name__ == "__main__":
    main()
