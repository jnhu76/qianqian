#!/usr/bin/env python3
import argparse
import hashlib
import json
import os
import random
import shutil
import statistics
import subprocess
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
NATIVE = HERE.parents[1]
INCLUDE = NATIVE / "include"
STATIC = NATIVE / "build" / "artifacts" / "libsongcore.a"
SHARED = NATIVE / "build" / "artifacts" / "shared" / "libsongcore.so"
REFERENCE = HERE / "reference.json"
FIXTURES = HERE / "fixtures"
RESULTS = HERE / "results"

EXPECTED_EXPORTS = [
    "song_audio_stream_count", "song_audio_stream_info", "song_close",
    "song_get_artwork_count", "song_get_artwork_item", "song_get_metadata",
    "song_get_metadata_count", "song_get_metadata_entry", "song_last_error",
    "song_open", "song_probe", "song_read_pcm", "song_seek",
    "song_select_stream", "songcore_abi_version",
]

LEAK_TOKENS = ("av_", "ff_", "swr_")


def die(msg: str) -> None:
    print(f"FATAL: {msg}", file=sys.stderr)
    sys.exit(1)


def run(cmd: list[str]) -> str:
    p = subprocess.run(cmd, capture_output=True, text=True)
    if p.returncode:
        die(f"command failed ({p.returncode}): {' '.join(cmd)}\n{p.stderr[-2000:]}")
    return p.stdout


def harness_json(harness: Path, args: list[str], cpu: int | None) -> dict:
    cmd = [str(harness), *args]
    if cpu is not None and shutil.which("taskset"):
        cmd = ["taskset", "-c", str(cpu)] + cmd
    p = subprocess.run(cmd, capture_output=True, text=True)
    lines = [l for l in p.stdout.splitlines() if l.strip().startswith("{")]
    if p.returncode or not lines:
        die(f"harness {' '.join(args)} failed rc={p.returncode}: {p.stderr[-1000:]}")
    return json.loads(lines[-1])


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def build_harness(cc: str) -> Path:
    out = NATIVE / "build" / "songcore-equivalence-harness"
    out.parent.mkdir(parents=True, exist_ok=True)
    src = HERE / "harness.c"
    run([cc, "-O2", "-std=c11", "-Wall", "-Wextra", f"-I{INCLUDE}",
         str(src), str(STATIC), "-lm", "-lpthread", "-o", str(out)])
    return out


ALLOWED_SHARED_NEEDED = ("libc.so.6", "libm.so.6")


def shared_dependency_gate(so_path: str) -> dict:
    needed = sorted(
        l.split("[")[-1].rstrip("]").strip()
        for l in run(["readelf", "-d", so_path]).splitlines()
        if "NEEDED" in l and "[" in l
    )
    leaks = sorted(set(needed) - set(ALLOWED_SHARED_NEEDED))
    return {
        "shared_needed": needed,
        "shared_dependency_leaks": leaks,
        "pass": not leaks,
    }


def abi_gate() -> dict:
    exports = sorted(
        l.split()[-1]
        for l in run(["nm", "-D", "--defined-only", str(SHARED)]).splitlines()
        if l.strip()
    )
    missing = sorted(set(EXPECTED_EXPORTS) - set(exports))
    extras = sorted(set(exports) - set(EXPECTED_EXPORTS))
    leaks = sorted(e for e in exports if e.startswith(LEAK_TOKENS))
    header = (INCLUDE / "songcore.h").read_text()
    header_leaks = sorted(
        tok for tok in header.replace("\n", " ").split()
        if tok.startswith(LEAK_TOKENS) or (tok.startswith("AV") and tok[2:3].isupper())
    )
    dep = shared_dependency_gate(str(SHARED))
    ok = not missing and not extras and not leaks and not header_leaks and dep["pass"]
    return {
        "exports": exports,
        "expected_count": len(EXPECTED_EXPORTS),
        "actual_count": len(exports),
        "missing": missing,
        "extras": extras,
        "ffmpeg_symbol_leaks": leaks,
        "header_ffmpeg_type_leaks": header_leaks,
        "shared_dependency_gate": dep,
        "shared_needed": dep["shared_needed"],
        "pass": ok,
    }


