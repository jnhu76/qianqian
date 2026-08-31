#!/usr/bin/env python3
"""E09 performance collection driver (Layer 1).

Collects, per runtime, on a representative fixture subset:

  1. Mode A (guest decode+hash) wall/CPU + the guest's own songcore/decode-core
     split (the bench JSON) -> execution_tax vs native twin.
  2. perf stat counters (cycles, instructions, IPC, context switches, faults).
  3. perf record -> flamegraphcollapsed stacks. Guest-internal attribution is
     timing-based for interpreted runtimes (their host samples collapse into
     the interpreter loop by construction); the native twin gets a true
     function-level flamegraph.

Output: bench/results/wasm/performance.json and
        bench/profiles/wasm-<runtime>-<fixture>.json
        (perf raw: bench/provenance/e09-perf/)

Run AFTER the correctness gate (no concurrent builds or gates).
"""

import json
import re
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
ART = ROOT / "build" / "artifacts"
WASM = ART / "wasm"
OUTDIR = ROOT / "bench" / "results" / "wasm"
PROFDIR = ROOT / "bench" / "profiles"
PERFRAW = ROOT / "bench" / "provenance" / "e09-perf"

# representative subset: one clean per codegen-sensitive family + the
# long-tail file (mp3-long) where interp cost goes superlinear in wall time
FIXTURES = [
    "flac-16-44-stereo.flac",
    "mp3-cbr-id3v23.mp3",
    "aac-lc-44-stereo.m4a",
    "opus-48-stereo.opus",
    "mp3-long.mp3",
]

RUNS = [
    ("native", ART / "qn_native_runner", "native"),
    ("wamr", ART / "qn_wamr_runner", WASM / "qn_guest_bench.wasm"),
    ("wamr_aot", ART / "qn_wamr_aot_runner", WASM / "qn_guest_bench.aot"),
    ("wasm3", ART / "qn_wasm3_runner", WASM / "qn_guest_bench.wasm"),
    ("wasmtime", ART / "qn_wasmtime_runner", WASM / "qn_guest_bench.wasm"),
]

ITERS = "3"


def guest_arg(rt_binary, guest):
    return [str(guest)] if "native" not in rt_binary.name else []


def run_bench(binary, guest, fixture):
    fx = str(ROOT / "corpus" / "fixtures" / fixture)
    if "native" in binary.name:
        cmd = [str(binary), fx, "bench", ITERS]
    else:
        cmd = [str(binary), str(guest), "bench", fx, ITERS]
    p = subprocess.run(cmd, capture_output=True, text=True, timeout=1800)
    lines = [l for l in p.stdout.splitlines() if l.startswith("{")]
    if not lines:
        return {"_err": "no-json", "stderr": p.stderr[-300:]}
    return json.loads(lines[-1])


def perf_stat(binary, guest, fixture):
    PERFRAW.mkdir(parents=True, exist_ok=True)
    statfile = PERFRAW / f"perf_stat_{binary.name}_{fixture}.txt"
    fx = str(ROOT / "corpus" / "fixtures" / fixture)
    if "native" in binary.name:
        tgt = [str(binary), fx, "bench", ITERS]
    else:
        tgt = [str(binary), str(guest), "bench", fx, ITERS]
    cmd = ["perf", "stat", "-o", str(statfile), "--"] + tgt
    subprocess.run(cmd, capture_output=True, text=True, timeout=1800)
    counters = {}
    txt = statfile.read_text() if statfile.exists() else ""
    EVENT_KEYS = {
        "cpu-cycles": "cycles", "instructions": "instructions",
        "context-switches": "context-switches", "cpu-migrations": "cpu-migrations",
        "page-faults": "page-faults", "branches": "branches",
        "branch-misses": "branch-misses", "task-clock": "task-clock",
    }
    for line in txt.splitlines():
        parts = line.split()
        if len(parts) < 2:
            continue
        ev = parts[1].split(":")[0]
        if ev in EVENT_KEYS:
            try:
                counters[EVENT_KEYS[ev]] = int(float(parts[0].replace(",", "")))
            except ValueError:
                pass
        elif len(parts) >= 4 and parts[1] == "seconds" and parts[2] == "time":
            counters["elapsed_s"] = float(parts[0])
        elif len(parts) >= 3 and parts[1] == "seconds" and parts[2] in ("user", "sys"):
            counters["user_s" if parts[2] == "user" else "sys_s"] = float(parts[0])
    return counters


def perf_record(binary, guest, fixture, tag, seconds=30):
    PERFRAW.mkdir(parents=True, exist_ok=True)
    data = PERFRAW / f"{tag}.data"
    fx = str(ROOT / "corpus" / "fixtures" / fixture)
    if "native" in binary.name:
        tgt = [str(binary), fx, "bench", ITERS]
    else:
        tgt = [str(binary), str(guest), "bench", fx, ITERS]
    cmd = ["perf", "record", "-F", "999", "-g", "-o", str(data), "--"] + tgt
    try:
        subprocess.run(cmd, capture_output=True, timeout=seconds * 4)
    except subprocess.TimeoutExpired:
        pass
    if not data.exists():
        return None, None
    txt = subprocess.run(["perf", "report", "-i", str(data), "--stdio",
                          "--no-children"], capture_output=True, text=True)
    collapsed = subprocess.run(
        ["bash", "-c",
         f"perf script -i {data} | $HOME/toolchains/e09/flamegraph/stackcollapse-perf.pl 2>/dev/null || true"],
        capture_output=True, text=True)
    return txt.stdout, collapsed.stdout or None


def main():
    out = {}
    OUTDIR.mkdir(parents=True, exist_ok=True)
    PROFDIR.mkdir(parents=True, exist_ok=True)
    for rt, binary, guest in RUNS:
        if not binary.exists():
            out[rt] = {"_err": "runner-missing"}
            continue
        per = {}
        for fx in FIXTURES:
            e = {}
            e["bench"] = run_bench(binary, guest, fx)
            e["perf_stat"] = perf_stat(binary, guest, fx)
            per[fx] = e
            print(f"[{rt}] {fx}: bench_ok={('_err' not in e['bench'])}", flush=True)
        # one flamegraph per runtime on the flac reference fixture
        rep, collapsed = perf_record(binary, guest, "flac-16-44-stereo.flac",
                                     f"wasm-{rt}-flac16")
        (PROFDIR / f"wasm-{rt}-flac16.hot.txt").write_text(rep or "")
        if collapsed:
            (PERFRAW / f"wasm-{rt}-flac16.folded").write_text(collapsed)
        out[rt] = per
        json.dump(out, open(OUTDIR / "performance.json", "w"),
                  indent=1, ensure_ascii=False)
        print(f"[{rt}] done", flush=True)
    print("performance.json written")


if __name__ == "__main__":
    main()
