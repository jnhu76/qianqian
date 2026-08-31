#!/usr/bin/env python3
"""E09 summary derivation (single machine authority for the report tables).

Reads every bench/results/wasm/*.json and emits summary.json containing
EVERY datum the report tables render — including the perf-stat counters and
the Emscripten rows — so the report derives from this one file and nothing
else (no hardcoded numbers: the em pb bandwidth is read from
em_performance.json, never written as a literal).

Derivations here:
  - gate: per-runtime exact / accepted-with-tolerance / rejected buckets +
    wasm inter-runtime bit-exactness over every wasm row present (incl. the
    joined Emscripten/V8 authority correctness-em.json)
  - execution ladder (Mode A) + execution_tax vs native
  - perf stat (flac) promoted into summary so the report stops re-reading
    performance.json
  - bridge tax PER BACKEND AND FIXTURE: bridge_tax = Mode B transfer cost /
    guest decode cost (the slow interpreter is NOT the universal denominator)
  - memory: linear pages + peak RSS per runtime (native row now valid)
  - lifecycle stages (load/compile/instantiate/first open/first PCM/steady)
  - shipping: product-shaped deployment sets (AOT replaces .wasm)
  - tolerance policy + per-fixture evidence
"""

import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
R = ROOT / "bench" / "results" / "wasm"
OUT = R / "summary.json"

PERF_FIXTURES = [
    "flac-16-44-stereo.flac", "mp3-cbr-id3v23.mp3", "aac-lc-44-stereo.m4a",
    "opus-48-stereo.opus", "mp3-long.mp3",
]
RTS = ["native", "wamr", "wamr_aot", "wasm3", "wasmtime"]
WASM_RTS = ["wamr", "wamr_aot", "wasm3", "wasmtime"]


def load(name):
    p = R / name
    return json.loads(p.read_text()) if p.exists() else None