def artifact_gate() -> dict:
    members = run(["ar", "t", str(STATIC)]).split()
    undefined = [
        l.split()[-1]
        for l in run(["nm", "-u", str(STATIC)]).splitlines()
        if l.strip()
    ]
    external = sorted({u for u in undefined if not u.startswith("__")}
                      - {"m", "pthread"})[:40]
    return {
        "static_bytes": STATIC.stat().st_size,
        "static_members": len(members),
        "shared_bytes": SHARED.stat().st_size,
        "undefined_symbol_kinds": external,
        "pass": len(members) > 0 and SHARED.is_file(),
    }


def preflight(ref: dict) -> None:
    if not STATIC.is_file():
        die(f"missing {STATIC}; run `xmake build songcore` first")
    for fx in ref["fixtures"]:
        p = FIXTURES / fx["file"]
        if not p.is_file():
            die(f"missing fixture {p}")
        got = sha256_file(p)
        if got != fx["fixture_sha256"]:
            die(f"fixture sha mismatch for {p}: {got} != {fx['fixture_sha256']}")


def correctness(harness: Path, ref: dict, cpu: int | None) -> dict:
    out = {}
    for fx in ref["fixtures"]:
        p = FIXTURES / fx["file"]
        r = harness_json(harness, ["correct", str(p)], cpu)
        row = {
            "observed": r,
            "fixture_sha_ok": True,
            "frames_match": r.get("frames") == fx["pcm_frames"],
            "pcm_sha256_match": r.get("pcm_sha256") == fx["pcm_sha256"],
            "terminal_match": r.get("terminal") == "SONG_EOF",
        }
        row["pass"] = row["frames_match"] and row["pcm_sha256_match"] and row["terminal_match"]
        out[fx["id"]] = row
    return out


