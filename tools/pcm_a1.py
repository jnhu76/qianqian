#!/usr/bin/env python3
"""E10-A1 driver: SRC shootout (BYPASS / swr / soxr / r8b / lsr) under
the P0 RateStage contract. Generates deterministic signals, runs each
candidate across the conversion matrix and block sizes, computes
delay-compensated quality metrics (numpy), measures performance,
allocation and shipping cost, and assembles the a1-* authority JSONs:

  a1-src-quality.json      delay-compensated quality metrics
  a1-src-correctness.json  drain/output-length + first-output + latency
  a1-src-performance.json  ns/frame, xRT (synthetic + real SongCore PCM)
  a1-src-memory.json       post-prepare allocation counts
  a1-src-shipping.json     runner sizes raw/stripped/xz
  a1-summary.json          assembled decision table

  python3 tools/pcm_a1.py            # full shootout
  python3 tools/pcm_a1.py --check    # verify summary vs section JSONs

No numpy/scipy is a product dependency; this is a local experiment.
"""

import argparse
import json
import math
import os
import re
import shutil
import subprocess
import sys
from pathlib import Path

import numpy as np

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "bench/results/pcm-processing"
RUN = ROOT / "build/pcm-a1"
SIG = RUN / "signals"

CANDIDATES = ["swr", "soxr", "r8b", "lsr"]
CONVERSIONS = [(44100, 48000), (48000, 44100), (96000, 48000),
               (48000, 96000), (44100, 96000), (96000, 44100)]
BLOCK_QUALITY = 256
BLOCK_MATRIX = [1, 64, 1024, 4096]

RNG_SEED = 0xE10A1B00

# Honest per-candidate adapter semantics (review: do not hide adapter
# limitations behind a generic "contract-compatible" claim). The
# lifecycle invariants are enforced for EVERY candidate; the
# classification only documents HOW the candidate satisfies the drain
# semantics.
CANDIDATE_CLASSIFICATION = {
    "swr": {
        "class": "GENERIC",
        "rationale": "swr_convert(NULL, 0) is a native flush-to-EOF; "
                     "reset = swr_close + swr_init",
    },
    "soxr": {
        "class": "GENERIC",
        "rationale": "soxr_process(NULL, 0) is a native drain; "
                     "reset = soxr_clear",
    },
    "lsr": {
        "class": "GENERIC",
        "rationale": "end_of_input on the last process call flushes the "
                     "filter tail natively; reset = src_reset",
    },
    "r8b": {
        "class": "ADAPTER_SEMANTICS_SPECIAL_CASE | NO_NATIVE_EOF",
        "rationale": "r8brain has no EOF/drain API: the adapter feeds "
                     "zeros and trims to the cumulative ideal output "
                     "count (total_input_fed based); getLatency() is 0. "
                     "reset() must also zero the cumulative accounting "
                     "(fixed this branch) or post-reset drain mis-trims",
    },
}

SECTION_JSONS = ["a1-src-quality.json", "a1-src-correctness.json",
                 "a1-src-performance.json", "a1-src-memory.json",
                 "a1-src-shipping.json", "a1-src-lifecycle.json"]


def sh(cmd, **kw):
    return subprocess.run(cmd, capture_output=True, text=True, **kw)


def git(*args):
    r = sh(["git", "-C", str(ROOT), *args])
    return r.stdout.strip() if r.returncode == 0 else None


# ---------------------------------------------------------------- #
# signal generation                                                #
# ---------------------------------------------------------------- #

def gen_signals(sigdir):
    sigdir.mkdir(parents=True, exist_ok=True)
    rng = np.random.default_rng(RNG_SEED)
    signals = {}
    rates = {44100, 48000, 96000}

    def put(name, x, rate):
        # x: 1D mono or (n,2) stereo float32
        if x.ndim == 1:
            x = np.stack([x, x * 0.8], axis=1)
        x = x.astype(np.float32)
        p = sigdir / f"{name}-{rate}.raw"
        x.tofile(p)
        signals[(name, rate)] = str(p)

    for rate in sorted(rates):
        n = rate * 2
        t = np.arange(n) / rate
        # impulse at 25% into the stream
        imp = np.zeros(n)
        imp[int(rate * 0.5)] = 1.0
        put("impulse", imp, rate)
        # DC 0.5
        put("dc", np.full(n, 0.5), rate)
        # 1 kHz sine
        put("sine1k", np.sin(2 * np.pi * 1000 * t), rate)
        # multitone: 10 tones 100..8000 Hz equal amplitude
        mt = np.zeros(n)
        freqs = [100, 250, 500, 1000, 2000, 3000, 4000, 6000, 8000, 10000]
        for f in freqs:
            if f < rate / 2 * 0.9:
                mt += np.sin(2 * np.pi * f * t)
        put("multitone", mt, rate)
        # near-Nyquist passband tone (0.85 * source Nyquist; frequency is
        # preserved by resampling, samples/period is what changes)
        put("near_nq", np.sin(2 * np.pi * 0.85 * (rate / 2) * t), rate)
        # log sweep 20..20k (or up to 0.4*nyq)
        f_lo, f_hi = 20.0, min(20000.0, rate * 0.4)
        k = np.exp(np.linspace(np.log(f_lo), np.log(f_hi), n))
        ph = 2 * np.pi * np.cumsum(k) / rate
        put("sweep", np.sin(ph), rate)
        # white noise (deterministic)
        put("noise", rng.standard_normal(n), rate)
        # alias tone for downsampling: just above destination Nyquist,
        # chosen per conversion in the caller.
    return signals


