#!/usr/bin/env python3
"""E10-B0 driver: build + run the thin DSP reference harness and assemble
the B0 machine evidence:

  b0-correctness.json   gain/biquad/eq10/limiter/nan correctness
  b0-memory.json        logical PCM passes per chain + allocations
  b0-dsp-response.json  measured vs RBJ-analytical frequency response
  b0-summary.json       assembled

  python3 tools/pcm_b0.py            # full run
  python3 tools/pcm_b0.py --check    # verify summary vs sections
"""

import argparse
import json
import math
import os
import subprocess
import sys
from pathlib import Path

import numpy as np

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "bench/results/pcm-processing"
BIN = ROOT / "build/pcm-b0"

SRCS = ["bench/pcm/src/b0_harness.c", "bench/pcm/src/b0_dsp.c"]
FLAGS = ["-std=c11", "-O2", "-Wall", "-Wextra"]
SAN_FLAGS = ["-std=c11", "-O1", "-g", "-Wall", "-Wextra",
             "-fsanitize=address,undefined", "-fno-omit-frame-pointer"]
WRAP = "-Wl,--wrap=malloc,--wrap=calloc,--wrap=free"

BANDS = [31.25, 62.5, 125, 250, 500, 1000, 2000, 4000, 8000, 16000]


def sh(cmd, **kw):
    return subprocess.run(cmd, capture_output=True, text=True, **kw)


def git(*args):
    r = sh(["git", "-C", str(ROOT), *args])
    return r.stdout.strip() if r.returncode == 0 else None


def build(out_name="b0_harness", flags=None):
    binpath = BIN / out_name
    BIN.mkdir(parents=True, exist_ok=True)
    r = sh(["cc", *(flags or FLAGS), WRAP, "-I", "bench/pcm/src", *SRCS,
            "-lm", "-o", str(binpath)], cwd=ROOT)
    if r.returncode != 0:
        print(r.stderr, file=sys.stderr)
        raise SystemExit("build failed")
    return binpath


def run_sanitizer():
    """ASan+UBSan+LSan build/run of the full harness; leak detection is
    the machine evidence for the reprepare/destroy ownership gates
    (review: lifecycle tests under sanitizer where available)."""
    san_bin = build("b0_harness_san", SAN_FLAGS)
    san_dir = BIN / "sanitized"
    san_dir.mkdir(parents=True, exist_ok=True)
    env = dict(os.environ, ASAN_OPTIONS="detect_leaks=1")
    r = subprocess.run([str(san_bin), str(san_dir)], capture_output=True,
                       text=True, env=env, cwd=ROOT)
    clean = (r.returncode == 0 and
             "ERROR: AddressSanitizer" not in r.stderr and
             "LeakSanitizer" not in r.stderr and
             "runtime error:" not in r.stderr)
    print((r.stdout or "").strip())
    if not clean:
        print(r.stderr[-1500:], file=sys.stderr)
    return {"verdict": "PASS" if clean else "FAIL",
            "flags": SAN_FLAGS,
            "leak_detection": "LSan detect_leaks=1"}


def rbj_response(sr, f0, q, gain_db, freqs):
    """RBJ peaking biquad magnitude response at freqs (analytical)."""
    A = 10 ** (gain_db / 40.0)
    w0 = 2 * math.pi * np.array(f0) / sr
    alpha = np.sin(w0) / (2 * q)
    a0 = 1 + alpha / A
    b0 = (1 + alpha * A) / a0
    b1 = (-2 * np.cos(w0)) / a0
    b2 = (1 - alpha * A) / a0
    a1 = (-2 * np.cos(w0)) / a0
    a2 = (1 - alpha / A) / a0
    w = 2 * np.pi * np.asarray(freqs, dtype=float) / sr
    z = np.exp(-1j * w)
    H = (b0 + b1 * z + b2 * z ** 2) / (1 + a1 * z + a2 * z ** 2)
    return 20 * np.log10(np.abs(H) + 1e-12)


def measure_response(imp_raw, sr, freqs):
    imp = np.fromfile(imp_raw, dtype=np.float32)
    n = len(imp)
    # zero-pad to 8x for frequency resolution
    nfft = 1 << (int(np.log2(n)) + 3)
    spec = np.abs(np.fft.rfft(imp, nfft))
    # FFT of the impulse response IS the transfer function (rfft of a
    # unit impulse is 1 at every bin); no nfft normalization.
    spec_db = 20 * np.log10(spec + 1e-12)
    spec_freqs = np.fft.rfftfreq(nfft, 1.0 / sr)
    measured = []
    for f in freqs:
        idx = np.argmin(np.abs(spec_freqs - f))
        measured.append(spec_db[idx])
    return np.array(measured)


