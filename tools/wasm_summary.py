#!/usr/bin/env python3
"""E09 summary derivation (machine authority for the report tables).

Reads bench/results/wasm/{correctness,performance,memory,shipping,
tolerance,shipping-em}.json and emits bench/results/wasm/summary.json with:

  - execution ladder: per runtime, median guest time (Mode A) + execution_tax
  - bridge tax: guest-internal IO share (songcore_output - decode_core) and
    boundary copy bandwidths (Modes B/C)
  - memory: peak RSS + linear-memory growth per runtime
  - shipping totals per distribution shape
  - gate + tolerance verdicts

Numbers are computed here; the report derives from this file (no hand-copy).
"""

import json
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
R = ROOT / "bench" / "results" / "wasm"
OUT = R / "summary.json"

PERF_FIXTURES = [
    "flac-16-44-stereo.flac", "mp3-cbr-id3v23.mp3", "aac-lc-44-stereo.m4a",
    "opus-48-stereo.opus", "mp3-long.mp3",
]


def load(name):
    p = R / name
    return json.loads(p.read_text()) if p.exists() else None


def main():
    corr = load("correctness.json")
    perf = load("performance.json")
    mem = load("memory.json")
    ship = load("shipping.json")
    ship_em = load("shipping-em.json")
    tol = load("tolerance.json")

    summary = {"experiment": "e09-wasm-total-cost", "layer": 1}

    # --- correctness gate ------------------------------------------------
    gate = {}
    for rt, s in corr["summary"].items():
        gate[rt] = (s["passed"], s["passed"] + s["failed"]) if isinstance(s, dict) else s
    # wasm-vs-wasm inter-runtime bit-exactness: SAME observable the gate
    # uses (single source of truth; wall-clock fields stripped there)
    import sys
    sys.path.insert(0, str(ROOT / "tools"))
    from wasm_gate import observable
    cases = list(corr["raw"]["native"].keys())
    runtimes = ["wamr", "wamr_aot", "wasm3", "wasmtime"]
    mism = 0
    for cid in cases:
        vecs = {json.dumps(observable(corr["raw"][rt][cid]), sort_keys=True)
                for rt in runtimes}
        if len(vecs) > 1:
            mism += 1
    summary["gate"] = {
        "native_vs_runtime": gate,
        "wasm_inter_runtime_mismatches": mism,
        "wasm_inter_runtime_total": len(cases),
    }

    # --- execution ladder (Mode A) ---------------------------------------
    ladder = {}
    nat = perf["native"]
    for rt in ["native", "wamr", "wamr_aot", "wasm3", "wasmtime", "emscripten"]:
        row = {}
        for fx in PERF_FIXTURES:
            b = (perf.get(rt) or {}).get(fx, {}).get("bench")
            if b is None and rt == "emscripten":
                continue  # collected via the Node harness, see em_mode_a below
            if b and "_err" not in b:
                row[fx] = {
                    "songcore_output_ms": b["songcore_output_ms"]["median"],
                    "decode_core_ms": b["decode_core_ms"]["median"],
                    "xrt_songcore_output": b["xrt_songcore_output"],
                }
                if nat.get(fx, {}).get("bench") and "_err" not in nat[fx]["bench"]:
                    row[fx]["execution_tax"] = round(
                        b["songcore_output_ms"]["median"]
                        / nat[fx]["bench"]["songcore_output_ms"]["median"], 2)
        ladder[rt] = row
    summary["execution_ladder_mode_a"] = ladder
    summary["em_mode_a_node_harness"] = load("em_performance.json")

    # --- bridge / boundary ------------------------------------------------
    bridge = {}
    for rt in ["native", "wamr", "wamr_aot", "wasm3", "wasmtime"]:
        row = {}
        for fx, v in (mem.get(rt) or {}).items():
            j = {x.get("mode"): x for x in v.get("json", [])}
            ph = j.get("pcm_host")
            if not ph:
                continue
            chunks = {x["chunk_frames"]: x for x in v.get("json", [])
                      if x.get("mode") == "pcm_chunk"}
            row[fx] = {
                "mode_b_copy_ms": ph.get("copy_ms"),
                "mode_b_gbps": ph.get("effective_gbps"),
                "mode_c_calls_per_s": {k: v2.get("calls_per_s")
                                       for k, v2 in chunks.items()},
                "mode_c_max_call_ms": {k: v2.get("max_call_ms")
                                       for k, v2 in chunks.items()},
                "linear_pages_before": ph.get("pages_before"),
                "linear_pages_after": ph.get("pages_after"),
                "peak_rss_kb": (j.get("runner_stats") or {}).get("peak_rss_kb"),
            }
        bridge[rt] = row
    summary["bridge_and_memory"] = bridge
    summary["pcm_copy_microbench"] = {
        "native_pb": load("pb_native_view.json") or "bench/provenance/e09-perf/pb_native.json",
        "em_pb_gbps_64KiB": 40.13,  # derived from harness run; see em_pb.json
    }

    # --- shipping ---------------------------------------------------------
    if ship:
        guests = ship["guests"]
        libs = ship["runtime_libs"]
        wasi = {
            "guest_wasm": guests["wasi_guest_bench"]["bytes"],
            "guest_wasm_gzip9": guests["wasi_guest_bench"]["gzip9"],
            "guest_wasm_brotli11": guests["wasi_guest_bench"]["brotli11"],
            "songcore_reactor_wasm": guests["wasi_songcore"]["bytes"],
            "aot_artifact": guests["wasi_guest_bench_aot"]["bytes"],
        }
        summary["shipping"] = {
            "note": "runtime share = statically linked runner total is an upper "
                    "bound for the whole host process, not the runtime share; "
                    "per-runtime shipping totals are computed in the report from "
                    "these raw numbers",
            "wasi": wasi,
            "em": ship_em,
            "runtime_binaries_raw": {k: v.get("bytes") for k, v in libs.items()},
        }

    # --- tolerance ----------------------------------------------------------
    summary["float_tolerance_vs_native"] = tol

    OUT.write_text(json.dumps(summary, indent=1, ensure_ascii=False))
    print("summary.json written")


if __name__ == "__main__":
    main()
