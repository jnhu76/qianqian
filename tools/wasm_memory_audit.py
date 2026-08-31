#!/usr/bin/env python3
"""E09 memory audit driver (Layer 1).

Per runtime x fixture, runs the runner's `pcm` mode (Mode B + Mode C +
runner peak RSS) while sampling host /proc RSS at ~10 ms. Emits:

  bench/results/wasm/memory.json:
    per runtime/fixture: rss timeline summary (peak, growth steps),
    guest linear-memory pages before/after decode (pcm_host line),
    per-chunk-size call latency (max_call_ms -> long-tail evidence),
    runner peak_rss_kb.

Long-tail pause evidence = max_call_ms vs median call cost; the sampler
also captures RSS sawtooth (allocation spikes) for the report.

Run AFTER the gate (exclusive machine use).
"""

import json
import subprocess
import threading
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
ART = ROOT / "build" / "artifacts"
WASM = ART / "wasm"
OUT = ROOT / "bench" / "results" / "wasm" / "memory.json"

FIXTURES = ["flac-16-44-stereo.flac", "mp3-long.mp3"]

RUNS = [
    ("native", ART / "qn_native_runner", "native"),
    ("wamr", ART / "qn_wamr_runner", WASM / "qn_guest_bench.wasm"),
    ("wamr_aot", ART / "qn_wamr_aot_runner", WASM / "qn_guest_bench.aot"),
    ("wasm3", ART / "qn_wasm3_runner", WASM / "qn_guest_bench.wasm"),
    ("wasmtime", ART / "qn_wasmtime_runner", WASM / "qn_guest_bench.wasm"),
]


def sample_rss(pid, stop, timeline):
    path = Path(f"/proc/{pid}/status")
    while not stop.is_set():
        try:
            txt = path.read_text()
        except FileNotFoundError:
            return
        for line in txt.splitlines():
            if line.startswith("VmRSS:"):
                timeline.append((time.monotonic(), int(line.split()[1])))
                break
        time.sleep(0.01)


def run_pcm(binary, guest, fixture):
    fx = str(ROOT / "corpus" / "fixtures" / fixture)
    cmd = [str(binary)] + ([str(guest)] if "native" not in binary.name else []) + \
        ["pcm", fx]
    timeline = []
    t0 = time.monotonic()
    p = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
                         text=True)
    stop = threading.Event()
    th = threading.Thread(target=sample_rss, args=(p.pid, stop, timeline))
    th.start()
    out, _ = p.communicate(timeout=1800)
    stop.set()
    th.join()
    wall_s = time.monotonic() - t0

    jlines = []
    for line in out.splitlines():
        if line.startswith("{"):
            try:
                jlines.append(json.loads(line))
            except json.JSONDecodeError:
                pass
    rss = [v for _, v in timeline]
    return {
        "wall_s": round(wall_s, 3),
        "json": jlines,
        "rss_peak_kb": max(rss) if rss else None,
        "rss_first_kb": rss[0] if rss else None,
        "rss_samples": len(rss),
        "returncode": p.returncode,
    }


def main():
    out = {}
    for rt, binary, guest in RUNS:
        if not binary.exists():
            out[rt] = {"_err": "runner-missing"}
            continue
        per = {}
        for fx in FIXTURES:
            per[fx] = run_pcm(binary, guest, fx)
            print(f"[{rt}] {fx}: peak_rss={per[fx]['rss_peak_kb']}kB "
                  f"wall={per[fx]['wall_s']}s", flush=True)
        out[rt] = per
        OUT.parent.mkdir(parents=True, exist_ok=True)
        json.dump(out, open(OUT, "w"), indent=1, ensure_ascii=False)
    print("memory.json written")


if __name__ == "__main__":
    main()
