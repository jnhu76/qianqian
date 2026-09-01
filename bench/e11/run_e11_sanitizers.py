#!/usr/bin/env python3
"""E11 sanitizer density pass (test-only, never ships).

Instruments exactly the code under test — SongCore (src/songcore_ffmpeg.c)
plus the machine-test harness (bench/e11/qn_e11_record.c) — with
AddressSanitizer, UndefinedBehaviorSanitizer and LeakSanitizer, links against
the frozen uninstrumented FFmpeg closure (build/artifacts/libqianqian_av.a)
and runs the full fixture corpus through the dense consumption modes:

  record  <file>           full decode to EOF + snapshot + seek + select
  states  <file>           deterministic fuzz-like call sequences
  neg     <file>           typed open/probe/read error paths
  iofail  <file> <n>       host I/O fault injection

Gates (fail-closed):
  - zero AddressSanitizer reports
  - zero LeakSanitizer reports at process exit
  - zero UndefinedBehavior reports attributed to Qianqian code
    (src/, bench/e11/, include/); reports inside the pinned upstream
    FFmpeg closure are counted as upstream noise, not our defect
  - every invocation exits cleanly (no ASan aborts / crashes)

Writes bench/results/songcore-v1/sanitizers.json; --check validates an
existing tree read-only.
"""

import argparse
import glob
import json
import os
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
ARTIFACTS = os.path.join(ROOT, "build", "artifacts")
CLOSURE_LIB = os.path.join(ARTIFACTS, "libqianqian_av.a")
SOURCE_ROOT = os.path.join(ROOT, "build", "ffmpeg-src")
ORACLE_ROOT = os.path.join(ROOT, "build", "minimize", "e11-test", "oracle")
OUT_DIR = os.path.join(ROOT, "bench", "results", "songcore-v1")
SAN_BUILD_DIR = os.path.join(ROOT, "build", "sanitize")
SAN_BINARY = os.path.join(SAN_BUILD_DIR, "qn_e11_record_san")
FIXTURES = os.path.join(ROOT, "corpus", "fixtures")
E11_MANIFEST = os.path.join(ROOT, "corpus", "manifest", "e11.json")
COMMON_MANIFEST = os.path.join(ROOT, "corpus", "manifest", "common-formats.json")

# Files we own; UBSan reports pointing here are defects.
OWNED_PREFIXES = (
    os.path.join(ROOT, "src", "songcore_ffmpeg.c"),
    os.path.join(ROOT, "bench", "e11", "qn_e11_record.c"),
    os.path.join(ROOT, "include", "songcore.h"),
)
UPSTREAM_PREFIX = os.path.join(ROOT, "build", "ffmpeg-src")

ASAN_OPTIONS = "detect_leaks=1:halt_on_error=0:abort_on_error=0"
UBSAN_OPTIONS = "print_stacktrace=1:halt_on_error=0"


def run(cmd, env=None, timeout=300):
    e = dict(os.environ)
    if env:
        e.update(env)
    return subprocess.run(cmd, capture_output=True, text=True, env=e,
                          timeout=timeout)


def build_binary():
    os.makedirs(SAN_BUILD_DIR, exist_ok=True)
    if not os.path.isfile(CLOSURE_LIB):
        raise SystemExit(f"closure library missing: {CLOSURE_LIB}")
    src = [
        os.path.join(ROOT, "src", "songcore_ffmpeg.c"),
        os.path.join(ROOT, "bench", "e11", "qn_e11_record.c"),
    ]
    r = run(["gcc", "-O1", "-g", "-fno-omit-frame-pointer",
             "-fsanitize=address,undefined",
             "-I" + os.path.join(ROOT, "include"),
             "-I" + SOURCE_ROOT, "-I" + ORACLE_ROOT,
             *src, CLOSURE_LIB, "-lm", "-lpthread",
             "-o", SAN_BINARY])
    if r.returncode != 0:
        print(r.stderr[-3000:])
        raise SystemExit("sanitizer build failed")
    print(f"sanitizer binary: {SAN_BINARY}")


def list_fixtures():
    ids = {}
    for manifest in (E11_MANIFEST, COMMON_MANIFEST):
        m = json.load(open(manifest))
        for case in m["cases"]:
            ids[case["file"]] = case.get("id", case["file"])
    files = sorted(f for f in os.listdir(FIXTURES) if not f.startswith("."))
    return [(ids.get(f, f), os.path.join(FIXTURES, f)) for f in files]


def classify_ubsan(stderr_lines):
    """UBSan `runtime error:` lines with their owning file, when visible."""
    owned = []
    upstream = 0
    unowned = 0
    for i, line in enumerate(stderr_lines):
        if "runtime error:" not in line:
            continue
        # scan nearby lines for a source location (## file.c:line:col)
        ctx = stderr_lines[max(0, i - 6):i + 6]
        where = None
        for c in ctx:
            for pref in OWNED_PREFIXES:
                if pref in c:
                    where = "owned"
            if where is None and UPSTREAM_PREFIX in c:
                where = "upstream"
        if where == "owned":
            owned.append(line.strip())
        elif where == "upstream":
            upstream += 1
        else:
            unowned += 1
    return owned, upstream, unowned


