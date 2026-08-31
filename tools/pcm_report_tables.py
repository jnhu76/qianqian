#!/usr/bin/env python3
"""Generates the numeric tables of docs/experiments/e10-pcm-processing.md
from bench/results/pcm-processing/p0-summary.json — the SINGLE machine
authority. Every number between the GENERATED markers comes from
p0-summary.json; this script never reads the section JSONs directly.

  python3 tools/pcm_report_tables.py            (regenerate tables in place)
  python3 tools/pcm_report_tables.py --check    (exit 1 if tables drifted
                                                 from p0-summary.json)
"""

import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
S = json.loads((ROOT / "bench/results/pcm-processing/p0-summary.json").read_text())
DOC = ROOT / "docs/experiments/e10-pcm-processing.md"

BEGIN = "<!-- BEGIN GENERATED TABLES -->"
END = "<!-- END GENERATED TABLES -->"


def gate_table():
    lines = [
        "| gate | verdict |",
        "|---|---|",
    ]
    for k, v in S["gates"].items():
        lines.append(f"| `{k}` | {v} |")
    lines.append("")
    lines.append(f"总体 verdict：**{S['verdict']}**。`report --check` 由 "
                 "`tools/pcm_report_tables.py --check` 单独执行（doc 与 "
                 "authority JSON 漂移即 FAIL）。")
    return "\n".join(lines)


def accounting_table():
    rows = []
    for m in S["key_numbers"]["accounting_modes"]:
        rows.append(
            f"| {m['mode']} | {m['input_frames']:,} | {m['output_frames']:,} "
            f"| {m['process_calls']:,} | {m['explicit_copy_calls']:,} "
            f"| {m['explicit_bytes_copied']:,} "
            f"| {m['zero_copy_frames_forwarded']:,} "
            f"| {m['full_memory_passes']:.3f} "
            f"| {m['peak_buffered_frames']} | "
            f"{m['internal_buffer_capacity_frames']} |")
    g = S["key_numbers"]["allocation_gate"]
    lines = [
        "| 模式 | input frames | output frames | process calls | "
        "explicit copies | copied bytes | forwarded frames | full memory "
        "passes | peak buffered | internal capacity |",
        "|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|",
        *rows,
        "",
        f"allocation gate：{g['instrument']}；armed RT 区间分配 = "
        f"**{g['allocations_in_armed_rt_region']}**，violation = "
        f"**{g['rt_violations']}**（selftest 证明该 gate 具备 FAIL 能力）。"
        f"queue slab 全部在 prepare 期分配"
        f"（{g['queue_slabs_allocated_during_prepare']} 次）。",
    ]
    return "\n".join(lines)


def placement_table():
    lines = [
        "| shape | mode | full memory passes | 峰值缓冲 (slabs×frames) | "
        "update 时 stale frames | reset 丢弃 frames | underrun |",
        "|---|---|---:|---:|---:|---:|---:|",
    ]
    for s in S["key_numbers"]["placement_shapes"]:
        qcap = S["key_numbers"]["placement_schedule"]["queue_capacity_slabs"]
        sframes = S["key_numbers"]["placement_schedule"]["slab_frames"]
        buffered = (f"{s['queue_peak_slabs']}×{sframes} "
                    f"(cap {qcap}×{sframes})")
        lines.append(
            f"| {s['shape']} | {s['mode']} "
            f"| {s['full_memory_passes']:.3f} | {buffered} "
            f"| {s['buffered_frames_at_update_event']:,} "
            f"| {s['reset_dropped_frames']:,} | {s['underruns']} |")
    lines += [
        "",
        f"reps = {S['key_numbers']['placement_schedule']['reps']}，全部 "
        "shape 的 device-stream fnv1a64 逐 rep 相等且跨 shape 相等"
        "（bypass 恒等）。Control-update 可见性：A = 下一 callback 边界"
        "（stale 0）；B = 队列中已缓冲 frames + worker 块量化"
        "（≤ slab_frames-1）。**不做 production placement 决策。**",
        "",
        "callback / pipeline / sink-copy 工作量的 ns 分布（7 reps，"
        "median/min/max）属逐 run 易变数据，只记录于 "
        "`p0-placement.json` authority，不进 doc；本表仅含跨 run 确定"
        "的结构量。",
    ]
    return "\n".join(lines)