def main():
    corr = load("correctness.json")
    corr_em = load("correctness-em.json")
    perf = load("performance.json")
    mem = load("memory.json")
    ship = load("shipping.json")
    tol = load("tolerance.json")
    lifecycle = load("lifecycle.json")
    em_perf = load("em_performance.json")

    if corr is None:
        print("error: correctness.json missing")
        return 2
    summary = {"experiment": "e09-wasm-total-cost", "layer": 1}

    # --- correctness gate --------------------------------------------------
    gate = {}
    for rt, s in corr["summary"].items():
        if isinstance(s, dict):
            gate[rt] = {k: s.get(k) for k in
                        ("exact_matches", "accepted_with_tolerance",
                         "rejected", "total")}
        else:
            gate[rt] = s
    em_row = None
    if corr_em:
        s = corr_em["summary"].get("emscripten")
        if isinstance(s, dict):
            em_row = {k: s.get(k) for k in
                      ("exact_matches", "accepted_with_tolerance",
                       "rejected", "total")}
            gate["emscripten"] = em_row

    # wasm-vs-wasm inter-runtime bit-exactness over the SAME observable the
    # gate uses (single source of truth; wall-clock fields stripped there).
    sys.path.insert(0, str(ROOT / "tools"))
    from wasm_gate import observable  # noqa: E402

    wasm_rows = {rt: None for rt in WASM_RTS}
    if corr_em and "raw" in corr_em and "emscripten" in corr_em["raw"]:
        wasm_rows["emscripten"] = corr_em["raw"]["emscripten"]
    cases = list(corr["raw"]["native"].keys())
    mism = 0
    per_case = {}
    for cid in cases:
        vecs = set()
        for rt in wasm_rows:
            j = corr["raw"].get(rt, {}).get(cid)
            if rt == "emscripten":
                j = wasm_rows["emscripten"].get(cid)
            vecs.add(json.dumps(observable(j), sort_keys=True))
        if len(vecs) > 1:
            mism += 1
            per_case[cid] = len(vecs)
    summary["gate"] = {
        "policy": corr.get("policy"),
        "native_vs_runtime": gate,
        "emscripten_authority": "correctness-em.json (node harness)" if corr_em
                                else "absent",
        "wasm_inter_runtime_mismatches": mism,
        "wasm_inter_runtime_total": len(cases),
        "wasm_inter_runtime_runtimes": sorted(wasm_rows),
        "wasm_inter_runtime_mismatch_cases": per_case,
    }

    # --- execution ladder (Mode A) -----------------------------------------
    ladder = {}
    nat = perf["native"]
    for rt in ["native", "wamr", "wamr_aot", "wasm3", "wasmtime"]:
        row = {}
        for fx in PERF_FIXTURES:
            b = (perf.get(rt) or {}).get(fx, {}).get("bench")
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
    summary["em_mode_a_node_harness"] = em_perf if em_perf else {}

    # --- perf stat (flac) promoted into summary -----------------------------
    perf_stat = {}
    for rt in RTS:
        ps = (perf.get(rt) or {}).get("flac-16-44-stereo.flac", {}).get("perf_stat", {})
        cyc, ins = ps.get("cycles"), ps.get("instructions")
        perf_stat[rt] = {
            "cycles": cyc,
            "instructions": ins,
            "ipc": round(ins / cyc, 2) if cyc and ins else None,
            "elapsed_ms": round(ps.get("elapsed_s") * 1000, 1) if ps.get("elapsed_s") else None,
            "user_s": ps.get("user_s"),
            "sys_s": ps.get("sys_s"),
            "context_switches": ps.get("context-switches"),
            "page_faults": ps.get("page-faults"),
            "note": "whole-process elapsed (load+instantiate+bench x3); "
                    "startup stages are measured separately in `lifecycle`",
        }
    summary["perf_stat_flac"] = perf_stat

    # --- bridge / boundary + memory, bridge_tax per backend -----------------
    bridge = {}
    for rt in RTS:
        row = {}
        for fx, v in (mem.get(rt) or {}).items():
            if not isinstance(v, dict) or v.get("_err"):
                continue
            j = {x.get("mode"): x for x in v.get("json", [])}
            ph = j.get("pcm_host")
            if not ph:
                continue
            chunks = {x["chunk_frames"]: x for x in v.get("json", [])
                      if x.get("mode") == "pcm_chunk"}
            entry = {
                "mode_b_copy_ms": ph.get("copy_ms"),
                "mode_b_gbps": ph.get("effective_gbps"),
                "mode_c_calls_per_s": {k: v2.get("calls_per_s")
                                       for k, v2 in chunks.items()},
                "mode_c_max_call_ms": {k: v2.get("max_call_ms")
                                       for k, v2 in chunks.items()},
                "linear_pages_before": ph.get("pages_before"),
                "linear_pages_after": ph.get("pages_after"),
                "peak_rss_kb": (j.get("runner_stats") or {}).get("peak_rss_kb"),
                "rss_peak_sampled_kb": v.get("rss_peak_kb"),
            }
            # bridge_tax = transfer cost / guest decode cost (per backend!)
            g = (ladder.get(rt) or {}).get(fx, {}).get("songcore_output_ms")
            entry["guest_decode_ms"] = g
            if ph.get("copy_ms") is not None and g:
                entry["bridge_tax"] = round(ph["copy_ms"] / g, 4)
            else:
                entry["bridge_tax"] = None
            row[fx] = entry
        bridge[rt] = row
    summary["bridge_and_memory"] = bridge

    # --- em bridge: comparable explicit-copy cost vs em guest decode --------
    em = {}
    if em_perf and em_perf.get("em_mode_b"):
        mb = em_perf["em_mode_b"]
        copy_ms = mb.get("copyMs")
        em["mode_b"] = mb
        g = None
        for fx in PERF_FIXTURES:
            b = (em_perf.get("em_mode_a") or {}).get(fx, {}).get("bench") or {}
            if isinstance(b.get("songcore_output_ms"), dict):
                g = b["songcore_output_ms"]["median"]
                break
        em["bridge_tax_vs_em_guest"] = round(copy_ms / g, 4) if copy_ms and g else None
    if em_perf and em_perf.get("em_pb"):
        em["em_pb"] = em_perf["em_pb"]  # single authority for the pb number
    summary["em_bridge"] = em

    # --- lifecycle ----------------------------------------------------------
    if lifecycle:
        lc = dict(lifecycle)
        lc.setdefault("runtimes", {})
        # the em row is its own authority (collected in the em session)
        if em_perf and em_perf.get("em_lifecycle"):
            lc["runtimes"]["emscripten"] = em_perf["em_lifecycle"]
        summary["lifecycle"] = lc

    # --- shipping (product-shaped deployments) ------------------------------
    if ship:
        summary["shipping"] = {
            "calibers": ship.get("calibers"),
            "deployments": ship.get("deployments"),
            "artifacts": ship.get("artifacts"),
            "runtime_libs": ship.get("runtime_libs"),
        }

    # --- tolerance ----------------------------------------------------------
    if tol:
        summary["float_tolerance_vs_native"] = tol.get("fixtures", tol)
        summary["float_tolerance_policy"] = {
            "families": (corr.get("policy") or {}).get("tolerance_allowed",
                                                       {}).get("families"),
            "gate_bound": None,
        }
        bnd = (corr.get("policy") or {}).get("tolerance_allowed", {})
        # bound lives in the gate policy string; keep the numeric constant in
        # sync by reading it back out of the gate's policy text
        import re
        m = re.search(r"max_abs_delta\s*<=\s*([0-9.e-]+)", json.dumps(bnd))
        if m:
            summary["float_tolerance_policy"]["gate_bound"] = float(m.group(1))

    OUT.write_text(json.dumps(summary, indent=1, ensure_ascii=False))
    print("summary.json written")
    return 0


if __name__ == "__main__":
    sys.exit(main())