def alias_tone(in_rate, out_rate, sigdir):
    """Tone at 1.08 * destination Nyquist (must be < source Nyquist)."""
    dest_nyq = out_rate / 2.0
    f = 1.08 * dest_nyq
    if f >= in_rate / 2.0 * 0.98:
        return None
    n = in_rate * 2
    t = np.arange(n) / in_rate
    x = np.sin(2 * np.pi * f * t)
    x = np.stack([x, x * 0.8], axis=1).astype(np.float32)
    p = sigdir / f"alias-{in_rate}-{out_rate}.raw"
    x.tofile(p)
    return str(p), f


# ---------------------------------------------------------------- #
# runner invocation                                                #
# ---------------------------------------------------------------- #

def run_one(cand, in_rate, out_rate, ch, block, in_raw, out_raw, json_path):
    runner = RUN / f"a1_run_{cand}"
    r = subprocess.run([str(runner), str(in_rate), str(out_rate), str(ch),
                        str(block), in_raw, out_raw or "-", json_path],
                       capture_output=True, text=True)
    if r.returncode != 0:
        raise RuntimeError(f"runner {cand} {in_rate}->{out_rate} failed: "
                           f"{r.stdout[:200]} {r.stderr[:200]}")
    return json.loads(Path(json_path).read_text())


# ---------------------------------------------------------------- #
# quality analysis (numpy)                                          #
# ---------------------------------------------------------------- #

def _sine_fit(x, sr, f):
    """Least-squares fit of a sine at f (+DC) and return (amplitude,
    reconstructed, residual). Exact regardless of FFT bin alignment."""
    n = len(x)
    t = np.arange(n) / sr
    A = np.stack([np.cos(2 * np.pi * f * t),
                  np.sin(2 * np.pi * f * t),
                  np.ones(n)], axis=1)
    coef, *_ = np.linalg.lstsq(A, x, rcond=None)
    fit = A @ coef
    amp = math.hypot(coef[0], coef[1])
    return amp, fit, x - fit


def _tone_amp(x, sr, f):
    amp, _, _ = _sine_fit(x, sr, f)
    return amp


def _thd_n(x, sr, f):
    """THD+N in dB: fit and subtract the fundamental (+DC), residual
    energy over the fitted fundamental energy."""
    amp, fit, resid = _sine_fit(x, sr, f)
    num = np.sqrt(np.sum(resid ** 2))
    den = np.sqrt(np.sum(fit ** 2))
    return 20 * math.log10(num / (den + 1e-12) + 1e-12)