def performance_table():
    rows = {("copy", r["channels"], r["block_frames"]): r
            for r in S["key_numbers"]["performance_rows"]
            if r["mode"] == "copy"}
    zrows = {("zero_copy", r["channels"], r["block_frames"]): r
             for r in S["key_numbers"]["performance_rows"]
             if r["mode"] == "zero_copy"}
    lines = [
        "| block frames | ch | copy ns/call (median) | memcpy ref ns/call | "
        "overhead ns/call | overhead ratio | zero-copy ns/call |",
        "|---:|---:|---:|---:|---:|---:|---:|",
    ]
    for (mode, ch, blk), r in sorted(rows.items(), key=lambda x: (x[1]["channels"], x[1]["block_frames"])):
        z = zrows[("zero_copy", ch, blk)]
        lines.append(
            f"| {blk} | {ch} | {r['ns_per_call']['median']:.1f} "
            f"| {r['memcpy_ref_ns_per_call']['median']:.1f} "
            f"| {r['overhead_ns_per_call_vs_memcpy']:.1f} "
            f"| {r['overhead_ratio_vs_memcpy']:.2f} "
            f"| {z['ns_per_call']['median']:.1f} |")
    loops = S["key_numbers"]["performance_rows"][0].get(
        "stream_loops_per_sample", 1)
    lines += [
        "",
        f"timed passes = 5 + 1 warmup，每个 timed sample 内重复整条流 "
        f"{loops} 次（摊销钟/调度噪声；分布见 p0-performance.json）；"
        f"零拷贝转发路径即管线抽象地板（~{zrows[('zero_copy', 2, 256)]['ns_per_call']['median']:.0f} ns/call "
        "@ 256fr stereo）。小 block 行的 overhead 受残余噪声支配，"
        "解读以量级为准。",
    ]
    return "\n".join(lines)


def correctness_table():
    c = S["key_numbers"]["correctness"]
    lc = S["key_numbers"]["lifecycle"]
    rd = lc["reset_determinism"]
    rt = lc["reprepare_rate_transition"]
    zg = lc["zero_copy_guard_with_dsp_stage"]
    pc = lc["partial_consume"]
    lines = [
        "| 项 | 值 |",
        "|---|---|",
        f"| corpus cases（4 patterns × 2ch × 3 rates × 2 corpus × 2 modes） "
        f"| {c['cases_total']} |",
        f"| bit/alias identical | {c['cases_passed']} / {c['cases_total']} |",
        f"| 逐 call memcmp 总字节 | {c['bytes_compared_total']:,} |",
        f"| reset→process 5 cycles 输出 bit-identical | "
        f"{rd['outputs_bit_identical_across_cycles']} |",
        f"| reprepare 44.1k→48k 后输出 bit-identical | "
        f"{rt['outputs_bit_identical_across_rates']} |",
        f"| reconfigure 丢弃 buffered frames | "
        f"{rt['dropped_frames_on_reconfigure']} |",
        f"| zero-copy guard（+1 DSP stage 必须走 copy 路径） | "
        f"{zg['verdict']} |",
        f"| partial consume（out_cap<in_frames 可分段重组） | "
        f"{pc['verdict']} |",
    ]
    return "\n".join(lines)


def build_block():
    parts = [
        BEGIN,
        "### P0 机器 gate",
        "",
        "machine authority：`bench/results/pcm-processing/p0-summary.json`"
        "（由 `tools/pcm_p0.py` 从四个 section JSON 汇编；禁止手抄数字）",
        "",
        gate_table(),
        "",
        "### P0 正确性汇总",
        "",
        "machine authority：`p0-correctness.json`（经 summary 转录）",
        "",
        correctness_table(),
        "",
        "### Buffer / copy accounting（BYPASS，1M frames stereo @48k，256fr blocks）",
        "",
        "machine authority：`p0-buffer-accounting.json`（经 summary 转录）",
        "",
        accounting_table(),
        "",
        "### Placement model：Shape A（RT）vs Shape B（worker）",
        "",
        "machine authority：`p0-placement.json`（经 summary 转录；确定性单线程"
        "调度模型，非真实 RT 线程）",
        "",
        placement_table(),
        "",
        "### BYPASS 开销 block matrix",
        "",
        "machine authority：`p0-performance.json`（经 summary 转录）",
        "",
        performance_table(),
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
            print("DRIFT: generated tables differ from p0-summary.json; "
                  "run tools/pcm_report_tables.py to regenerate")
            return 1
        print("report tables in sync with p0-summary.json")
        return 0
    DOC.write_text(new_text)
    print("report tables generated into", DOC)
    return 0


if __name__ == "__main__":
    sys.exit(main())
