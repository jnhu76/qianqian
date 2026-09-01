#!/usr/bin/env python3
"""E10-A0 driver: build + run the native Windows WASAPI probe and compose
the A0 machine evidence under bench/results/pcm-processing/:
  a0-windows-endpoints.json  a0-format-support.json  a0-reopen.json
  a0-summary.json

The probe runs NATIVELY on Windows (invoked through cmd.exe from WSL).
WSL enumeration alone does NOT count as Windows AudioSink evidence.
Requires Windows interop, Visual Studio 2022 C++ workload, and a Windows
audio render endpoint.

  python3 tools/pcm_a0_windows.py           # build, run, compose
  python3 tools/pcm_a0_windows.py --check   # verify summary consistency
"""

import argparse
import datetime
import json
import platform
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "bench/results/pcm-processing"
WINROOT = Path("/mnt/c/Users/Public/pcm-a0")
WINPATH = r"C:\Users\Public\pcm-a0"
WINOUT = WINPATH + r"\out"

SRC = ROOT / "bench/pcm/windows/qn_wasapi_probe.c"
BAT = ROOT / "tools/pcm_a0_build.bat"
A0_FILES = ["a0-windows-endpoints.json", "a0-format-support.json",
            "a0-reopen.json"]


def sh(cmd, **kw):
    return subprocess.run(cmd, capture_output=True, text=True, **kw)


def win_sh(parts):
    """Run a Windows command; capture raw bytes (Windows console output is
    not UTF-8 on zh-CN locales) and decode leniently."""
    return subprocess.run(["cmd.exe", "/c", *parts], capture_output=True)


def _win_text(r):
    return (r.stdout.decode("utf-8", errors="replace") if r.stdout else "") + \
           (r.stderr.decode("utf-8", errors="replace") if r.stderr else "")


def git(*args):
    r = sh(["git", "-C", str(ROOT), *args])
    return r.stdout.strip() if r.returncode == 0 else None


def build_and_run():
    if not WINROOT.exists():
        raise SystemExit("Windows build area missing "
                         "(expected /mnt/c/Users/Public/pcm-a0)")
    # stage sources + build script into the Windows-visible tree
    for rel, src in [("bench/pcm/windows/qn_wasapi_probe.c", SRC),
                     ("tools/pcm_a0_build.bat", BAT)]:
        dst = WINROOT / rel
        dst.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(src, dst)
    bat = WINPATH + r"\tools\pcm_a0_build.bat"
    r = win_sh([bat])
    if r.returncode != 0:
        print(_win_text(r), file=sys.stderr)
        raise SystemExit("Windows build failed")
    exe = WINPATH + r"\build\pcm-a0\qn_wasapi_probe.exe"
    if not Path("/mnt/c/Users/Public/pcm-a0/build/pcm-a0/qn_wasapi_probe.exe").exists():
        raise SystemExit("probe exe not produced")
    Path("/mnt/c/Users/Public/pcm-a0/out").mkdir(parents=True, exist_ok=True)
    r = win_sh([exe, WINOUT])
    if r.returncode != 0:
        print(_win_text(r), file=sys.stderr)
        raise SystemExit("probe run failed")
    OUT.mkdir(parents=True, exist_ok=True)
    for f in A0_FILES:
        shutil.copyfile(Path("/mnt/c/Users/Public/pcm-a0/out") / f, OUT / f)


def load():
    return {f: json.loads((OUT / f).read_text()) for f in A0_FILES}