def measure_quality(cand, in_rate, out_rate, signals, sigdir, runs_dir):
    """Run the quality signal set and compute delay-compensated metrics."""
    res = {"candidate": cand, "in_rate": in_rate, "out_rate": out_rate,
           "channels": 2, "block_frames": BLOCK_QUALITY,
           "signals": {}}
    ideal_out = None
    for name, in_raw in signals.items():
        jp = runs_dir / f"q_{cand}_{in_rate}_{out_rate}_{name}.json"
        op = runs_dir / f"q_{cand}_{in_rate}_{out_rate}_{name}.raw"
        j = run_one(cand, in_rate, out_rate, 2, BLOCK_QUALITY, in_raw,
                    str(op), jp)
        out = np.fromfile(op, dtype=np.float32).reshape(-1, 2)[:, 0]
        total_in = j["input_frames"]
        ideal = round(total_in * out_rate / in_rate)
        total_out = j["produced_frames"]
        duration_err = (total_out - ideal) / ideal
        drain = j["drain_produced_frames"]
        res["signals"][name] = {
            "output_frames": int(total_out),
            "ideal_frames": ideal,
            "duration_error_frames": int(total_out - ideal),
            "duration_error_ratio": round(float(duration_err), 6),
            "drain_frames": int(drain),
            "first_output_input_pos": int(j["first_output_input_pos"]),
            "latency_frames_reported": int(j["latency_frames_reported"]),
        }
        if ideal_out is None:
            ideal_out = ideal

    # impulse-based delay / ringing (run impulse again for per-cand value)
    imp_raw = signals.get("impulse")
    if imp_raw:
        jp = runs_dir / f"q_{cand}_{in_rate}_{out_rate}_impulse.json"
        op = runs_dir / f"q_{cand}_{in_rate}_{out_rate}_impulse.raw"
        j = run_one(cand, in_rate, out_rate, 2, BLOCK_QUALITY, imp_raw,
                    str(op), jp)
        out = np.fromfile(op, dtype=np.float32).reshape(-1, 2)[:, 0]
        peak = int(np.argmax(np.abs(out)))
        thr = 0.001 * (np.max(np.abs(out)) + 1e-12)
        above = np.where(np.abs(out) > thr)[0]
        pre = peak - (above[above < peak].max() if np.any(above < peak)
                      else peak)
        post = (above[above > peak].min() if np.any(above > peak)
                else 0) - peak
        res["impulse"] = {
            "delay_frames_measured": peak,
            "pre_ring_frames": int(pre),
            "post_ring_frames": int(post),
            "peak_amplitude": float(np.max(np.abs(out))),
        }

    # DC gain
    dc_raw = signals.get("dc")
    if dc_raw:
        jp = runs_dir / f"q_{cand}_{in_rate}_{out_rate}_dc.json"
        op = runs_dir / f"q_{cand}_{in_rate}_{out_rate}_dc.raw"
        j = run_one(cand, in_rate, out_rate, 2, BLOCK_QUALITY, dc_raw,
                    str(op), jp)
        out = np.fromfile(op, dtype=np.float32).reshape(-1, 2)[:, 0]
        steady = out[int(len(out) * 0.2):int(len(out) * 0.8)]
        res["dc_gain"] = round(float(np.mean(steady) / 0.5), 6)

    # THD+N on 1k sine (steady-state window, delay-compensated)
    sine_raw = signals.get("sine1k")
    if sine_raw:
        jp = runs_dir / f"q_{cand}_{in_rate}_{out_rate}_sine1k.json"
        op = runs_dir / f"q_{cand}_{in_rate}_{out_rate}_sine1k.raw"
        j = run_one(cand, in_rate, out_rate, 2, BLOCK_QUALITY, sine_raw,
                    str(op), jp)
        out = np.fromfile(op, dtype=np.float32).reshape(-1, 2)[:, 0]
        delay = res["impulse"]["delay_frames_measured"]
        start = int(delay + out_rate * 0.2)
        end = int(out_rate * 1.4)
        seg = out[start:end]
        res["thd_n_1k_dB"] = round(_thd_n(seg, out_rate, 1000.0), 1)
        amp, _, _ = _sine_fit(seg, out_rate, 1000.0)
        res["sine_amplitude_db"] = round(20 * math.log10(amp + 1e-12), 2)

    # passband ripple on multitone
    mt_raw = signals.get("multitone")
    if mt_raw:
        jp = runs_dir / f"q_{cand}_{in_rate}_{out_rate}_multitone.json"
        op = runs_dir / f"q_{cand}_{in_rate}_{out_rate}_multitone.raw"
        j = run_one(cand, in_rate, out_rate, 2, BLOCK_QUALITY, mt_raw,
                    str(op), jp)
        out = np.fromfile(op, dtype=np.float32).reshape(-1, 2)[:, 0]
        delay = res["impulse"]["delay_frames_measured"]
        start = int(delay + out_rate * 0.2)
        end = int(out_rate * 1.4)
        seg = out[start:end]
        # multitone input = sum of 10 unit sines -> each tone ~0 dB
        gains = []
        for f in [100, 250, 500, 1000, 2000, 3000, 4000, 6000, 8000, 10000]:
            a = _tone_amp(seg, out_rate, f)
            gains.append(round(20 * math.log10(a + 1e-12), 2))
        res["passband_ripple_db"] = round(max(gains) - min(gains), 2)
        res["passband_gain_db"] = gains

        # near-Nyquist passband tone: at 0.85 * SOURCE Nyquist. Frequency is
        # PRESERVED by resampling (Hz stays Hz; only samples/period change),
        # so the output tone sits at the same f. Valid only when the tone is
        # below the destination Nyquist (otherwise it aliases by design).
        nq_raw = signals.get("near_nq")
        if nq_raw:
            f_near = 0.85 * (in_rate / 2)
            if f_near < out_rate / 2:
                jp = runs_dir / f"q_{cand}_{in_rate}_{out_rate}_near_nq.json"
                op = runs_dir / f"q_{cand}_{in_rate}_{out_rate}_near_nq.raw"
                j = run_one(cand, in_rate, out_rate, 2, BLOCK_QUALITY, nq_raw,
                            str(op), jp)
                out = np.fromfile(op, dtype=np.float32).reshape(-1, 2)[:, 0]
                delay = res["impulse"]["delay_frames_measured"]
                seg = out[int(delay + out_rate * 0.2): int(out_rate * 1.4)]
                amp = _tone_amp(seg, out_rate, f_near)
                res["near_nyquist_gain_db"] = round(
                    20 * math.log10(amp + 1e-12), 2)
                res["near_nyquist_freq"] = round(f_near, 1)
            else:
                res["near_nyquist_gain_db"] = None
                res["near_nyquist_note"] = "tone would alias below dest " \
                                           "Nyquist; skipped"

    # alias rejection on downsampling
    if in_rate > out_rate:
        ap = alias_tone(in_rate, out_rate, sigdir)
        if ap is not None:
            ap_path, f_alias = ap
            jp = runs_dir / f"q_{cand}_{in_rate}_{out_rate}_alias.json"
            op = runs_dir / f"q_{cand}_{in_rate}_{out_rate}_alias.raw"
            j = run_one(cand, in_rate, out_rate, 2, BLOCK_QUALITY, ap_path,
                        str(op), jp)
            out = np.fromfile(op, dtype=np.float32).reshape(-1, 2)[:, 0]
            delay = res["impulse"]["delay_frames_measured"]
            seg = out[int(delay + out_rate * 0.2): int(out_rate * 1.4)]
            dest_nyq = out_rate / 2
            folded = dest_nyq - (f_alias - dest_nyq)  # image location
            amp_img = _tone_amp(seg, out_rate, folded)
            res["alias_rejection_db"] = round(
                20 * math.log10(amp_img + 1e-12), 1)
            res["alias_tone_freq"] = round(f_alias, 1)
            res["alias_tone_note"] = "input tone above destination Nyquist"
        else:
            res["alias_rejection_db"] = None
            res["alias_tone_note"] = "no valid tone above dest Nyquist " \
                                     "(too close to source Nyquist)"

    # imaging on upsampling: output energy above input Nyquist
    if in_rate < out_rate:
        swp_raw = signals.get("sweep")
        jp = runs_dir / f"q_{cand}_{in_rate}_{out_rate}_sweep.json"
        op = runs_dir / f"q_{cand}_{in_rate}_{out_rate}_sweep.raw"
        j = run_one(cand, in_rate, out_rate, 2, BLOCK_QUALITY, swp_raw,
                    str(op), jp)
        out = np.fromfile(op, dtype=np.float32).reshape(-1, 2)[:, 0]
        delay = res["impulse"]["delay_frames_measured"]
        seg = out[int(delay + out_rate * 0.2): int(out_rate * 1.4)]
        seg = seg - np.mean(seg)
        nperseg = 16384
        w = np.hanning(nperseg)
        nseg = max(1, (len(seg) - nperseg) // (nperseg // 2) + 1)
        spec = np.zeros(nperseg // 2 + 1)
        for i in range(nseg):
            s = seg[i * (nperseg // 2): i * (nperseg // 2) + nperseg]
            if len(s) < nperseg:
                s = np.pad(s, (0, nperseg - len(s)))
            spec += np.abs(np.fft.rfft(s * w)) ** 2
        spec = np.sqrt(spec / nseg)
        freqs = np.fft.rfftfreq(nperseg, 1.0 / out_rate)
        above = freqs > in_rate / 2 * 0.95
        below = (freqs > 200) & (freqs < in_rate / 2 * 0.8)
        above_p = np.sum(spec[above] ** 2)
        below_p = np.sum(spec[below] ** 2)
        res["imaging_above_input_nyquist_db"] = round(
            10 * math.log10(above_p / (below_p + 1e-12) + 1e-12), 1)

    return res


# ---------------------------------------------------------------- #
# performance / memory / shipping                                   #
# ---------------------------------------------------------------- #

def run_performance(sigdir, runs_dir):
    results = []
    streams = {}
    # synthetic long stream (60 s stereo @96k)
    rng = np.random.default_rng(0xA1E10)
    n = 96000 * 60
    x = (0.7 * np.sin(2 * np.pi * 440 * np.arange(n) / 96000) +
         0.3 * rng.standard_normal(n)).astype(np.float32)
    x = np.stack([x, x], axis=1)
    sp = sigdir / "perf_long_96k.raw"
    x.tofile(sp)
    streams["synthetic_96k_60s"] = (sp, 96000)

    # real SongCore PCM: decode corpus files at their source rates first
    for raw, rate in decode_real_pcm():
        streams[f"real_songcore_{rate}"] = (raw, rate)

    for cand in CANDIDATES:
        for in_rate, out_rate in [(44100, 48000), (96000, 44100)]:
            for name, (raw, rate) in streams.items():
                if rate != in_rate:
                    continue
                reps = 3
                times = []
                for rep in range(reps):
                    jp = runs_dir / f"perf_{cand}_{in_rate}_{out_rate}_{name}_{rep}.json"
                    j = run_one(cand, in_rate, out_rate, 2, 1024, raw,
                                "-", jp)
                    times.append(j["ns_per_input_frame"])
                    last = j
                med = sorted(times)[len(times) // 2]
                xrt = last["xrt"]
                results.append({
                    "candidate": cand, "in_rate": in_rate,
                    "out_rate": out_rate, "stream": name,
                    "reps": reps,
                    "ns_per_input_frame_median": round(med, 3),
                    "xrt": round(xrt, 2),
                    "post_prepare_alloc_calls": last["alloc"]["post_prepare_calls"],
                })
    return results


def decode_real_pcm():
    """Decode real SongCore corpus files to raw Float32 via the repo's
    qn_pcm_dump (QPCM container on stdout: 'QPCM' + u32le rate + u16le
    ch + u16le format(1=f32le) + interleaved f32le). Returns list of
    (path, rate). Fail loud: a silent degradation to synthetic-only
    perf evidence would quietly drop the real-PCM rows the committed
    claims reference (this actually happened once — caught by the
    report-table formatter crashing on the missing columns)."""
    dump = ROOT / "build/artifacts/qn_pcm_dump"
    if not dump.exists():
        raise SystemExit("qn_pcm_dump missing — build it with "
                         "`xmake qn_pcm_dump`; refusing to write perf "
                         "evidence without the real-SongCore rows")
    result = []
    wanted = [("flac-16-44-stereo.flac", 44100),
              ("wav-s24le-96-stereo.wav", 96000)]
    for fname, rate in wanted:
        src = ROOT / "corpus/fixtures" / fname
        if not src.exists():
            raise SystemExit(f"corpus fixture missing: {src}")
        r = subprocess.run([str(dump), str(src)], capture_output=True)
        data = r.stdout
        if r.returncode != 0 or len(data) < 12:
            raise SystemExit(f"qn_pcm_dump failed on {src}")
        if data[:4] != b"QPCM":
            raise SystemExit(f"unexpected dump format for {src}")
        r_rate = int.from_bytes(data[4:8], "little")
        r_ch = int.from_bytes(data[8:10], "little")
        r_fmt = int.from_bytes(data[10:12], "little")
        if r_fmt != 1 or r_rate != rate or r_ch != 2:
            raise SystemExit(f"unexpected PCM geometry for {src}: "
                             f"{r_rate} Hz {r_ch}ch fmt={r_fmt}")
        raw = RUN / f"real_{rate}.raw"
        raw.write_bytes(data[12:])
        result.append((str(raw), rate))
    return result


def measure_shipping():
    rows = []
    for cand in CANDIDATES:
        b = RUN / f"a1_run_{cand}"
        raw = b.stat().st_size
        stripped = RUN / f"a1_run_{cand}_strip"
        shutil.copyfile(b, stripped)
        subprocess.run(["strip", str(stripped)], capture_output=True)
        ss = stripped.stat().st_size
        xz = RUN / f"a1_run_{cand}_strip.xz"
        subprocess.run(["xz", "-9", "-f", "-k", str(stripped)],
                       capture_output=True)
        xs = xz.stat().st_size
        stripped.unlink(missing_ok=True)
        rows.append({
            "candidate": cand,
            "raw_bytes": int(raw),
            "stripped_bytes": int(ss),
            "xz_bytes": int(xs),
            "dynamic_deps": ldd_needed(b),
        })
    bypass = RUN / "a1_run_bypass"
    base = bypass.stat().st_size if bypass.exists() else 0
    return {
        "baseline_bypass_bytes": int(base),
        "rows": rows,
        "deployment_note": "raw/stripped/xz are RUNNER ARTIFACT sizes; "
                           "dynamic_deps records the dynamic linker "
                           "NEEDED set (Linux ldd of the unstripped "
                           "runner) as the new dependency surface the "
                           "candidate would add to a deployment. Artifact "
                           "size != deployment footprint (e.g. soxr "
                           "pulls libgomp via -fopenmp, r8b pulls "
                           "libstdc++ via g++ linkage).",
    }


def ldd_needed(binpath):
    """Structured `ldd` NEEDED set of a runner (Linux). Static binaries
    report dynamic=False."""
    r = subprocess.run(["ldd", str(binpath)], capture_output=True,
                       text=True)
    out = r.stdout.strip()
    if r.returncode != 0 or "not a dynamic executable" in out:
        return {"dynamic": False, "needed": []}
    needed = sorted(set(
        m.group(1) for m in re.finditer(r"^\s*(\S+\.so[^\s]*)\s+=>", out,
                                        re.M)))
    return {"dynamic": True, "needed": needed}


def run_lifecycle(signals):
    """Streaming-contract lifecycle coverage per candidate (review:
    the block matrix previously exercised BYPASS only). One adapter
    instance: pass A (1/64/257-frame chunks, uneven tail) -> drain ->
    reset() -> pass B (1024-frame chunks, uneven tail) -> drain; the two
    output streams must be bit-identical (deterministic within declared
    policy) and accounting must conserve input."""
    lc_dir = RUN / "lifecycle"
    lc_dir.mkdir(parents=True, exist_ok=True)
    in_rate, out_rate = 44100, 48000
    in_raw = signals[("sine1k", in_rate)]
    rows = []
    for cand in CANDIDATES:
        jp = lc_dir / f"lc_{cand}.json"
        r = subprocess.run(
            [str(RUN / f"a1_run_{cand}"), "--lifecycle",
             str(in_rate), str(out_rate), "2", in_raw,
             str(lc_dir / f"lc_{cand}"), str(jp)],
            capture_output=True, text=True)
        if r.returncode != 0:
            rows.append({"candidate": cand, "verdict": "FAIL",
                         "error": (r.stderr or r.stdout)[-400:]})
            continue
        j = json.loads(jp.read_text())
        cls = CANDIDATE_CLASSIFICATION[cand]
        row = {
            "candidate": cand,
            "classification": cls["class"],
            "classification_rationale": cls["rationale"],
            "conversion": f"{in_rate}->{out_rate}",
            "passA_blocks": j["passA_blocks"],
            "passB_blocks": j["passB_blocks"],
            "passA_frames": j["passA"]["produced_frames"],
            "passB_frames": j["passB"]["produced_frames"],
            "passA_drain_rounds": j["passA"]["drain_rounds"],
            "passB_drain_rounds": j["passB"]["drain_rounds"],
            "accounting_ok": j["accounting_ok"],
            "frames_equal": j["frames_equal"],
            "bit_identical": j["bit_identical"],
            "verdict": "pass" if (j["accounting_ok"] and
                                  j["frames_equal"] and
                                  j["bit_identical"] and
                                  j["passA"]["drain_rounds"] < 65536 and
                                  j["passB"]["drain_rounds"] < 65536)
                       else "FAIL",
        }
        rows.append(row)
    return {
        "experiment": "e10-a1", "section": "src_lifecycle",
        "method": "same adapter instance; pass A = 1/64/257-frame chunks "
                  "with uneven final block + drain; reset(); pass B = "
                  "1024-frame chunks with uneven final block + drain; "
                  "outputs must be bit-identical (determinism within "
                  "declared policy), accounting conserved, drain "
                  "terminates",
        "rows": rows,
        "verdict": "PASS" if all(r.get("verdict") == "pass"
                                 for r in rows) else "FAIL",
    }


# ---------------------------------------------------------------- #
# main                                                              #
# ---------------------------------------------------------------- #

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--check", action="store_true")
    ap.add_argument("--quality-only", action="store_true",
                    help="skip perf/shipping (used for iteration)")
    args = ap.parse_args()

    if args.check:
        sec = load_sections()
        summary = json.loads((OUT / "a1-summary.json").read_text())
        drift = []
        for name in SECTION_JSONS:
            if summary.get(section_key(name)) != sec[name]:
                drift.append(name)
        fresh_verdict = derive_verdict(sec)
        if drift:
            print("DRIFT: a1-summary.json differs from section JSONs: "
                  + ", ".join(drift))
            return 1
        if summary.get("verdict") != fresh_verdict:
            print(f"DRIFT: a1-summary verdict {summary.get('verdict')!r} "
                  f"!= derived {fresh_verdict!r}")
            return 1
        if fresh_verdict != "PASS":
            print("GATE FAIL: A1 derived verdict is", fresh_verdict)
            return 1
        print("a1-summary.json in sync with section JSONs; verdict PASS")
        return 0

    RUN.mkdir(parents=True, exist_ok=True)
    runs_dir = RUN / "runs"
    runs_dir.mkdir(parents=True, exist_ok=True)
    sigdir = SIG
    signals = gen_signals(sigdir)

    # quality + correctness across the matrix
    quality_rows = []
    correctness_rows = []
    for in_rate, out_rate in CONVERSIONS:
        qsignals = {name: p for (name, r), p in signals.items() if r == in_rate}
        for cand in CANDIDATES:
            q = measure_quality(cand, in_rate, out_rate, qsignals, sigdir,
                                runs_dir)
            quality_rows.append(q)
            # correctness row from the sine run (duration + latency)
            s = q["signals"].get("sine1k", {})
            correctness_rows.append({
                "candidate": cand, "in_rate": in_rate, "out_rate": out_rate,
                "input_frames": 2 * in_rate,
                "output_frames": s.get("output_frames"),
                "ideal_frames": s.get("ideal_frames"),
                "duration_error_frames": s.get("duration_error_frames"),
                "drain_frames": s.get("drain_frames"),
                "first_output_input_pos": s.get("first_output_input_pos"),
                "latency_frames_reported": s.get("latency_frames_reported"),
                "impulse_delay_frames": q.get("impulse", {}).get(
                    "delay_frames_measured"),
            })

    # bypass sanity (same-rate transparency, block matrix 1..4096)
    bypass_sanity = []
    for blk in BLOCK_MATRIX + [BLOCK_QUALITY]:
        in_raw = signals[("sine1k", 48000)]
        jp = runs_dir / f"bypass_48000_{blk}.json"
        j = run_one("bypass", 48000, 48000, 2, blk, in_raw, "-", jp)
        bypass_sanity.append({
            "block": blk,
            "bit_identical": j["produced_frames"] == j["input_frames"],
            "latency": j["latency_frames_reported"],
        })

    # memory / allocation (from all quality + perf runs)
    memory_rows = {}
    for jf in runs_dir.glob("*.json"):
        j = json.loads(jf.read_text())
        c = j["candidate"]
        if c not in CANDIDATES:
            continue
        if c not in memory_rows:
            memory_rows[c] = {"candidate": c, "max_post_prepare_calls": 0,
                              "max_post_prepare_bytes": 0, "runs": 0}
        memory_rows[c]["max_post_prepare_calls"] = max(
            memory_rows[c]["max_post_prepare_calls"],
            j["alloc"]["post_prepare_calls"])
        memory_rows[c]["max_post_prepare_bytes"] = max(
            memory_rows[c]["max_post_prepare_bytes"],
            j["alloc"]["post_prepare_bytes"])
        memory_rows[c]["runs"] += 1

    # performance + shipping
    performance_rows = []
    shipping = None
    if not args.quality_only:
        performance_rows = run_performance(sigdir, runs_dir)
        shipping = measure_shipping()

    # streaming lifecycle coverage (per candidate)
    lifecycle = run_lifecycle(signals)

    # assemble
    quality_sec = {"experiment": "e10-a1", "section": "src_quality",
                   "rows": quality_rows}
    correctness_sec = {"experiment": "e10-a1",
                       "section": "src_correctness_duration",
                       "method": "deterministic signals; delay measured via "
                                 "impulse; duration vs ideal = in*out/in",
                       "rows": correctness_rows,
                       "bypass_sanity_same_rate": bypass_sanity}
    memory_sec = {"experiment": "e10-a1", "section": "src_memory",
                  "instrument": "linker --wrap malloc/calloc/realloc/free; "
                                "post-prepare = process+drain+reset region",
                  "rows": sorted(memory_rows.values(),
                                 key=lambda r: r["candidate"])}
    perf_sec = {"experiment": "e10-a1", "section": "src_performance",
                "method": "CLOCK_MONOTONIC, block 1024, median of 3 reps; "
                          "real PCM pre-decoded before timing",
                "rows": performance_rows}
    shipping_sec = {"experiment": "e10-a1", "section": "src_shipping",
                    "method": "standalone runner per candidate with "
                              "equivalent responsibility; stripped; xz -9",
                    **shipping} if shipping else None

    for name, sec in [("a1-src-quality.json", quality_sec),
                      ("a1-src-correctness.json", correctness_sec),
                      ("a1-src-memory.json", memory_sec),
                      ("a1-src-performance.json", perf_sec),
                      ("a1-src-lifecycle.json", lifecycle)]:
        (OUT / name).write_text(json.dumps(sec, indent=1) + "\n")
    if shipping_sec:
        (OUT / "a1-src-shipping.json").write_text(
            json.dumps(shipping_sec, indent=1) + "\n")

    sec_for_verdict = {
        "a1-src-quality.json": quality_sec,
        "a1-src-correctness.json": correctness_sec,
        "a1-src-memory.json": memory_sec,
        "a1-src-performance.json": perf_sec,
        "a1-src-lifecycle.json": lifecycle,
        "a1-src-shipping.json": shipping_sec,
    }
    verdict = derive_verdict(sec_for_verdict)
    summary = {
        "experiment": "e10-a1-src-shootout",
        "authority_files": SECTION_JSONS,
        "scope": "selection experiment only; no production decision frozen; "
                 "SRC selection remains deferred",
        "verdict": verdict,
        "verdict_semantics": "PASS = evidence internally consistent and "
                             "every candidate satisfies the RateStage "
                             "streaming contract (or is honestly "
                             "classified); it does NOT mean an SRC/DSP "
                             "backend was selected",
        "quality": quality_sec,
        "correctness": correctness_sec,
        "memory": memory_sec,
        "performance": perf_sec,
        "shipping": shipping_sec,
        "lifecycle": lifecycle,
        "provenance": {
            "git_parent_commit": git("rev-parse", "HEAD"),
            "git_branch": git("rev-parse", "--abbrev-ref", "HEAD"),
            "candidates": {
                "swr": {"ffmpeg": "n9.0.1 bf1b838f2ab88b4f8fd83443325c782ea0e0f7fa",
                        "build": "Qianqian c5 oracle (already in FFmpeg closure)",
                        "license": "LGPL-2.1-or-later"},
                "soxr": {"version": "0.1.3", "sha": "945b592b70470e29f917f4de89b4281fbbd540c0",
                         "license": "LGPL-2.1-or-later"},
                "r8b": {"version": "6.5", "sha": "3c930bf6825c0cfea4a813210c83b1a650c820b5",
                        "license": "MIT"},
                "lsr": {"version": "0.2.2", "sha": "c96f5e3de9c4488f4e6c97f59f5245f22fda22f7",
                        "license": "BSD-2-Clause"},
            },
            "compiler": "cc (gcc) / g++ for r8b, O2, c11/c++14",
        },
    }
    (OUT / "a1-summary.json").write_text(json.dumps(summary, indent=1) + "\n")
    print("a1-summary written; verdict", verdict)
    return 0 if verdict == "PASS" else 1


def load_sections():
    sec = {}
    for f in SECTION_JSONS:
        sec[f] = json.loads((OUT / f).read_text())
    return sec


def section_key(name):
    """Summary key under which a section JSON is embedded."""
    return {
        "a1-src-quality.json": "quality",
        "a1-src-correctness.json": "correctness",
        "a1-src-memory.json": "memory",
        "a1-src-performance.json": "performance",
        "a1-src-shipping.json": "shipping",
        "a1-src-lifecycle.json": "lifecycle",
    }[name]


def derive_verdict(sec):
    """Fail-closed A1 verdict, derived from machine fields (review: the
    old top-level composed hard-coded PASS). Every required gate must
    hold; missing sections fail."""
    try:
        life = sec["a1-src-lifecycle.json"]
        corr = sec["a1-src-correctness.json"]
        mem = sec["a1-src-memory.json"]
        perf = sec["a1-src-performance.json"]
        shp = sec["a1-src-shipping.json"]
    except (KeyError, FileNotFoundError):
        return "FAIL"
    if life.get("verdict") != "PASS":
        return "FAIL"
    # bypass same-rate sanity must be bit-identical at every block size
    if not all(r.get("bit_identical") for r in
               corr.get("bypass_sanity_same_rate", [])):
        return "FAIL"
    # duration accounting: every candidate within declared +-1 frame
    for r in corr.get("rows", []):
        d = r.get("duration_error_frames")
        if d is None or abs(d) > 1:
            return "FAIL"
    mem_cands = {r.get("candidate") for r in mem.get("rows", [])}
    shp_cands = {r.get("candidate") for r in shp.get("rows", [])}
    if mem_cands < set(CANDIDATES) or shp_cands < set(CANDIDATES):
        return "FAIL"
    perf_rows = perf.get("rows", [])
    if not perf_rows:
        return "FAIL"
    # real-SongCore perf rows are part of the claim surface; a run that
    # silently degrades to synthetic-only must not compose a green verdict
    if not any(str(r.get("stream", "")).startswith("real_songcore_")
               for r in perf_rows):
        return "FAIL"
    return "PASS"


if __name__ == "__main__":
    sys.exit(main())