def perf(harness: Path, ref: dict, cpu: int | None, runs: int) -> dict:
    load1 = float(open("/proc/loadavg").read().split()[0])
    if load1 > 2.0:
        die(f"NOISY_HOST: 1-min loadavg {load1} > 2.0 (historical LOAD_GATE_1MIN)")
    ids = [fx["id"] for fx in ref["fixtures"]]
    files = {fx["id"]: FIXTURES / fx["file"] for fx in ref["fixtures"]}
    raw = {i: {"bench": [], "startup": [], "latency": []} for i in ids}
    timers = []
    rng = random.Random(20260912)
    for _ in range(runs):
        order = ids[:]
        rng.shuffle(order)
        for fid in order:
            raw[fid]["bench"].append(harness_json(harness, ["bench", str(files[fid]), "3", "20"], cpu))
            raw[fid]["startup"].append(harness_json(harness, ["startup", str(files[fid]), "50"], cpu))
            raw[fid]["latency"].append(harness_json(harness, ["latency", str(files[fid]), "1024", "2", "3"], cpu))
        timers.append(harness_json(harness, ["timer", "1000"], cpu))

    perf_out = {}
    for fid in ids:
        xrt = [b["x_realtime"] for b in raw[fid]["bench"]]
        wall = [b["wall_us_median"] for b in raw[fid]["bench"]]
        ttfp = [s["ttfp_us_median"] for s in raw[fid]["startup"]]
        open_us = [s["open_us_median"] for s in raw[fid]["startup"]]
        probe_us = [s["probe_us_median"] for s in raw[fid]["startup"]]
        first_us = [s["first_read_us_median"] for s in raw[fid]["startup"]]
        p50 = [l["p50_us"] for l in raw[fid]["latency"]]
        p99 = [l["p99_us"] for l in raw[fid]["latency"]]
        mx = [l["max_us"] for l in raw[fid]["latency"]]
        hist = ref["historical_performance"][fid]
        row = {
            "x_realtime_median": statistics.median(xrt),
            "x_realtime_per_run": xrt,
            "wall_us_median_of_run_medians": statistics.median(wall),
            "ttfp_us_median": statistics.median(ttfp),
            "open_us_median": statistics.median(open_us),
            "probe_us_median": statistics.median(probe_us),
            "first_read_us_median": statistics.median(first_us),
            "read_p50_us_median": statistics.median(p50),
            "read_p99_us_median": statistics.median(p99),
            "read_max_us_median": statistics.median(mx),
            "historical": hist,
            "delta_x_realtime_percent": round(
                (statistics.median(xrt) / hist["x_realtime"] - 1.0) * 100.0, 2),
            "delta_ttfp_percent": round(
                (statistics.median(ttfp)
                 / (hist["open_median_us"] + hist["probe_median_us"]
                    + hist["first_read_median_us"]) - 1.0) * 100.0, 2),
            "delta_read_p99_percent": round(
                (statistics.median(p99) / hist["read_p99_us"] - 1.0) * 100.0, 2),
        }
        perf_out[fid] = row
    return {
        "fixtures": perf_out,
        "timer_baseline_us": statistics.median([t["clock_pair_us_median"] for t in timers]),
        "protocol": {
            "runs": runs,
            "throughput": {"block_frames": 4096, "warmup": 3, "iterations": 20},
            "startup": {"iterations": 50, "first_read_capacity_frames": 4096},
            "read_latency": {"block_frames": 1024, "warmup": 2, "passes": 3},
            "affinity_cpu": cpu,
            "load_gate_1min": 2.0,
            "shuffle_seed": 20260912,
            "run_order": "fixtures shuffled per run (recorded seed)",
        },
    }


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--cpu", type=int, default=2)
    ap.add_argument("--runs", type=int, default=3)
    ap.add_argument("--skip-perf", action="store_true")
    ap.add_argument("--cc", default=os.environ.get("CC", "gcc"))
    ap.add_argument("--out", default=str(RESULTS / "songcore-equivalence.json"))
    args = ap.parse_args()

    ref = json.loads(REFERENCE.read_text())
    preflight(ref)
    harness = build_harness(args.cc)

    abi = abi_gate()
    art = artifact_gate()
    if not abi["pass"]:
        die("ABI gate failed: " + json.dumps({
            "missing": abi["missing"],
            "extras": abi["extras"],
            "ffmpeg_symbol_leaks": abi["ffmpeg_symbol_leaks"],
            "header_ffmpeg_type_leaks": abi["header_ffmpeg_type_leaks"],
            "shared_dependency_leaks": abi["shared_dependency_gate"]["shared_dependency_leaks"],
        }))
    if not art["pass"]:
        die("artifact gate failed")

    report: dict = {
        "schema": "songcore-equivalence/1",
        "generated_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "source_commit": run(["git", "-C", str(NATIVE), "rev-parse", "HEAD"]).strip(),
        "compiler": run([args.cc, "--version"]).splitlines()[0],
        "harness_opt": "-O2",
        "loadavg_at_start": open("/proc/loadavg").read().split()[:3],
        "abi_gate": abi,
        "artifact_gate": art,
        "correctness": correctness(harness, ref, args.cpu),
    }
    verdict_correct = all(v["pass"] for v in report["correctness"].values())
    report["correctness_verdict"] = "PASS" if verdict_correct else "FAIL"

    regression = None
    if not args.skip_perf:
        report["performance"] = perf(harness, ref, args.cpu, args.runs)
        threshold = ref["perf_regression_threshold"]
        suspects = [
            fid for fid, row in report["performance"]["fixtures"].items()
            if row["delta_x_realtime_percent"] < -threshold * 100.0
        ]
        regression = suspects
        report["performance_verdict"] = (
            "PERF_REGRESSION_SUSPECT: " + ",".join(suspects)) if suspects else "EQUIVALENT_WITHIN_THRESHOLD"

    out = Path(args.out)
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")

    print(f"ABI: {abi['actual_count']}/{abi['expected_count']} exports, "
          f"extras={len(abi['extras'])}, leaks={len(abi['ffmpeg_symbol_leaks'])}"
          f"+{len(abi['header_ffmpeg_type_leaks'])}")
    print(f"artifacts: static {art['static_members']} members, "
          f"shared NEEDED={abi['shared_needed']}")
    print(f"correctness: {report['correctness_verdict']}")
    for fid, row in report["correctness"].items():
        print(f"  {fid}: sha={row['pcm_sha256_match']} frames={row['frames_match']} "
              f"terminal={row['terminal_match']}")
    if not args.skip_perf:
        print(f"performance: {report['performance_verdict']}")
        for fid, row in report["performance"]["fixtures"].items():
            h = row["historical"]
            print(f"  {fid}: x_realtime {h['x_realtime']} -> "
                  f"{row['x_realtime_median']:.1f} ({row['delta_x_realtime_percent']:+.2f}%) "
                  f"ttfp {row['delta_ttfp_percent']:+.2f}% "
                  f"p99 {row['delta_read_p99_percent']:+.2f}%")
    print(f"wrote {out}")
    if not verdict_correct:
        sys.exit(1)
    if regression:
        sys.exit(1)


if __name__ == "__main__":
    main()
