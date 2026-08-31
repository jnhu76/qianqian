#!/usr/bin/env python3
"""Generates the numeric tables of docs/experiments/e09-wasm-total-cost.md
from bench/results/wasm/summary.json (single machine authority).

Prose lives in the report; every number between the GENERATED markers comes
from this script. Run: python3 tools/wasm_report_tables.py
"""

import json
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
S = json.loads((ROOT / "bench/results/wasm/summary.json").read_text())
DOC = ROOT / "docs/experiments/e09-wasm-total-cost.md"

BEGIN = "<!-- BEGIN GENERATED TABLES -->"
END = "<!-- END GENERATED TABLES -->"

FX_LABEL = {
    "flac-16-44-stereo.flac": "flac 16/44.8k stereo (4 s)",
    "mp3-cbr-id3v23.mp3": "mp3 cbr (4 s)",
    "aac-lc-44-stereo.m4a": "aac-lc (12 s)",
    "opus-48-stereo.opus": "opus (12 s)",
    "mp3-long.mp3": "mp3 cbr long (12 s)",
}
RTS = ["native", "wamr", "wamr_aot", "wasm3", "wasmtime"]


def fmt(v, nd=2, scale=1.0):
    if v is None:
        return "—"
    return f"{v * scale:,.{nd}f}"


def gate_table():
    g = S["gate"]["native_vs_runtime"]
    rows = []
    for rt in RTS:
        v = g.get(rt)
        if isinstance(v, tuple):
            rows.append(f"| {rt} twin | {v[0]} / {v[1]} |")
        else:
            rows.append(f"| {rt} | {v} |")
    inter = S["gate"]["wasm_inter_runtime_mismatches"]
    total = S["gate"]["wasm_inter_runtime_total"]
    return "\n".join([
        "| runtime | passed / 44 |",
        "|---|---|",
        *rows,
        f"",
        f"wasm 侧互检（四个 wasm runtime 相互 observable 全等）：**{total - inter} / {total} 一致，{inter} 例分歧**。",
    ])


def ladder_table():
    lad = S["execution_ladder_mode_a"]
    lines = [
        "| runtime | " + " | ".join(FX_LABEL[f] for f in FX_LABEL) + " |",
        "|---|" + "---:|" * len(FX_LABEL),
    ]
    for rt in RTS:
        cells = []
        for f in FX_LABEL:
            e = (lad.get(rt) or {}).get(f)
            cells.append(fmt(e.get("songcore_output_ms")) if e else "—")
        lines.append(f"| {rt} ms | " + " | ".join(cells) + " |")
    for rt in RTS:
        cells = []
        for f in FX_LABEL:
            e = (lad.get(rt) or {}).get(f)
            cells.append(fmt(e.get("execution_tax")) if e and "execution_tax" in e else ("1.00" if rt == "native" and e else "—"))
        lines.append(f"| {rt} tax | " + " | ".join(cells) + " |")
    em = S.get("em_mode_a_node_harness", {}).get("em_mode_a", {})
    if em:
        cells = []
        for f in FX_LABEL:
            b = (em.get(f) or {}).get("bench") or {}
            m = b.get("songcore_output_ms", {}).get("median") if isinstance(b.get("songcore_output_ms"), dict) else None
            cells.append(fmt(m))
        nat = lad["native"]
        cells_tax = []
        for f in FX_LABEL:
            b = (em.get(f) or {}).get("bench") or {}
            m = b.get("songcore_output_ms", {}).get("median") if isinstance(b.get("songcore_output_ms"), dict) else None
            n = nat[f]["songcore_output_ms"]
            cells_tax.append(fmt(m / n) if m else "—")
        lines.append(f"| emscripten ms | " + " | ".join(cells) + " |")
        lines.append(f"| emscripten tax | " + " | ".join(cells_tax) + " |")
    return "\n".join(lines)


def bridge_table():
    b = S["bridge_and_memory"]
    lines = [
        "| runtime | Mode B copy GB/s | Mode C 256fr calls/s | Mode C 4096fr calls/s | Mode C max call ms (4096fr) |",
        "|---|---:|---:|---:|---:|",
    ]
    for rt in RTS:
        row = (b.get(rt) or {}).get("flac-16-44-stereo.flac")
        if not row:
            continue
        c = row.get("mode_c_calls_per_s", {})
        mx = row.get("mode_c_max_call_ms", {})
        lines.append(f"| {rt} | {fmt(row.get('mode_b_gbps'), 2)} | "
                     f"{fmt(c.get('256'), 0)} | {fmt(c.get('4096'), 0)} | "
                     f"{fmt(mx.get('4096'), 3)} |")
    em = S.get("em_mode_a_node_harness", {})
    emb = em.get("em_mode_b", {})
    if emb.get("gbps"):
        lines.append(f"| emscripten | {fmt(emb.get('gbps'), 2)} | "
                     f"{fmt(em.get('em_mode_c', {}).get('256', {}).get('calls_per_s'), 0)} | "
                     f"{fmt(em.get('em_mode_c', {}).get('4096', {}).get('calls_per_s'), 0)} | "
                     f"{fmt(em.get('em_mode_c', {}).get('4096', {}).get('call_ms_p999'), 3)} |")
    return "\n".join(lines)


