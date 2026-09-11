#!/usr/bin/env python3
import argparse
import hashlib
import json
import os
import platform
import random
import statistics
import struct
import subprocess
import sys
import time
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PROBE_DEFAULT = ROOT / "build" / "perf" / "pcm_perf_probe"
QNBENCH = ROOT / "build" / "perf" / "qn_bench"
PCM_DUMP = ROOT / "build" / "artifacts" / "qn_pcm_dump"
FIXTURES = ROOT / "corpus" / "fixtures"
LOCALS = ROOT / "corpus" / "local"
MANIFEST_DIR = ROOT / "corpus" / "manifest"
RUNS_DIR = ROOT / "bench" / "results" / "runs" / "decode-baseline"
COMMITTED_DIR = ROOT / "bench" / "results" / "decode-baseline"

FIX = lambda name: FIXTURES / name
LOC = lambda name: LOCALS / "yinxing-de-chibi" / name

MATRIX = [
    ("mp3-cbr-id3v23", FIX("mp3-cbr-id3v23.mp3"), "codec-representative"),
    ("mp3-vbr-id3v24", FIX("mp3-vbr-id3v24.mp3"), "codec-variant-vbr"),
    ("mp3-long", FIX("mp3-long.mp3"), "longer-sample-same-codec"),
    ("mp3-short", FIX("mp3-short.mp3"), "short-sample-same-codec"),
    ("flac-16-44-stereo", FIX("flac-16-44-stereo.flac"), "codec-representative"),
    ("flac-24-96", FIX("flac-24-96.flac"), "high-rate-high-depth"),
    ("aac-lc-44-stereo", FIX("aac-lc-44-stereo.m4a"), "codec-representative"),
    ("aac-lc-44-mono", FIX("aac-lc-44-mono.m4a"), "mono"),
    ("alac-16-44-stereo", FIX("alac-16-44-stereo.m4a"), "codec-representative-historical-xrt-family"),
    ("alac-16-44-mono", FIX("alac-16-44-mono.m4a"), "mono"),
    ("alac-24-96-stereo", FIX("alac-24-96-stereo.m4a"), "high-rate-high-depth"),
    ("alac-long", FIX("alac-long.m4a"), "longer-sample-same-codec"),
    ("wav-s16le-44-stereo", FIX("wav-s16le-44-stereo.wav"), "pcm-container"),
    ("wav-f32le-44-stereo", FIX("wav-f32le-44-stereo.wav"), "pcm-container-float"),
    ("vorbis-44-stereo", FIX("vorbis-44-stereo.ogg"), "codec-representative"),
    ("opus-48-stereo", FIX("opus-48-stereo.opus"), "codec-representative-delayed-frame"),
    ("artwork-mp3-jpeg", FIX("artwork-mp3-jpeg.mp3"), "artwork-heavy"),
    ("metadata-full-flac", FIX("metadata-full.flac"), "metadata-heavy"),
    ("yinxing-mp3-cbr-128", LOC("mp3-cbr-128.mp3"), "real-long-media"),
    ("yinxing-mp3-cbr-320-artwork", LOC("mp3-cbr-320-artwork.mp3"), "real-long-media-artwork"),
    ("yinxing-flac-16-44-artwork", LOC("flac-16-44-artwork.flac"), "real-long-media-artwork"),
]
REAL_LONG = {"yinxing-mp3-cbr-128", "yinxing-mp3-cbr-320-artwork", "yinxing-flac-16-44-artwork"}
SWEEP_IDS = {"flac-16-44-stereo", "mp3-cbr-id3v23", "alac-16-44-stereo", "opus-48-stereo", "yinxing-mp3-cbr-128"}
SWEEP_BLOCKS = [64, 128, 256, 512, 1024, 2048, 4096]
MAIN_BLOCK = 1024
THROUGHPUT_BLOCK = 4096
THROUGHPUT_WARMUP = 3
THROUGHPUT_ITERS = 20
STARTUP_ITERS = 50
LATENCY_WARMUP = 2
LATENCY_PASSES = 3
LOAD_GATE_1MIN = 2.0

def now_utc():
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")

