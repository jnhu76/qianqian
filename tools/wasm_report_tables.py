#!/usr/bin/env python3
"""Generates the numeric tables of docs/experiments/e09-wasm-total-cost.md
from bench/results/wasm/summary.json — the SINGLE machine authority.

Every number between the GENERATED markers comes from summary.json; this
script never reads performance.json/memory.json/etc. directly.

  python3 tools/wasm_report_tables.py            (regenerate tables in place)
  python3 tools/wasm_report_tables.py --check    (exit 1 if tables drifted
                                                  from summary.json)
"""

import json
import sys
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
    for rt in RTS + (["emscripten"] if "emscripten" in g else []):
        v = g.get(rt)
        if isinstance(v, dict):
            rows.append(f"| {rt} | {v['exact_matches']} | "
                        f"{v['accepted_with_tolerance']} | {v['rejected']} |")
        else:
            rows.append(f"| {rt} | {v} | — | — |")
    inter = S["gate"]["wasm_inter_runtime_mismatches"]
    total = S["gate"]["wasm_inter_runtime_total"]
    rts = ", ".join(S["gate"].get("wasm_inter_runtime_runtimes", []))
    return "\n".join([
        "| runtime | exact | accepted\\_with\\_tolerance | rejected |",
        "|---|---:|---:|---:|",
        *rows,
        "",
        f"wasm 侧互检（{rts} 相互 observable 全等）：**{total - inter} / {total} "
        f"一致，{inter} 例分歧**。",
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
            cells.append(fmt(e.get("execution_tax")) if e and "execution_tax" in e
                         else ("1.00" if rt == "native" and e else "—"))
        lines.append(f"| {rt} tax | " + " | ".join(cells) + " |")
    em = S.get("em_mode_a_node_harness", {}).get("em_mode_a", {})
    if em:
        cells = []
        for f in FX_LABEL:
            b = (em.get(f) or {}).get("bench") or {}
            m = b.get("songcore_output_ms", {}).get("median") if isinstance(
                b.get("songcore_output_ms"), dict) else None
            cells.append(fmt(m))
        nat = lad["native"]
        cells_tax = []
        for f in FX_LABEL:
            b = (em.get(f) or {}).get("bench") or {}
            m = b.get("songcore_output_ms", {}).get("median") if isinstance(
                b.get("songcore_output_ms"), dict) else None
            n = nat[f]["songcore_output_ms"]
            cells_tax.append(fmt(m / n) if m else "—")
        lines.append(f"| emscripten ms | " + " | ".join(cells) + " |")
        lines.append(f"| emscripten tax | " + " | ".join(cells_tax) + " |")
    return "\n".join(lines)


def perf_stat_table():
    ps = S["perf_stat_flac"]
    lines = [
        "| runtime | cycles（flac bench×3） | IPC | elapsed ms（整进程） |",
        "|---|---:|---:|---:|",
    ]
    for rt in RTS:
        v = ps.get(rt) or {}
        cyc = v.get("cycles")
        lines.append(f"| {rt} | {fmt(cyc, 0)} | {fmt(v.get('ipc'), 2)} | "
                     f"{fmt(v.get('elapsed_ms'), 0)} |")
    return "\n".join(lines)


def bridge_table():
    b = S["bridge_and_memory"]
    lines = [
        "| runtime | Mode B copy GB/s | copy ms | guest decode ms | bridge\\_tax | Mode C 256fr calls/s | Mode C 4096fr calls/s |",
        "|---|---:|---:|---:|---:|---:|---:|",
    ]
    for rt in RTS:
        row = (b.get(rt) or {}).get("flac-16-44-stereo.flac")
        if not row:
            continue
        c = row.get("mode_c_calls_per_s", {})
        lines.append(f"| {rt} | {fmt(row.get('mode_b_gbps'), 2)} | "
                     f"{fmt(row.get('mode_b_copy_ms'), 3)} | "
                     f"{fmt(row.get('guest_decode_ms'), 2)} | "
                     f"{fmt(row.get('bridge_tax'), 3)} | "
                     f"{fmt(c.get('256'), 0)} | {fmt(c.get('4096'), 0)} |")
    em = S.get("em_bridge", {})
    emb = em.get("mode_b")
    if emb and emb.get("copyMs") is not None:
        ema = S.get("em_mode_a_node_harness", {}).get("em_mode_a", {})
        b = (ema.get("flac-16-44-stereo.flac") or {}).get("bench") or {}
        g = b.get("songcore_output_ms", {}).get("median") if isinstance(
            b.get("songcore_output_ms"), dict) else None
        lines.append(
            f"| emscripten（view+copy+hash 分列） | {fmt(emb.get('gbps'), 2)} "
            f"(copy) | {fmt(emb.get('copyMs'), 3)} | {fmt(g, 2)} | "
            f"{fmt(em.get('bridge_tax_vs_em_guest'), 3)} | — | — |")
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
        if f1.get("linear_pages_before") is None:
            pages, grow = "n/a（native 无线性内存）", "—"
        else:
            pages = f"{f1['linear_pages_before']}→{f1['linear_pages_after']}"
            grow = 0 if f1["linear_pages_before"] == f1["linear_pages_after"] else "n>0"
        r1 = f1.get("rss_peak_sampled_kb") or f1.get("peak_rss_kb") or 0
        r2 = f2.get("rss_peak_sampled_kb") or f2.get("peak_rss_kb") or 0
        lines.append(f"| {rt} | {pages} | {grow} | {r1/1024:,.1f} / {r2/1024:,.1f} |")
    return "\n".join(lines)


def shipping_table():
    sh = S["shipping"]
    deps = sh.get("deployments", {})
    lines = [
        "| 交付形态 | host（stripped release） | guest artifact | 合计 raw | 合计 xz -9e | 浏览器口径 gzip/brotli |",
        "|---|---:|---:|---:|---:|---:|",
    ]
    for backend in ("native", "wamr_interp", "wamr_aot", "wasm3", "wasmtime",
                    "browser"):
        d = deps.get(backend)
        if not d or d.get("total_raw") is None:
            continue
        h = d.get("host") or {}
        g = d.get("guest") or {}
        host_s = fmt(h.get("bytes"), 0) if h.get("bytes") else "—"
        guest_s = (f"{g.get('file')} {fmt(g.get('bytes'), 0)}"
                   if g.get("bytes") else "—（native 内置）")
        raw = fmt(d.get("total_raw"), 0)
        xz = fmt(d.get("total_xz9e"), 0)
        if backend == "browser":
            bro = f"{fmt(d.get('total_gzip9'), 0)} / {fmt(d.get('total_brotli11'), 0)}"
        else:
            bro = "—"
        lines.append(f"| {backend} | {host_s} | {guest_s} | {raw} | {xz} | {bro} |")
    art = sh.get("artifacts", {})
    aot = art.get("SongCore.aot") or {}
    wasm_p = art.get("SongCore.wasm") or {}
    lines += [
        "",
        "参考行（单 artifact）：",
        f"- SongCore.wasm（pristine）：raw {fmt(wasm_p.get('bytes'), 0)}，"
        f"gzip -9 {fmt(wasm_p.get('gzip9'), 0)}，brotli -11 "
        f"{fmt(wasm_p.get('brotli11'), 0)}",
        f"- SongCore.aot（product，wamrc from SongCore.wamr-workaround.wasm）："
        f"raw {fmt(aot.get('bytes'), 0)}，xz -9e {fmt(aot.get('xz9e'), 0)}"
        f"（AOT 部署时替代 .wasm，不与 .wasm 相加）",
    ]
    return "\n".join(lines)


def tolerance_table():
    t = S["float_tolerance_vs_native"]
    lines = [
        "| fixture | max\\|Δ\\| (f32) | 16-bit LSB 折算 | 差异样本占比 | samples / frames |",
        "|---|---:|---:|---:|---:|",
    ]
    for fx, v in sorted(t.items()):
        if "_err" in v:
            lines.append(f"| {fx} | ERROR | — | — | — |")
            continue
        pct = 100.0 * v["differing_samples"] / v["samples"] if v["samples"] else 0
        lines.append(f"| {fx} | {v['max_abs_delta']:.2e} | "
                     f"{v['max_abs_delta_in_16bit_lsb']:.4f} | {pct:.2f}% | "
                     f"{v['samples']} / {v['frames']} |")
    return "\n".join(lines)


def lifecycle_table():
    lc = S.get("lifecycle", {})
    if not lc.get("runtimes"):
        return ""
    lines = [
        "| runtime | load ms | compile/JIT ms | instantiate ms | first open ms | first PCM ms | steady decode ms |",
        "|---|---:|---:|---:|---:|---:|---:|",
    ]
    fx = "flac-16-44-stereo.flac"
    for rt in RTS + (["emscripten"] if "emscripten" in lc.get("runtimes", {}) else []):
        rec = (lc.get("runtimes") or {}).get(rt, {}).get(fx)
        if not rec or "_err" in rec or rec.get("guest") is None:
            lines.append(f"| {rt} | — | — | — | — | — | — |")
            continue
        h = rec.get("host") or {}
        g = rec.get("guest") or {}
        lines.append(f"| {rt} | {fmt(h.get('load_ms'), 2)} | "
                     f"{fmt(h.get('compile_ms'), 2)} | "
                     f"{fmt(h.get('instantiate_ms'), 2)} | "
                     f"{fmt(g.get('open_ms'), 2)} | "
                     f"{fmt(g.get('first_pcm_ms'), 2)} | "
                     f"{fmt(g.get('decode_ms'), 2)} |")
    return "\n".join(lines)


def build_block():
    parts = [
        BEGIN,
        "### 1. Correctness gate（44-case，native twin 为 reference）",
        "",
        "machine authority：`bench/results/wasm/correctness.json` + "
        "`correctness-em.json`（V8 行，Node harness 独立 authority）",
        "",
        gate_table(),
        "",
        "### 3. 执行 ladder（Mode A，execution_tax = T_guest / T_native）",
        "",
        "machine authority：`bench/results/wasm/performance.json`（逐 runtime "
        "5 fixture；native twin 与 guest 同为 -Os codegen 口径）",
        "",
        ladder_table(),
        "",
        "perf stat（flac，bench×3，整进程 elapsed——startup 分阶段见 "
        "`lifecycle`）：",
        "",
        perf_stat_table(),
        "",
        "### 4. Bridge / boundary（flac，bridge_tax = 回拷成本 / 该后端 guest 解码成本）",
        "",
        "machine authority：`bench/results/wasm/memory.json`（Mode B/C）+ "
        "`em_performance.json`（em 行，view/copy/hash 分列）",
        "",
        bridge_table(),
        "",
        "### 6. Memory audit",
        "",
        "machine authority：`bench/results/wasm/memory.json`（native 行已修正 "
        "CLI，RSS 为真实测量）",
        "",
        memory_table(),
        "",
        "### 7. Lifecycle / startup（load → compile → instantiate → first open → first PCM → steady）",
        "",
        "machine authority：`bench/results/wasm/lifecycle.json`（flac）",
        "",
        lifecycle_table() or "（lifecycle.json 未生成）",
        "",
        "### 2. Shipping footprint（product-shaped deployment sets）",
        "",
        "machine authority：`bench/results/wasm/shipping.json`（host = stripped "
        "release runner；AOT 替代 .wasm）",
        "",
        shipping_table(),
        "",
        "### float 容差（native vs wasm，剩余分歧全量解释）",
        "",
        "machine authority：`bench/results/wasm/tolerance.json`（逐 fixture "
        "证据，覆盖全部 tolerated fixture）",
        "",
        tolerance_table(),
        "",
        END,
    ]
    return "\n".join(parts)


def main():
    block = build_block()
    text = DOC.read_text()
    if BEGIN in text:
        pre = text[:text.index(BEGIN)]
        post = text[text.index(END) + len(END):]
        new_text = pre + block + post
    else:
        new_text = text.rstrip() + "\n\n" + block + "\n"
    if "--check" in sys.argv:
        if new_text != text:
            print("DRIFT: generated tables differ from summary.json; "
                  "run tools/wasm_report_tables.py to regenerate")
            return 1
        print("report tables in sync with summary.json")
        return 0
    DOC.write_text(new_text)
    print("report tables generated into", DOC)
    return 0


if __name__ == "__main__":
    sys.exit(main())