def memory_table():
    b = S["bridge_and_memory"]
    lines = [
        "| runtime | 线性内存 before→after（pages） | memory.grow 次数 | peak RSS（flac / mp3-long）MB |",
        "|---|---|---|---|",
    ]
    for rt in RTS:
        f1 = (b.get(rt) or {}).get("flac-16-44-stereo.flac")
        f2 = (b.get(rt) or {}).get("mp3-long.mp3")
        if not f1:
            continue
        pages = f"{f1['linear_pages_before']}→{f1['linear_pages_after']}"
        grow = 0 if f1["linear_pages_before"] == f1["linear_pages_after"] else "n>0"
        r1 = f1.get("peak_rss_kb") or 0
        r2 = f2.get("peak_rss_kb") or 0
        lines.append(f"| {rt} | {pages} | {grow} | {r1/1024:,.1f} / {r2/1024:,.1f} |")
    return "\n".join(lines)


def shipping_table():
    sh = S["shipping"]
    w = sh["wasi"]
    em = sh["em"]["guests"]
    lines = [
        "| 交付形态 | raw | gzip -9 | brotli -11 |",
        "|---|---:|---:|---:|",
        f"| WASI guest bench .wasm | {w['guest_wasm']:,} | {w['guest_wasm_gzip9']:,} | {w['guest_wasm_brotli11']:,} |",
        f"| WASI SongCore.wasm（reactor） | {w['songcore_reactor_wasm']:,} | — | — |",
        f"| wamrc AOT artifact（x86_64） | {w['aot_artifact']:,} | — | — |",
    ]
    if em.get("em_guest_bench_wasm", {}).get("bytes"):
        lines.append(
            f"| EM guest bench .wasm（-g1 保符号名） | {em['em_guest_bench_wasm']['bytes']:,} "
            f"| {em['em_guest_bench_wasm']['gzip9']:,} | {em['em_guest_bench_wasm']['brotli11']:,} |")
        lines.append(
            f"| EM glue .js（bench） | {em['em_guest_bench_js']['bytes']:,} "
            f"| {em['em_guest_bench_js']['gzip9']:,} | {em['em_guest_bench_js']['brotli11']:,} |")
    return "\n".join(lines)


def tolerance_table():
    t = S["float_tolerance_vs_native"]["fixtures"]
    lines = [
        "| fixture | max\\|Δ\\| (f32) | 16-bit LSB 折算 | 差异样本占比 |",
        "|---|---:|---:|---:|",
    ]
    for fx, v in t.items():
        pct = 100.0 * v["differing_samples"] / v["samples"]
        lines.append(f"| {fx} | {v['max_abs_delta']:.2e} | "
                     f"{v['max_abs_delta_in_16bit_lsb']:.4f} | {pct:.2f}% |")
    return "\n".join(lines)


def perf_stat_table():
    # perf counters live in performance.json, not summary.json
    perf = json.loads((ROOT / "bench/results/wasm/performance.json").read_text())
    lines = [
        "| runtime | cycles（flac bench×3） | IPC | elapsed ms |",
        "|---|---:|---:|---:|",
    ]
    for rt in RTS:
        ps = perf[rt]["flac-16-44-stereo.flac"].get("perf_stat", {})
        cyc, ins = ps.get("cycles"), ps.get("instructions")
        ipc = round(ins / cyc, 2) if cyc and ins else None
        lines.append(f"| {rt} | {cyc:,} | {ipc} | {ps.get('elapsed_s')*1000:,.0f} |"
                     if cyc and ps.get("elapsed_s") else f"| {rt} | — | — | — |")
    return "\n".join(lines)


def main():
    text = DOC.read_text()
    block = "\n".join([
        BEGIN,
        "### 1. Correctness gate（44-case，native twin 为 reference）",
        "",
        "machine authority：`bench/results/wasm/correctness.json`",
        "",
        gate_table(),
        "",
        "### 3. 执行 ladder（Mode A，execution_tax = T_guest / T_native）",
        "",
        "machine authority：`bench/results/wasm/performance.json`（逐 runtime 5 fixture；"
        "native twin 与 guest 同为 -Os codegen 口径）",
        "",
        ladder_table(),
        "",
        "perf stat（flac，bench×3，含 instantiate）：",
        "",
        perf_stat_table(),
        "",
        "### 4. Bridge / boundary（flac）",
        "",
        "machine authority：`bench/results/wasm/memory.json`（Mode B/C）+ "
        "`em_performance.json`",
        "",
        bridge_table(),
        "",
        "### 6. Memory audit",
        "",
        "machine authority：`bench/results/wasm/memory.json`",
        "",
        memory_table(),
        "",
        "### 2. Shipping footprint",
        "",
        "machine authority：`bench/results/wasm/shipping.json` + `shipping-em.json`",
        "",
        shipping_table(),
        "",
        "### float 容差（native vs wasm，剩余分歧全量解释）",
        "",
        "machine authority：`bench/results/wasm/tolerance.json`",
        "",
        tolerance_table(),
        "",
        END,
    ])
    if BEGIN in text:
        pre = text[:text.index(BEGIN)]
        post = text[text.index(END) + len(END):]
        text = pre + block + post
    else:
        text = text.rstrip() + "\n\n" + block + "\n"
    DOC.write_text(text)
    print("report tables generated into", DOC)


if __name__ == "__main__":
    main()