def sha256_file(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()

def load_corpus_shas():
    out = {}
    manifests = list(MANIFEST_DIR.glob("*.json")) + [LOCALS / "manifest.json"]
    for mpath in manifests:
        data = json.loads(mpath.read_text())
        for case in data.get("cases", []):
            name = Path(case["file"]).name
            out[name] = case.get("fixture_sha256")
    return out

def loadavg():
    parts = Path("/proc/loadavg").read_text().split()
    return {"1min": float(parts[0]), "5min": float(parts[1]), "15min": float(parts[2]), "runnable": int(parts[3].split("/")[0])}

def affinity_wrap(cpu, cmd):
    return ["taskset", "-c", str(cpu)] + [str(c) for c in cmd]

def run_json(cmd, cpu, allow_failure=False):
    proc = subprocess.run(affinity_wrap(cpu, cmd), cwd=ROOT, capture_output=True, text=True)
    if proc.returncode != 0 and not allow_failure:
        raise SystemExit(f"command failed ({proc.returncode}): {cmd}\n{proc.stderr[-2000:]}")
    lines = [l for l in proc.stdout.splitlines() if l.strip()]
    if not lines:
        raise SystemExit(f"no JSON from command: {cmd}\n{proc.stderr[-2000:]}")
    return json.loads(lines[-1])

def collect_provenance(cpu):
    head = subprocess.run(["git", "rev-parse", "HEAD"], cwd=ROOT, capture_output=True, text=True).stdout.strip()
    branch = subprocess.run(["git", "branch", "--show-current"], cwd=ROOT, capture_output=True, text=True).stdout.strip()
    pin = json.loads((ROOT / "native" / "ffmpeg" / "pin.json").read_text())
    manifest = json.loads((ROOT / "build" / "ffmpeg-xmake" / "manifest.json").read_text())
    gcc = subprocess.run(["gcc", "--version"], capture_output=True, text=True).stdout.splitlines()[0]
    xmake = subprocess.run(["xmake", "--version"], capture_output=True, text=True).stdout.splitlines()[0]
    uname = platform.uname()
    mem_kb = 0
    for line in Path("/proc/meminfo").read_text().splitlines():
        if line.startswith("MemTotal"):
            mem_kb = int(line.split()[1])
            break
    cpu_model = ""
    for line in Path("/proc/cpuinfo").read_text().splitlines():
        if line.startswith("model name"):
            cpu_model = line.split(":", 1)[1].strip()
            break
    os_release = {}
    os_file = Path("/etc/os-release")
    if os_file.is_file():
        for line in os_file.read_text().splitlines():
            if "=" in line:
                k, v = line.split("=", 1)
                os_release[k] = v.strip('"')
    probe = subprocess.run(["gcc", "-O2", "-Q", "--help=common"], capture_output=True, text=True).stdout
    return {
        "captured_utc": now_utc(),
        "source": {
            "git_head": head,
            "git_branch": branch,
            "worktree": "bench branch off evidence/native-repro-corrective-1",
        },
        "ffmpeg": {
            "pin_tag": pin["ffmpeg_tag"],
            "pin_commit_sha": pin["ffmpeg_commit_sha"],
            "pin_source_sha256": pin["source_sha256"],
            "manifest_profile": manifest.get("profile"),
            "manifest_profile_sha256": manifest.get("profile_sha256"),
            "manifest_source_sha256": manifest.get("ffmpeg_source_sha256"),
            "manifest_target_id": (manifest.get("target") or {}).get("id"),
            "translation_units": (manifest.get("closure") or {}).get("translation_units"),
        },
        "compiler": {
            "cc": gcc,
            "driver_opt": "-O2",
            "xmake": xmake.strip(),
        },
        "host": {
            "os": os_release.get("PRETTY_NAME", uname.system),
            "kernel": uname.release,
            "cpu_model": cpu_model,
            "logical_cpus": os.cpu_count(),
            "mem_total_kb": mem_kb,
            "affinity_cpu": cpu,
            "affinity_tool": "taskset -c",
            "governor": "unavailable (WSL2; not claimed)",
        },
    }

def qnbench_canonical_sha(path, cpu):
    data = run_json([str(QNBENCH), "correct", str(path)], cpu)
    dec = data.get("decode", {})
    return dec.get("canonical_f32_sha256"), dec

def pcm_dump_sha(path, cpu):
    proc = subprocess.run(affinity_wrap(cpu, [str(PCM_DUMP), str(path)]), cwd=ROOT, capture_output=True)
    if proc.returncode != 0:
        raise SystemExit(f"qn_pcm_dump failed: {proc.stderr[-500:]}")
    raw = proc.stdout
    if raw[:4] != b"QPCM" or len(raw) < 12:
        raise SystemExit(f"bad QPCM stream for {path}")
    rate, ch, fmt = struct.unpack("<IHH", raw[4:12])
    payload = raw[12:]
    if fmt != 1 or ch <= 0 or len(payload) % (ch * 4):
        raise SystemExit(f"bad QPCM payload for {path}")
    return {"sample_rate": rate, "channels": ch, "pcm_sha256": hashlib.sha256(payload).hexdigest(), "frames": len(payload) // (ch * 4)}

def qnbench_corroborate(path, cpu, iters=10):
    return run_json([str(QNBENCH), "bench", str(path), str(iters)], cpu)

def median(values):
    return statistics.median(values) if values else None

def cv(values):
    if not values or len(values) < 2:
        return None
    m = statistics.mean(values)
    if m == 0:
        return None
    return statistics.pstdev(values) / abs(m)

def measure_file(fid, path, role, expected_sha, cpu, probe, raw_dir):
    snap = loadavg()
    if snap["1min"] > LOAD_GATE_1MIN:
        raise SystemExit(f"NOISY_HOST: load1={snap['1min']} exceeds gate {LOAD_GATE_1MIN}; rerun this run")
    record = {"id": fid, "path": str(path.relative_to(ROOT)), "role": role, "load_before": snap}
    record["fixture_sha256"] = sha256_file(path)
    record["fixture_sha_matches_manifest"] = (expected_sha == record["fixture_sha256"]) if expected_sha else None

    verify = run_json([str(probe), "verify", str(path)], cpu, allow_failure=True)
    record["verify"] = verify
    if verify.get("status") != "ok":
        record["excluded"] = {
            "reason": "correctness_gate_failed",
            "detail": "full-stream decode did not reach clean EOF in both layers; excluded from all performance summaries per measurement discipline (no partial decode in throughput)",
            "verify": verify,
        }
        record["load_after"] = loadavg()
        return record

    dump = pcm_dump_sha(path, cpu)
    record["pcm_dump"] = dump
    sha_gate = dump["pcm_sha256"] == verify["abi"]["pcm_sha256"] and dump["frames"] == verify["abi"]["frames"]
    record["gate_sha_vs_independent_dump"] = sha_gate

    if fid not in REAL_LONG:
        canon, qn_correct = qnbench_canonical_sha(path, cpu)
        record["qnbench_correct_canonical_sha256"] = canon
        record["gate_sha_vs_qnbench_oracle"] = (canon == verify["abi"]["pcm_sha256"]) if canon else None
        record["qnbench_corroboration"] = qnbench_corroborate(path, cpu, iters=10)
    else:
        record["qnbench_correct_canonical_sha256"] = None
        record["gate_sha_vs_qnbench_oracle"] = None

    tp = run_json([str(probe), "throughput", str(path), str(THROUGHPUT_WARMUP), str(THROUGHPUT_ITERS), str(THROUGHPUT_BLOCK)], cpu)
    record["throughput"] = tp
    duration_us = tp.get("duration_us", -1)
    audio_s = duration_us / 1e6 if duration_us and duration_us > 0 else verify["abi"]["frames"] / tp["sample_rate"]
    record["audio_seconds"] = audio_s
    if not (tp["core"]["iters"][0]["demux_eof"] and tp["core"]["iters"][0]["decoder_eof"]):
        raise SystemExit(f"throughput gate failed (core EOF) for {fid}")
    frame_spread = {it["frames"] for it in tp["abi"]["iters"]}
    sample_spread = {it["samples"] for it in tp["core"]["iters"]}
    if len(frame_spread) != 1 or len(sample_spread) != 1:
        raise SystemExit(f"throughput gate failed (frame instability) for {fid}")
    if next(iter(frame_spread)) != verify["abi"]["frames"]:
        raise SystemExit(f"throughput gate failed (frame mismatch vs verify) for {fid}")

    raw_sub = raw_dir / fid
    raw_sub.mkdir(parents=True, exist_ok=True)
    lat = run_json([str(probe), "read-latency", str(path), str(LATENCY_WARMUP), str(LATENCY_PASSES), str(MAIN_BLOCK), str(raw_sub)], cpu)
    record["read_latency_main"] = lat
    if fid in SWEEP_IDS:
        record["block_sweep"] = run_json([str(probe), "read-latency", str(path), str(LATENCY_WARMUP), str(LATENCY_PASSES), ",".join(map(str, SWEEP_BLOCKS)), str(raw_sub)], cpu)

    record["startup_abi"] = run_json([str(probe), "startup-abi", str(path), str(STARTUP_ITERS)], cpu)
    record["startup_core"] = run_json([str(probe), "startup-core", str(path), str(STARTUP_ITERS)], cpu)
    record["load_after"] = loadavg()
    return record

def cmd_run(args):
    probe = Path(args.probe).resolve()
    if not probe.is_file():
        raise SystemExit(f"probe binary missing: {probe}")
    if not QNBENCH.is_file() or not PCM_DUMP.is_file():
        raise SystemExit("qn_bench/qn_pcm_dump binaries missing; build them first")
    started = time.monotonic()
    out_dir = RUNS_DIR / args.label
    out_dir.mkdir(parents=True, exist_ok=True)
    corpus_shas = load_corpus_shas()
    provenance = collect_provenance(args.cpu)
    order = list(MATRIX)
    rng = random.Random(args.seed)
    rng.shuffle(order)
    timer_base = run_json([str(probe), "timer-baseline", "100000"], args.cpu)
    files = []
    for fid, path, role in order:
        print(f"[{args.label}] {fid} ...", flush=True)
        files.append(measure_file(fid, path, role, corpus_shas.get(Path(path).name), args.cpu, probe, out_dir))
    run_doc = {
        "schema": "decode-baseline-run/1",
        "label": args.label,
        "seed": args.seed,
        "started_utc": now_utc(),
        "wall_seconds": round(time.monotonic() - started, 1),
        "provenance": provenance,
        "protocol": {
            "throughput": {"warmup": THROUGHPUT_WARMUP, "iterations": THROUGHPUT_ITERS, "block_frames": THROUGHPUT_BLOCK, "layer_order_per_iteration": "core then abi (interleaved)"},
            "startup": {"iterations": STARTUP_ITERS, "first_read_capacity_frames": 4096},
            "read_latency": {"warmup": LATENCY_WARMUP, "passes": LATENCY_PASSES, "main_block_frames": MAIN_BLOCK, "sweep_blocks": SWEEP_BLOCKS if SWEEP_IDS else [], "percentile_method": "nearest-rank on pooled per-call microseconds", "p999_min_samples": 1000},
            "run_order": "files shuffled with recorded seed; layers interleaved inside throughput mode",
            "load_gate_1min": LOAD_GATE_1MIN,
            "timer": "CLOCK_MONOTONIC (wall), CLOCK_PROCESS_CPUTIME_ID (cpu)",
        },
        "timer_baseline": timer_base,
        "files": files,
    }
    out_path = out_dir / "run.json"
    out_path.write_text(json.dumps(run_doc, indent=2, sort_keys=True) + "\n")
    print(f"[{args.label}] wrote {out_path.relative_to(ROOT)} ({run_doc['wall_seconds']}s)")
    return 0

LAT_KEYS = ["count", "mean_us", "min_us", "p50_us", "p90_us", "p95_us", "p99_us", "p999_us", "max_us", "stddev_us"]

def agg_lat(blocks_by_run):
    stats_by_run = [b["read_call"] for b in blocks_by_run]
    out = {}
    for key in LAT_KEYS:
        vals = [s[key] for s in stats_by_run if s.get(key) is not None]
        out[key] = median(vals)
    vals_batch = [b["batch_mean_call_us"] for b in blocks_by_run]
    out["batch_mean_call_us"] = median(vals_batch)
    out["batch_wall_us"] = median([b["batch_wall_us"] for b in blocks_by_run])
    out["timer_tax_us"] = (out["mean_us"] - out["batch_mean_call_us"]) if (out["mean_us"] is not None and out["batch_mean_call_us"] is not None) else None
    out["flag"] = "INSUFFICIENT_SAMPLE_COUNT" if out.get("p999_us") is None else None
    return out

def throughput_view(run_map):
    labels = sorted(run_map)
    first = run_map[labels[0]]
    audio_s = median([r["audio_seconds"] for r in run_map.values()])
    view = {"audio_seconds": audio_s}
    for layer, count_key in (("core", "samples"), ("abi", "frames")):
        per_run_med = {}
        per_run_cpu = {}
        for label in labels:
            iters = run_map[label]["throughput"][layer]["iters"]
            per_run_med[label] = median([it["wall_us"] for it in iters])
            per_run_cpu[label] = median([it["cpu_us"] for it in iters])
        wall = median(per_run_med.values()) / 1e3
        cpu_med = median(per_run_cpu.values()) / 1e3
        entry = {
            "wall_ms_median": round(wall, 3),
            "cpu_ms_median": round(cpu_med, 3),
            "cpu_to_wall_ratio": round(cpu_med / wall, 3) if wall else None,
            "wall_ms_min": round(min(per_run_med.values()) / 1e3, 3),
            "wall_ms_max": round(max(per_run_med.values()) / 1e3, 3),
            "wall_ms_median_per_run": {label: round(v / 1e3, 3) for label, v in per_run_med.items()},
        }
        c = cv(list(per_run_med.values()))
        entry["run_cv"] = round(c, 4) if c is not None else None
        if audio_s and wall > 0:
            entry["x_realtime"] = round(audio_s / (wall / 1e3), 1)
        frames = first["verify"]["abi"]["frames"]
        channels = first["throughput"]["channels"]
        if wall > 0:
            entry["pcm_mbytes_per_s"] = round(frames * channels * 4 / (wall / 1e3) / 1e6, 1)
            entry["us_per_frame"] = round(wall * 1e3 / frames, 4)
        view[layer] = entry
    cw = view["core"]["wall_ms_median"]
    aw = view["abi"]["wall_ms_median"]
    frames = first["verify"]["abi"]["frames"]
    delta_ms = aw - cw
    view["canonical_output_cost"] = {
        "wall_ms_delta": round(delta_ms, 3),
        "relative_percent": round(delta_ms / cw * 100.0, 2) if cw else None,
        "us_per_frame_delta": round(delta_ms * 1e3 / frames, 4) if frames else None,
        "pcm_mbytes_per_s_delta": round((view["abi"].get("pcm_mbytes_per_s") or 0) - (view["core"].get("pcm_mbytes_per_s") or 0), 1),
    }
    return view

def startup_view(run_map):
    labels = sorted(run_map)
    out = {}
    for key in ("abi", "core"):
        docs = [run_map[label]["startup_" + key] for label in labels]
        entry = {}
        for part in ("open", "probe", "first_read", "ttfp", "decoder_open", "first_frame"):
            present = [d[part] for d in docs if part in d]
            if not present:
                continue
            entry[part] = {
                "median_us": round(median([d[part]["p50_us"] for d in present]), 2),
                "p95_us": round(median([d[part]["p95_us"] for d in present]), 2),
                "max_us": round(median([d[part]["max_us"] for d in present]), 2),
            }
        if key == "abi":
            entry["first_buffer_silent_count"] = median([d.get("first_buffer_silent_count") for d in docs])
        out[key] = entry
    return out

def sweep_view(run_map):
    labels = sorted(run_map)
    first = run_map[labels[0]]
    sweep = first.get("block_sweep")
    if not sweep:
        return None
    audio_s = median([r["audio_seconds"] for r in run_map.values()])
    docs = [run_map[label]["block_sweep"] for label in labels]
    out = []
    for blk in sweep["per_block"]:
        block = blk["block_frames"]
        runs_blk = [b for doc in docs for b in doc["per_block"] if b["block_frames"] == block]
        if not runs_blk:
            continue
        read_calls = [b["read_call"] for b in runs_blk]
        entry = {
            "block_frames": block,
            "calls_per_pass": median([b["calls_per_pass"] for b in runs_blk]),
            "frames_per_call": round(median([b["frames_per_pass"] / b["calls_per_pass"] for b in runs_blk]), 1),
            "batch_wall_us": median([b["batch_wall_us"] for b in runs_blk]),
            "batch_mean_call_us": median([b["batch_mean_call_us"] for b in runs_blk]),
        }
        for key in LAT_KEYS:
            vals = [rc[key] for rc in read_calls if rc.get(key) is not None]
            entry[key] = median(vals)
        if audio_s and entry["batch_wall_us"] > 0:
            entry["batch_x_realtime"] = round(audio_s / (entry["batch_wall_us"] / 1e6), 1)
            entry["batch_pcm_mbytes_per_s"] = round(median([b["batch_frames"] for b in runs_blk]) * sweep["channels"] * 4 / (entry["batch_wall_us"] / 1e6) / 1e6, 1)
        out.append(entry)
    return out

def cmd_commit(args):
    labels = [s.strip() for s in args.runs.split(",") if s.strip()]
    runs = []
    for label in labels:
        p = RUNS_DIR / label / "run.json"
        if not p.is_file():
            raise SystemExit(f"missing run: {p}")
        runs.append(json.loads(p.read_text()))
    heads = {r["provenance"]["source"]["git_head"] for r in runs}
    if len(heads) != 1:
        raise SystemExit(f"runs span multiple source commits: {heads}")
    file_ids = [[f["id"] for f in r["files"]] for r in runs]
    if any(set(fids) != set(file_ids[0]) for fids in file_ids):
        raise SystemExit("runs cover different corpora")
    by_file = {}
    for r in runs:
        for rec in r["files"]:
            by_file.setdefault(rec["id"], {"record": rec, "runs": {}})["runs"][r["label"]] = rec
    excluded = {}
    for fid, bundle in sorted(by_file.items()):
        if bundle["record"].get("excluded"):
            labels_hit = sorted(l for l, rec in bundle["runs"].items() if rec.get("excluded"))
            excluded[fid] = {
                "reason": bundle["record"]["excluded"]["reason"],
                "detail": bundle["record"]["excluded"]["detail"],
                "runs_excluded": labels_hit,
                "terminal": bundle["record"]["verify"]["abi"]["terminal"],
                "frames_decoded_before_failure": bundle["record"]["verify"]["abi"]["frames"],
                "duration_us": bundle["record"]["verify"]["abi"]["duration_us"],
            }
    for fid in excluded:
        del by_file[fid]

    throughput = {}
    startup = {}
    read_latency = {}
    sweeps = {}
    correctness = {}
    tp_cv = {"core": [], "abi": []}
    ttfp_cvs = []
    p99_cvs = []
    max_cvs = []
    for fid, bundle in sorted(by_file.items()):
        rec = bundle["record"]
        run_map = bundle["runs"]
        labels = sorted(run_map)
        tp_view = throughput_view(run_map)
        for layer in ("core", "abi"):
            c = tp_view[layer]["run_cv"]
            tp_cv[layer].append(c)
        throughput[fid] = tp_view

        st_view = startup_view(run_map)
        ttfp_vals = [run_map[label]["startup_abi"]["ttfp"]["p50_us"] for label in labels]
        st_view["abi"]["ttfp_p50_per_run_us"] = {label: round(v, 1) for label, v in zip(labels, ttfp_vals)}
        c = cv(ttfp_vals)
        st_view["abi"]["ttfp_run_cv"] = round(c, 4) if c is not None else None
        ttfp_cvs.append(c)
        startup[fid] = st_view

        lat_by_block = {}
        lat_docs = [run_map[label]["read_latency_main"] for label in labels]
        for blk in lat_docs[0]["per_block"]:
            block = blk["block_frames"]
            blocks_runs = []
            for doc in lat_docs:
                match = [b for b in doc["per_block"] if b["block_frames"] == block]
                blocks_runs.append(match[0])
            lat_by_block[str(block)] = agg_lat(blocks_runs)
            p99_vals = [b["read_call"]["p99_us"] for b in blocks_runs]
            max_vals = [b["read_call"]["max_us"] for b in blocks_runs]
            cp99 = cv(p99_vals)
            cmax = cv(max_vals)
            lat_by_block[str(block)]["p99_run_cv"] = round(cp99, 4) if cp99 is not None else None
            lat_by_block[str(block)]["max_run_cv"] = round(cmax, 4) if cmax is not None else None
            p99_cvs.append(cp99)
            max_cvs.append(cmax)
        read_latency[fid] = lat_by_block

        sw = sweep_view(run_map)
        if sw:
            sweeps[fid] = sw

        correctness[fid] = {
            "verify_ok": rec["verify"]["status"] == "ok",
            "abi_frames_eq_core_samples": rec["verify"]["abi"]["abi_frames_eq_core_samples"],
            "pcm_sha256": rec["verify"]["abi"]["pcm_sha256"],
            "sha_matches_independent_dump": rec["gate_sha_vs_independent_dump"],
            "sha_matches_qnbench_oracle": rec["gate_sha_vs_qnbench_oracle"],
            "fixture_sha_matches_manifest": rec["fixture_sha_matches_manifest"],
        }

    repeatability = {
        "throughput_median_wall_cv_by_file": {
            "core": {fid: round(c, 4) for fid, c in zip(sorted(by_file), tp_cv["core"]) if c is not None},
            "abi": {fid: round(c, 4) for fid, c in zip(sorted(by_file), tp_cv["abi"]) if c is not None},
        },
        "ttfp_p50_cv_by_file": {fid: round(c, 4) for fid, c in zip(sorted(by_file), ttfp_cvs) if c is not None},
        "read_p99_cv_by_file_main_block": {fid: round(c, 4) for fid, c in zip(sorted(by_file), p99_cvs) if c is not None},
        "read_max_cv_by_file_main_block": {fid: round(c, 4) for fid, c in zip(sorted(by_file), max_cvs) if c is not None},
    }

    corpus = []
    for fid, path, role in sorted(MATRIX, key=lambda m: m[0]):
        if fid not in by_file:
            continue
        rec = by_file[fid]["record"]
        corpus.append({
            "id": fid,
            "file": rec["path"],
            "role": role,
            "sha256": rec["fixture_sha256"],
            "duration_s": round(rec["audio_seconds"], 3),
            "codec": rec["verify"]["abi"]["codec"],
            "container": rec["verify"]["abi"]["container"],
            "sample_rate": rec["verify"]["abi"]["sample_rate"],
            "channels": rec["verify"]["abi"]["channels"],
        })

    all_gates_ok = all(
        c["verify_ok"] and c["abi_frames_eq_core_samples"] and c["sha_matches_independent_dump"]
        and c["sha_matches_qnbench_oracle"] is not False
        and c["fixture_sha_matches_manifest"] is not False
        for c in correctness.values()
    )
    doc = {
        "schema": "native-decode-performance/1",
        "generated_utc": now_utc(),
        "runs": labels,
        "source": runs[0]["provenance"]["source"],
        "ffmpeg": runs[0]["provenance"]["ffmpeg"],
        "compiler": runs[0]["provenance"]["compiler"],
        "host": runs[0]["provenance"]["host"],
        "measurement_layers": {
            "B0": "native decode core: direct libavformat/libavcodec demux+decode over the same replayed FFmpeg closure inside libsongcore.a; canonical conversion absent, output discarded; timed window = read/decode loop only",
            "B1": "SongCore public C ABI: song_open -> song_probe -> song_read_pcm loop (canonical Float32 interleaved at source rate/layout into caller buffer) -> song_close; timed window = read loop only",
            "layer_labels_note": "B0/B1 are report-level analysis labels; they do not appear in code identifiers",
        },
        "run_protocol": runs[0]["protocol"],
        "timer_baseline_median": {
            "clock_pair_us": median([r["timer_baseline"]["clock_pair"]["p50_us"] for r in runs]),
            "abi_version_call_us": median([r["timer_baseline"]["abi_version_call"]["p50_us"] for r in runs]),
        },
        "corpus": corpus,
        "corpus_excluded": excluded,
        "throughput": throughput,
        "startup": startup,
        "read_latency": read_latency,
        "block_sweep": sweeps,
        "repeatability": repeatability,
        "correctness": correctness,
        "correctness_verdict": "PASS" if all_gates_ok else "FAIL",
    }
    COMMITTED_DIR.mkdir(parents=True, exist_ok=True)
    out_path = Path(args.out) if args.out else COMMITTED_DIR / "native-decode-performance.json"
    out_path.write_text(json.dumps(doc, indent=2, sort_keys=True) + "\n")
    print(f"wrote {out_path.relative_to(ROOT)}")
    print(f"correctness_verdict: {doc['correctness_verdict']}")
    return 0

def main():
    parser = argparse.ArgumentParser(description="native decode performance baseline driver")
    sub = parser.add_subparsers(dest="command", required=True)
    p_run = sub.add_parser("run", help="execute one full measurement run")
    p_run.add_argument("--label", required=True)
    p_run.add_argument("--seed", type=int, default=None)
    p_run.add_argument("--cpu", type=int, default=2)
    p_run.add_argument("--probe", default=str(PROBE_DEFAULT))
    p_run.set_defaults(func=cmd_run)
    p_commit = sub.add_parser("commit", help="aggregate runs into the committed baseline summary")
    p_commit.add_argument("--runs", required=True)
    p_commit.add_argument("--out", default=None)
    p_commit.set_defaults(func=cmd_commit)
    args = parser.parse_args()
    if args.command == "run" and args.seed is None:
        args.seed = int.from_bytes(os.urandom(4), "little")
    sys.exit(args.func(args))

if __name__ == "__main__":
    main()
