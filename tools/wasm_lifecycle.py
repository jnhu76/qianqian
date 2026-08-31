#!/usr/bin/env python3
"""E09 lifecycle timing collector (startup is not whole-process elapsed).

Promotes a uniform per-backend stage schema into machine authority:

  process/load -> compile/JIT (if any) -> instantiate -> first open
  -> first PCM -> steady decode

- load_ms / compile_ms / instantiate_ms come from the runner's
  lifecycle_host JSON (native: null for all host stages);
- open_ms / first_pcm_ms / decode_ms come from the guest's lifecycle JSON
  (identical guest code across every backend).

This replaces the old practice of reading `perf stat elapsed` (process
start + load + instantiate + bench x3) as if it were "startup".

Emits bench/results/wasm/lifecycle.json.
"""

import json
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
ART = ROOT / "build" / "artifacts"
WASM = ART / "wasm"
OUT = ROOT / "bench" / "results" / "wasm" / "lifecycle.json"

FIXTURES = ["flac-16-44-stereo.flac", "mp3-long.mp3"]

RUNS = [
    ("native", ART / "qn_native_runner", None),
    ("wamr", ART / "qn_wamr_runner", WASM / "qn_guest_bench.wamr-workaround.wasm"),
    ("wamr_aot", ART / "qn_wamr_aot_runner", WASM / "qn_guest_bench.aot"),
    ("wasm3", ART / "qn_wasm3_runner", WASM / "qn_guest_bench.wasm"),
    ("wasmtime", ART / "qn_wasmtime_runner", WASM / "qn_guest_bench.wasm"),
]

HARNESS = ROOT / "tools" / "wasm_em_node_harness.mjs"


def run_c(binary, guest, fixture):
    fx = str(ROOT / "corpus" / "fixtures" / fixture)
    if guest is None:
        cmd = [str(binary), fx, "lifecycle"]
    else:
        cmd = [str(binary), str(guest), "lifecycle", fx]
    p = subprocess.run(cmd, capture_output=True, text=True, timeout=1800)
    host, guest_j = None, None
    for line in p.stdout.splitlines():
        if not line.startswith("{"):
            continue
        try:
            j = json.loads(line)
        except json.JSONDecodeError:
            continue
        if j.get("mode") == "lifecycle_host":
            host = j
        elif j.get("mode") == "lifecycle":
            guest_j = j
    if p.returncode not in (0, 1) or guest_j is None:
        return {"_err": f"rc={p.returncode} stderr={p.stderr[-200:]}"}
    return {"host": host, "guest": guest_j}


def run_em(fixture):
    fx = str(ROOT / "corpus" / "fixtures" / fixture)
    p = subprocess.run(["node", str(HARNESS), "bench", "lifecycle", fx],
                       capture_output=True, text=True, timeout=1800)
    for line in p.stdout.splitlines():
        if not line.startswith("{"):
            continue
        try:
            j = json.loads(line)
        except json.JSONDecodeError:
            continue
        if j.get("mode") == "lifecycle":
            return {"host": j.get("host"), "guest": j.get("guest")}
    return {"_err": f"rc={p.returncode} stderr={p.stderr[-200:]}"}


def main():
    out = {"schema": "load -> compile(JIT if any) -> instantiate -> first open "
                     "-> first PCM -> steady decode; host stages from the "
                     "runner, guest stages from the same guest code",
           "fixtures": FIXTURES, "runtimes": {}}
    for rt, binary, guest in RUNS:
        if not binary.exists():
            out["runtimes"][rt] = {"_err": "runner-missing"}
            continue
        out["runtimes"][rt] = {fx: run_c(binary, guest, fx) for fx in FIXTURES}
        print(f"[{rt}] done", flush=True)
    # The Emscripten row is collected in the em session
    # (tools/wasm_em_perf.py -> em_performance.json["em_lifecycle"]) because
    # the em guest artifacts overwrite the shared WASI ones (E09-xmake-1);
    # wasm_summary.py joins the two authorities.

    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(json.dumps(out, indent=1, ensure_ascii=False))
    print("lifecycle.json written")


if __name__ == "__main__":
    main()
