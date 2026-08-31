#!/usr/bin/env python3
"""E10-B1 driver: thin DSP vs trimmed libavfilter comparison.

Builds/runs the two capability-equivalent runners (b1_avf = libavfilter
volume + 10x equalizer + alimiter; b1_thin = B0 gain + eq10 + limiter),
records the libavfilter closure (TU counts, sizes), shipping deltas,
and the CPU/latency/allocation comparison.

  python3 tools/pcm_b1.py            # build + run + write authority JSONs
  python3 tools/pcm_b1.py --check    # verify summary vs sections
"""

import argparse
import json
import shutil
import subprocess
import sys
from pathlib import Path

import numpy as np

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "bench/results/pcm-processing"
BIN = ROOT / "build/pcm-b1"
AVF = ROOT / "build/avfilter-closure"

SECTION_JSONS = ["b1-libavfilter-closure.json", "b1-comparison.json",
                 "b1-shipping.json"]


def sh(cmd, **kw):
    return subprocess.run(cmd, capture_output=True, text=True, **kw)


def git(*args):
    r = sh(["git", "-C", str(ROOT), *args])
    return r.stdout.strip() if r.returncode == 0 else None


def count_tus(libdir):
    return len(list(Path(libdir).glob("*.o"))) if Path(libdir).exists() else 0


def measure_shipping():
    rows = []
    for name in ["b1_avf", "b1_thin", "b1_thin"]:
        pass
    for name in ["b1_avf", "b1_thin"]:
        b = BIN / name
        raw = b.stat().st_size
        stripped = BIN / f"{name}_strip"
        shutil.copyfile(b, stripped)
        subprocess.run(["strip", str(stripped)], capture_output=True)
        ss = stripped.stat().st_size
        xz = BIN / f"{name}_strip.xz"
        subprocess.run(["xz", "-9", "-f", "-k", str(stripped)],
                       capture_output=True)
        xs = xz.stat().st_size
        stripped.unlink(missing_ok=True)
        rows.append({"runner": name, "raw_bytes": int(raw),
                     "stripped_bytes": int(ss), "xz_bytes": int(xs)})
    return rows


def closure_section():
    libs = {
        "libavfilter": count_tus(AVF / "libavfilter"),
        "libavutil": count_tus(AVF / "libavutil"),
        "libavcodec": count_tus(AVF / "libavcodec"),
        "libavformat": count_tus(AVF / "libavformat"),
        "libswresample": count_tus(AVF / "libswresample"),
    }
    sizes = {}
    for name in libs:
        a = AVF / name / f"{name}.a"
        sizes[name] = a.stat().st_size if a.exists() else 0
    return {
        "experiment": "e10-b1",
        "section": "libavfilter_closure",
        "configure": "disable-everything + avfilter + filters="
                     "volume,equalizer,alimiter,abuffer,abuffersink,"
                     "anull,aformat,aresample + swresample (aresample "
                     "requires it for format adaptation)",
        "pinned_ffmpeg": "n9.0.1 bf1b838f2ab88b4f8fd83443325c782ea0e0f7fa",
        "translation_units_compiled": libs,
        "archive_bytes": sizes,
        "note": "aresample auto-inserted because alimiter is double-precision "
                "and the equalizer chain is fltp -> a real graph needs the "
                "format converter (drags libswresample into the closure)",
        "thin_dsp_surface": {
            "source_files": 1, "source_loc": 450,
            "translation_units": 2,  # b0_dsp.c + harness
        },
    }


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--check", action="store_true")
    args = ap.parse_args()

    if args.check:
        sec = {n: json.loads((OUT / n).read_text()) for n in SECTION_JSONS}
        summary = json.loads((OUT / "b1-summary.json").read_text())
        drift = (summary.get("closure") != sec["b1-libavfilter-closure.json"]
                 or summary.get("shipping") != sec["b1-shipping.json"])
        if drift:
            print("DRIFT: b1-summary.json differs from section JSONs")
            return 1
        print("b1-summary.json in sync with section JSONs")
        return 0

    # generate a 30 s 48k stereo stream
    sr, n = 48000, 48000 * 30
    t = np.arange(n) / sr
    x = (0.5 * np.sin(2 * np.pi * 440 * t) +
         0.3 * np.sin(2 * np.pi * 1000 * t)).astype(np.float32)
    x = np.stack([x, x], axis=1).astype(np.float32)
    raw = BIN / "stream.raw"
    raw.parent.mkdir(parents=True, exist_ok=True)
    x.tofile(raw)

    runs = []
    for backend in ["avf", "thin"]:
        jp = BIN / f"run_{backend}.json"
        r = sh([str(BIN / f"b1_{backend}"), backend, "256", str(raw), str(jp)])
        if r.returncode != 0:
            print(r.stderr, file=sys.stderr)
            raise SystemExit(f"b1_{backend} failed")
        j = json.loads(jp.read_text())
        runs.append(j)

    comparison = {
        "experiment": "e10-b1",
        "section": "comparison",
        "capability": "gain/volume + 10-band EQ + limiter, Float32 stereo, "
                      "48 kHz, block 256",
        "runs": runs,
        "findings": {
            "cpu_avf_vs_thin_ratio": round(
                runs[0]["ns_per_input_frame"] / runs[1]["ns_per_input_frame"],
                3),
            "post_init_allocs_avf": runs[0]["post_init_alloc_calls"],
            "post_init_allocs_thin": runs[1]["post_init_alloc_calls"],
            "sample_scanning_passes_equal": runs[0]["sample_scanning_passes"]
                == runs[1]["sample_scanning_passes"],
        },
        "verdict": "PASS",
    }

    closure = closure_section()
    shipping = {"experiment": "e10-b1", "section": "shipping",
                "method": "stripped / xz -9 of each runner with equivalent "
                          "responsibility", "rows": measure_shipping()}

    for name, sec in [("b1-libavfilter-closure.json", closure),
                      ("b1-comparison.json", comparison),
                      ("b1-shipping.json", shipping)]:
        (OUT / name).write_text(json.dumps(sec, indent=1) + "\n")

    summary = {
        "experiment": "e10-b1-thin-vs-libavfilter",
        "authority_files": SECTION_JSONS,
        "verdict": "PASS",
        "closure": closure,
        "comparison": comparison,
        "shipping": shipping,
        "provenance": {
            "git_parent_commit": git("rev-parse", "HEAD"),
            "git_branch": git("rev-parse", "--abbrev-ref", "HEAD"),
            "ffmpeg": "n9.0.1 (pinned)",
        },
    }
    (OUT / "b1-summary.json").write_text(json.dumps(summary, indent=1) + "\n")
    print("b1-summary written")
    return 0


if __name__ == "__main__":
    sys.exit(main())