def run_one(mode, path, fail_after=None):
    args = [SAN_BINARY, mode, path]
    if fail_after is not None:
        args.append(str(fail_after))
    r = run(args, env={"ASAN_OPTIONS": ASAN_OPTIONS,
                       "UBSAN_OPTIONS": UBSAN_OPTIONS}, timeout=600)
    return r


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", default=OUT_DIR)
    ap.add_argument("--build-only", action="store_true",
                    help="compile the instrumented binary and stop")
    ap.add_argument("--check", action="store_true",
                    help="read-only validation of an existing sanitizers.json")
    ap.add_argument("--no-build", action="store_true")
    args = ap.parse_args()

    if args.check:
        return validate(args.out)

    if not args.no_build:
        build_binary()
    if args.build_only:
        return 0
    if not os.path.isfile(SAN_BINARY):
        raise SystemExit(f"instrumented binary missing: {SAN_BINARY}")

    asan_reports = []
    leak_reports = []
    owned_ubsan = []
    upstream_ubsan = 0
    crashes = []
    invocations = 0
    decoded_frames = 0

    for cid, path in list_fixtures():
        # dense consume: record + states on every fixture; error fixtures
        # also go through neg; iofail on the fault-injection case only
        modes = [("record", None), ("states", None)]
        if cid == "e11-iofail":
            modes.append(("neg", None))
            modes.append(("iofail", 0))
            modes.append(("iofail", 512))
        elif cid.startswith("e11-neg-"):
            modes.append(("neg", None))
        for mode, fa in modes:
            invocations += 1
            try:
                r = run_one(mode, path, fa)
            except subprocess.TimeoutExpired:
                crashes.append(f"{cid}/{mode}: TIMEOUT")
                continue
            err = r.stderr or ""
            if "ERROR: AddressSanitizer" in err:
                asan_reports.append({"case": cid, "mode": mode,
                                     "head": next(
                                         (l for l in err.splitlines()
                                          if "ERROR: AddressSanitizer" in l),
                                         "")})
            if ("ERROR: LeakSanitizer" in err or
                    "Direct leak of " in err or "Indirect leak of " in err):
                leak_reports.append({"case": cid, "mode": mode,
                                     "head": next(
                                         (l for l in err.splitlines()
                                          if "leak of " in l or
                                          "LeakSanitizer" in l), "")})
            owned, upstream, _ = classify_ubsan(err.splitlines())
            owned_ubsan.extend([{"case": cid, "mode": mode, "line": l}
                                for l in owned])
            upstream_ubsan += upstream
            # record mode exits 1 by design when open/probe fails (negative
            # fixtures); any other non-zero exit is a real crash
            if r.returncode != 0:
                crash = True
                if mode == "record":
                    phase = ""
                    for line in r.stdout.splitlines():
                        if '"phase"' in line:
                            try:
                                phase = json.loads(line).get("phase", "")
                            except ValueError:
                                phase = ""
                            break
                    if phase in ("open_failed", "probe_failed"):
                        crash = False
                if crash:
                    crashes.append(f"{cid}/{mode}: exit {r.returncode}")
            # density metric: count decoded frames from the record JSON
            if mode == "record":
                for line in r.stdout.splitlines():
                    if '"decode"' in line:
                        try:
                            rec = json.loads(line)
                            decoded_frames += rec.get("decode", {}).get(
                                "frames", 0)
                        except ValueError:
                            pass
                        break

    verdict = "PASS"
    problems = []
    if asan_reports:
        verdict = "FAIL"
        problems.append(f"{len(asan_reports)} AddressSanitizer report(s)")
    if leak_reports:
        verdict = "FAIL"
        problems.append(f"{len(leak_reports)} LeakSanitizer report(s)")
    if owned_ubsan:
        verdict = "FAIL"
        problems.append(f"{len(owned_ubsan)} UBSan report(s) in Qianqian code")
    if crashes:
        verdict = "FAIL"
        problems.append(f"{len(crashes)} invocation(s) crashed/timed out")

    evidence = {
        "instrumentation": "address,undefined,leak",
        "instrumented_units": ["src/songcore_ffmpeg.c",
                               "bench/e11/qn_e11_record.c"],
        "closure_linked_uninstrumented": "build/artifacts/libqianqian_av.a",
        "invocations": invocations,
        "fixture_count": len(list_fixtures()),
        "decoded_frames_total": decoded_frames,
        "asan_reports": asan_reports,
        "leak_reports": leak_reports,
        "ubsan_owned": owned_ubsan,
        "ubsan_upstream_noise": upstream_ubsan,
        "crashes": crashes,
        "verdict": verdict,
        "problems": problems,
        "note": ("density pass over the E11 + common-format fixture corpus; "
                 "ASan/Leak are hard gates, UBSan counts only Qianqian-owned "
                 "frames as defects"),
    }

    os.makedirs(args.out, exist_ok=True)
    with open(os.path.join(args.out, "sanitizers.json"), "w") as f:
        json.dump(evidence, f, indent=1, ensure_ascii=False)
        f.write("\n")
    print(f"E11 sanitizers verdict: {verdict} "
          f"({invocations} invocations, {decoded_frames} frames decoded)")
    for p in problems:
        print("  -", p)
    if upstream_ubsan:
        print(f"  (note: {upstream_ubsan} UBSan report(s) attributed to the "
              f"pinned upstream FFmpeg closure, not Qianqian code)")
    return 0 if verdict == "PASS" else 1


def validate(out_dir):
    failures = []
    path = os.path.join(out_dir, "sanitizers.json")
    if not os.path.isfile(path):
        failures.append(f"missing authority file {path}")
    else:
        d = json.load(open(path))
        if d.get("verdict") != "PASS":
            failures.append(f"stored sanitizer verdict is "
                            f"{d.get('verdict')}: {d.get('problems')}")
        if d.get("asan_reports") or d.get("leak_reports"):
            failures.append("stored ASan/leak reports present")
        if d.get("ubsan_owned"):
            failures.append("stored UBSan reports in Qianqian code")
    if failures:
        print("E11 sanitizers --check FAIL")
        for x in failures:
            print("  -", x)
        return 1
    print("E11 sanitizers --check PASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