def compose_summary(sec):
    ep = sec["a0-windows-endpoints.json"]
    fs = sec["a0-format-support.json"]
    ro = sec["a0-reopen.json"]
    endpoints = []
    for e in ep.get("endpoints", []):
        endpoints.append({
            "endpoint_id_hash": e.get("endpoint_id_hash"),
            "is_default": e.get("is_default"),
            "friendly_name": e.get("friendly_name"),
        })
    rows = []
    for row in fs.get("rows", []):
        res = row["results"][0]
        rates = {str(r["rate"]): r for r in res["rates"]}
        rows.append({
            "endpoint_id_hash": row["endpoint_id_hash"],
            "mix_format": res["mix_format"],
            "engine_period_hns": res["engine_period_hns"],
            "rates": rates,
            "paths": res["paths"],
        })
    # reopen cost per rate (median total + init)
    reopen = [
        {"rate": c["rate"], "iterations": c.get("iterations"),
         "total_cycle_ms_median": c["total_cycle_ms"]["median"],
         "initialize_ms_median": c["initialize_ms"]["median"],
         "total_cycle_ms_min": c["total_cycle_ms"]["min"],
         "total_cycle_ms_max": c["total_cycle_ms"]["max"]}
        for c in ro.get("cycles", [])
    ]

    # Classification: for each endpoint, which of the three paths work.
    classification = []
    for r in rows:
        paths = r["paths"]
        p1 = paths["path1_app_bypass_shared_init"]["result"] == "S_OK"
        p2 = paths["path2_windows_src_shared_init"]["result"] == "S_OK"
        p3 = paths["path3_exclusive_init"]["result"] == "S_OK"
        supported_shared = [rate for rate, info in r["rates"].items()
                            if info["shared_isformat"] == "S_OK"]
        classification.append({
            "endpoint_id_hash": r["endpoint_id_hash"],
            "app_bypass_possible": p1,
            "app_bypass_rates_shared_native": supported_shared,
            "windows_src_available": p2,
            "exclusive_source_rate_available": p3,
            "device_source_rate_exact": p3,  # format acceptance only
        })

    device_evidence = "COLLECTED" if rows else "CODE_COMPLETE_PENDING_VALIDATION"
    return {
        "experiment": "e10-a0-windows-audiosink",
        "authority_files": A0_FILES,
        "device_evidence_status": device_evidence,
        "note": "single-host Windows device evidence; Path 3 'exclusive' is "
                "format acceptance only, NOT bit-perfect; do not generalize "
                "across machines without more endpoints",
        "endpoints": endpoints,
        "endpoint_rows": rows,
        "reopen_cost_ms": reopen,
        "classification": classification,
        "limitations": [
            "single Windows host; one active render endpoint (Realtek)",
            "exclusive-mode IsFormatSupported/Initialize returned "
            "AUDCLNT_E_UNSUPPORTED_FORMAT for every rate on this device",
            "no audible signals emitted; silence only",
            "format acceptance is not absence of driver/device DSP",
        ],
        "provenance": {
            "git_parent_commit": git("rev-parse", "HEAD"),
            "git_branch": git("rev-parse", "--abbrev-ref", "HEAD"),
            "windows_os": platform.uname().system,
            "generated_utc": datetime.datetime.now(
                datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        },
    }


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--check", action="store_true",
                    help="verify a0-summary.json matches the probe JSONs")
    args = ap.parse_args()

    if args.check:
        sec = load()
        summary = json.loads((OUT / "a0-summary.json").read_text())
        fresh = compose_summary(sec)
        # reopen timings are run-to-run volatile (QPC); compare only the
        # structural part (rate + iterations), exact for everything else.
        reopen_struct = lambda r: [
            {"rate": c["rate"], "iterations": c["iterations"]} for c in r]
        drift = (summary.get("endpoint_rows") != fresh["endpoint_rows"] or
                 summary.get("classification") != fresh["classification"] or
                 reopen_struct(summary.get("reopen_cost_ms", [])) !=
                 reopen_struct(fresh["reopen_cost_ms"]))
        if drift:
            print("DRIFT: a0-summary.json differs from probe JSONs; "
                  "run tools/pcm_a0_windows.py to regenerate")
            return 1
        print("a0-summary.json in sync with probe JSONs")
        return 0

    build_and_run()
    sec = load()
    summary = compose_summary(sec)
    (OUT / "a0-summary.json").write_text(
        json.dumps(summary, indent=1) + "\n")
    print(f"a0-summary written (device evidence "
          f"{summary['device_evidence_status']})")
    for c in summary["classification"]:
        print(f"  {c['endpoint_id_hash']}: app_bypass={c['app_bypass_possible']} "
              f"win_src={c['windows_src_available']} "
              f"exclusive={c['exclusive_source_rate_available']}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