def run_response():
    """Compute b0-dsp-response.json from the harness's impulse raws."""
    sr = 48000
    freqs = np.geomspace(20, 20000, 200)
    results = {}
    # biquad 1000Hz +6dB Q=1
    b_imp = Path("/tmp/b0_imp_biquad.raw")
    if b_imp.exists():
        meas = measure_response(b_imp, sr, freqs)
        ana = rbj_response(sr, 1000, 1.0, 6.0, freqs)
        err = meas - ana
        # ignore below -90 dB (FFT floor) in error
        valid = ana > -80
        max_err = float(np.max(np.abs(err[valid])))
        # also report the +6dB at 1000Hz achieved
        f1k = np.argmin(np.abs(freqs - 1000))
        results["biquad_peaking_1000hz_6db"] = {
            "max_db_error_20_20k": round(max_err, 3),
            "gain_at_1k_measured_db": round(float(meas[f1k]), 3),
            "gain_at_1k_analytical_db": round(float(ana[f1k]), 3),
            "verdict": "pass" if max_err < 1.0 else "FAIL",
        }
    # eq10 all +6dB
    e_imp = Path("/tmp/b0_imp_eq10.raw")
    if e_imp.exists():
        meas = measure_response(e_imp, sr, freqs)
        ana = np.zeros_like(freqs)
        for f0 in BANDS:
            if f0 < 0.95 * sr / 2:
                ana += rbj_response(sr, f0, 1.0, 6.0, freqs)
        err = meas - ana
        valid = ana > -80
        max_err = float(np.max(np.abs(err[valid])))
        results["eq10_all_bands_6db"] = {
            "max_db_error_20_20k": round(max_err, 3),
            "verdict": "pass" if max_err < 1.5 else "FAIL",
        }
    return {"experiment": "e10-b0", "section": "dsp_response",
            "method": "impulse response FFT vs RBJ analytical; "
                      "bands >= 0.95*nyq skipped",
            "sample_rate": sr, "results": results,
            "verdict": "PASS" if all(
                r["verdict"] == "pass" for r in results.values()) else "FAIL"}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--check", action="store_true")
    args = ap.parse_args()

    if args.check:
        sections = {}
        for name in ["b0-correctness.json", "b0-memory.json",
                     "b0-dsp-response.json"]:
            sections[name] = json.loads((OUT / name).read_text())
        summary = json.loads((OUT / "b0-summary.json").read_text())
        drift = (summary.get("correctness") != sections["b0-correctness.json"]
                 or summary.get("memory") != sections["b0-memory.json"]
                 or summary.get("response") != sections["b0-dsp-response.json"])
        if drift:
            print("DRIFT: b0-summary.json differs from section JSONs")
            return 1
        # verdict must be derived, not trusted from the stored string
        derived = (sections["b0-correctness.json"]["verdict"] == "PASS"
                   and sections["b0-dsp-response.json"]["verdict"] == "PASS"
                   and summary.get("sanitizer", {}).get("verdict") == "PASS"
                   and all(m["verdict"] == "pass"
                           for m in sections["b0-memory.json"]["rows"]))
        expected = "PASS" if derived else "FAIL"
        if summary.get("verdict") != expected:
            print(f"DRIFT: b0-summary verdict {summary.get('verdict')!r} "
                  f"!= derived {expected!r}")
            return 1
        if not derived:
            print("GATE FAIL: B0 derived verdict is FAIL")
            return 1
        print("b0-summary.json in sync with section JSONs; verdict PASS")
        return 0

    build()
    OUT.mkdir(parents=True, exist_ok=True)
    r = sh([str(BIN / "b0_harness"), str(OUT)], cwd=ROOT)
    print(r.stdout, end="")
    if r.returncode != 0:
        print(r.stderr, file=sys.stderr)
        raise SystemExit("b0 harness failed")

    correctness = json.loads((OUT / "b0-correctness.json").read_text())
    memory = json.loads((OUT / "b0-memory.json").read_text())
    response = run_response()
    (OUT / "b0-dsp-response.json").write_text(
        json.dumps(response, indent=1) + "\n")
    sanitizer = run_sanitizer()

    ok = (correctness["verdict"] == "PASS" and
          response["verdict"] == "PASS" and
          sanitizer["verdict"] == "PASS" and
          all(m["verdict"] == "pass" for m in memory["rows"]))
    summary = {
        "experiment": "e10-b0-thin-dsp",
        "authority_files": ["b0-correctness.json", "b0-memory.json",
                            "b0-dsp-response.json"],
        "scope": "scalar reference only; no SIMD; no production ABI",
        "verdict": "PASS" if ok else "FAIL",
        "correctness": correctness,
        "memory": memory,
        "response": response,
        "sanitizer": sanitizer,
        "provenance": {
            "git_parent_commit": git("rev-parse", "HEAD"),
            "git_branch": git("rev-parse", "--abbrev-ref", "HEAD"),
            "compiler": "cc -std=c11 -O2",
            "biquad_authority": "Audio EQ Cookbook (RBJ) peaking, "
                                "Direct Form II Transposed",
        },
    }
    (OUT / "b0-summary.json").write_text(json.dumps(summary, indent=1) + "\n")
    print("b0-summary written")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
