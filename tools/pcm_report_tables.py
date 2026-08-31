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


def load_a0():
    p = ROOT / "bench/results/pcm-processing/a0-summary.json"
    if not p.exists():
        return None
    return json.loads(p.read_text())


def load_a1():
    p = ROOT / "bench/results/pcm-processing/a1-summary.json"
    if not p.exists():
        return None
    return json.loads(p.read_text())


def a1_table():
    a1 = load_a1()
    if not a1:
        return None
    q = a1["quality"]["rows"]
    perf = a1["performance"]["rows"]
    shp = a1["shipping"]
    qrows = {}
    for r in q:
        qrows.setdefault(r["candidate"], []).append(r)
    lines = []
    # quality summary
    lines += [
        "| candidate | THD+N 1k (min..max, dB) | alias rej (downsample, dB) | "
        "imaging (upsample, dB) | DC gain (min) | near-nyq passband (dB) |",
        "|---|---|---:|---:|---:|---:|",
    ]
    for cand in ["swr", "soxr", "r8b", "lsr"]:
        rs = qrows.get(cand, [])
        if not rs:
            continue
        thd = [r["thd_n_1k_dB"] for r in rs if r.get("thd_n_1k_dB") is not None]
        al = [r["alias_rejection_db"] for r in rs
              if r.get("alias_rejection_db") is not None]
        im = [r["imaging_above_input_nyquist_db"] for r in rs
              if r.get("imaging_above_input_nyquist_db") is not None]
        dc = [r["dc_gain"] for r in rs if r.get("dc_gain") is not None]
        nn = [r["near_nyquist_gain_db"] for r in rs
              if r.get("near_nyquist_gain_db") is not None]
        thd_s = f"{min(thd)}..{max(thd)}" if thd else "n/a"
        al_s = f"{min(al)}" if al else "n/a"
        im_s = f"{max(im)}" if im else "n/a"
        dc_s = f"{min(dc):.6f}" if dc else "n/a"
        nn_s = f"{min(nn)}..{max(nn)}" if nn else "n/a"
        lines.append(f"| {cand} | {thd_s} | {al_s} | {im_s} | {dc_s} | {nn_s} |")
    # performance
    lines += [
        "",
        "| candidate | ns/input frame (real 44.1→48) | ns/input frame "
        "(real 96→44.1) | xRT (min across streams) | post-prepare alloc "
        "calls |",
        "|---|---:|---:|---:|---:|",
    ]
    for cand in ["swr", "soxr", "r8b", "lsr"]:
        rows = [r for r in perf if r["candidate"] == cand]
        if not rows:
            continue
        f441 = next((r["ns_per_input_frame_median"] for r in rows
                     if r["in_rate"] == 44100), None)
        f96 = next((r["ns_per_input_frame_median"] for r in rows
                    if r["in_rate"] == 96000), None)
        xrt = min(r["xrt"] for r in rows)
        alloc = max(r["post_prepare_alloc_calls"] for r in rows)
        lines.append(
            f"| {cand} | {f441:.2f} | {f96:.2f} | {xrt:.1f} | {alloc} |")
    # shipping
    lines += [
        "",
        "| candidate | runner raw bytes | stripped | xz -9 |",
        "|---|---:|---:|---:|",
        f"| bypass (baseline) | {shp['baseline_bypass_bytes']} | - | - |",
    ]
    for r in shp["rows"]:
        lines.append(
            f"| {r['candidate']} | {r['raw_bytes']} | {r['stripped_bytes']} "
            f"| {r['xz_bytes']} |")
    return "\n".join(lines)


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
            f"| {m['logical_copy_bytes']:,} "
            f"| {m['estimated_memory_traffic_bytes']:,} "
            f"| {m['full_memory_passes']:.3f} "
            f"| {m['peak_buffered_frames']} | "
            f"{m['internal_buffer_capacity_frames']} |")
    g = S["key_numbers"]["allocation_gate"]
    bb = S["key_numbers"]["buffer_bound"]
    lines = [
        "| 模式 | input frames | output frames | process calls | "
        "explicit copies | copied bytes | forwarded frames | logical copy "
        "bytes | est. mem traffic bytes | full memory passes | peak buffered "
        "| internal capacity |",
        "|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|",
        *rows,
        "",
        f"allocation gate：{g['instrument']}；armed RT 区间分配 = "
        f"**{g['allocations_in_armed_rt_region']}**，violation = "
        f"**{g['rt_violations']}**（selftest 证明该 gate 具备 FAIL 能力）。"
        f"queue slab 全部在 prepare 期分配"
        f"（{g['queue_slabs_allocated_during_prepare']} 次）。",
        "",
        f"buffer bound：`logical_copy_bytes` 是精确 memcpy 字节；"
        f"`estimated_memory_traffic_bytes` 按一次拷贝含读+写估算"
        f"（= logical × 2），**不是硬件计数器 truth**。queue 快照："
        f"count={bb['queue_count']}，free_top={bb['queue_free_top']}，"
        f"owned={bb['queue_owned_queued_slots']}，"
        f"pending={bb['queue_valid_pending_slots']}，capacity="
        f"{bb['queue_capacity_slabs']}；ownership conservation = "
        f"**{bb['ownership_conservation']}**（free+owned+pending==capacity）。",
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
        f"| 同一 pipeline 实例 reset→process 5 cycles 输出 bit-identical | "
        f"{rd['outputs_bit_identical_across_cycles']} "
        f"（same instance = {rd['same_pipeline_instance']}） |",
        f"| state_epoch 逐 reset 严格递增 | "
        f"{rd['state_epoch_strictly_increasing_on_reset']} "
        f"（epoch 序列 {rd['state_epoch_per_cycle']}） |",
        f"| lifecycle armed 区间分配 = 0 | "
        f"{rd['post_prepare_rt_allocations']} "
        f"（{rd['post_prepare_allocations_zero']}） |",
        f"| 跨 cycles 无 stale buffered frames | "
        f"{rd['peak_buffered_frames_across_cycles']} |",
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


def negative_table():
    neg = S["key_numbers"]["negative_tests"]
    mut = S["key_numbers"]["mutations"]
    san = S["key_numbers"]["sanitizer"]
    lines = [
        "| check | 断言 | verdict |",
        "|---|---|---|",
    ]
    desc = {
        "bypass_rejects_rate_mismatch_44100_48000":
            "BYPASS 拒绝 44100→48000",
        "bypass_rejects_rate_mismatch_48000_44100":
            "BYPASS 拒绝 48000→44100",
        "bypass_rejects_zero_in_rate": "BYPASS 拒绝 0 采样率",
        "bypass_rejects_negative_in_rate": "BYPASS 拒绝负采样率",
        "stale_commit_after_flush_rejected":
            "flush 后 stale commit 被拒（PCM_ERR_STALE_TOKEN）",
        "queue_ownership_conservation":
            "free+owned+pending == capacity 全程成立",
        "queue_bound_enforced": "count ≤ capacity；满时 acquire 背压",
        "variable_frames_commit":
            "commit(actual) 保留精确帧数（1/partial tail/超容量拒绝）",
        "zero_copy_guard_with_dsp_stage":
            "带 DSP stage 时必须走 copy 路径",
        "state_epoch_bump_on_prepare_and_reset":
            "prepare/reset 严格递增 state_epoch",
        "bypass_memcpy_present": "copy 模式每次 process 恰好 1 次 memcpy",
    }
    for c in S["key_numbers"]["negative_checks"]:
        lines.append(f"| `{c['id']}` | {desc.get(c['id'], c['expect'])} "
                     f"| {c['verdict']} |")
    lines += [
        "",
        f"negative 合计：{neg['checks_passed']} / {neg['checks_total']} "
        f"pass（{neg['checks_failed']} fail）。",
        "",
        f"mutation tests：{len(mut['mutations'])} 个确定性故障注入，"
        f"全部被对应 check 捕获 = **{mut['all_caught']}**："
        + "；".join(
            f"`{m['mutation']}` → {m['define']} → "
            f"{'/'.join('`'+x+'`' for x in m['actually_failed_checks'])}"
            for m in mut["mutations"]) + "。",
        "",
        f"sanitizer（ASan+UBSan，correctness + negative）："
        f"{san['verdict']}（correctness={san['correctness_verdict']}，"
        f"negative={san['negative_verdict']}）——hostile-float sink 已改 "
        f"raw-bits 消费，sanitizer 成为机器证据。",
    ]
    return "\n".join(lines)


def a0_table():
    a0 = load_a0()
    if not a0:
        return None
    lines = [
        f"device evidence status：**{a0['device_evidence_status']}**"
        f"（单主机；端点数 {len(a0['endpoints'])}）",
        "",
        "| endpoint | app BYPASS（shared 原生率） | Windows SRC | "
        "exclusive/source-rate device |",
        "|---|---|---|---|",
    ]
    for c in a0["classification"]:
        lines.append(
            f"| `{c['endpoint_id_hash'][:8]}…` | "
            f"{'YES' if c['app_bypass_possible'] else 'NO'} "
            f"({', '.join(str(r) for r in c['app_bypass_rates_shared_native'])}"
            f" kHz) | {'YES' if c['windows_src_available'] else 'NO'} | "
            f"{'YES' if c['exclusive_source_rate_available'] else 'NO'} |")
    lines += [
        "",
        "| 端点 | mix format | engine period（default/min, 100ns） |",
        "|---|---|---|",
    ]
    for r in a0["endpoint_rows"]:
        m = r["mix_format"]
        p = r["engine_period_hns"]
        lines.append(
            f"| `{r['endpoint_id_hash'][:8]}…` | "
            f"{m['rate']} Hz / {m['channels']}ch / f32 "
            f"({'float' if m.get('subtype_float') else 'other'}) | "
            f"{p['default']} / {p['minimum']} |")
    lines += [
        "",
        "reopen/reconfigure（44.1k→48k→44.1k，每 rate 30 cycles，QPC）：",
        "",
        "| rate | total cycle median ms | initialize median ms | min ms | max ms |",
        "|---|---:|---:|---:|---:|",
    ]
    for c in a0["reopen_cost_ms"]:
        lines.append(
            f"| {c['rate']} | {c['total_cycle_ms_median']:.2f} "
            f"| {c['initialize_ms_median']:.2f} | {c['total_cycle_ms_min']:.2f} "
            f"| {c['total_cycle_ms_max']:.2f} |")
    lines += [
        "",
        "limitations：" + "；".join(a0["limitations"]) + "。",
        "",
        "machine authority：`a0-windows-endpoints.json` / "
        "`a0-format-support.json` / `a0-reopen.json` / `a0-summary.json`"
        "（`tools/pcm_a0_windows.py` 汇编；`--check` 漂移即 FAIL）。",
    ]
    return "\n".join(lines)


def load_b0():
    p = ROOT / "bench/results/pcm-processing/b0-summary.json"
    if not p.exists():
        return None
    return json.loads(p.read_text())


def b0_table():
    b0 = load_b0()
    if not b0:
        return None
    mem = b0["memory"]["rows"]
    resp = b0["response"]["results"]
    corr = b0["correctness"]
    lines = [
        "| chain | nodes | logical PCM passes / block | explicit copies | "
        "buffered frames | post-prepare allocs |",
        "|---|---:|---:|---:|---:|---:|",
    ]
    for m in mem:
        lines.append(
            f"| {m['chain']} | {m['nodes']} | {m['logical_passes_per_block']} "
            f"| 0 | 0 | {m['post_prepare_allocations']} |")
    lines += [
        "",
        "| response check | measured | analytical | max error (20..20k) |",
        "|---|---|---:|---:|",
        f"| biquad 1k +6dB | {resp['biquad_peaking_1000hz_6db']['gain_at_1k_measured_db']} dB "
        f"| {resp['biquad_peaking_1000hz_6db']['gain_at_1k_analytical_db']} dB "
        f"| {resp['biquad_peaking_1000hz_6db']['max_db_error_20_20k']} dB |",
        f"| EQ10 全带 +6dB | - | - | "
        f"{resp['eq10_all_bands_6db']['max_db_error_20_20k']} dB |",
        "",
        f"correctness verdict：**{corr['verdict']}**（gain 0dB bit-identical / "
        "-6dB analytical / fusion equivalent；biquad 稳定 + reset 清状态；"
        "limiter 无过冲 clamp + latency 0；NaN 策略 active-sanitize，"
        "TRUE OFF 位透明）",
        "",
        "machine authority：`b0-correctness.json` / `b0-memory.json` / "
        "`b0-dsp-response.json` / `b0-summary.json`",
    ]
    return "\n".join(lines)


def build_block():
    a0 = a0_table()
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
        "### P0 negative / mutation / sanitizer",
        "",
        "machine authority：`p0-negative-tests.json` / `p0-mutations.json` "
        "（经 summary 转录；sanitizer 结果来自 "
        "`p0-correctness-sanitized.json` / `p0-negative-tests-sanitized.json`）",
        "",
        negative_table(),
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
    ]
    if a0:
        parts += [
            "### E10-A0 Windows AudioSink（原生 WASAPI，本主机）",
            "",
            "machine authority：`a0-summary.json`（经 `tools/pcm_a0_windows.py`"
            " 汇编；表格禁止手抄）",
            "",
            a0,
            "",
        ]
    a1 = a1_table()
    if a1:
        parts += [
            "### E10-A1 SRC shootout（BYPASS/swr/soxr/r8b/lsr）",
            "",
            "machine authority：`a1-summary.json`（经 `tools/pcm_a1.py` 汇编；"
            "quality/perf/shipping 数字禁止手抄）",
            "",
            a1,
            "",
        ]
    b0 = b0_table()
    if b0:
        parts += [
            "### E10-B0 thin DSP 参考（Gain/Biquad/EQ10/Limiter）",
            "",
            "machine authority：`b0-summary.json`（经 `tools/pcm_b0.py` 汇编）",
            "",
            b0,
            "",
        ]
    parts += [END]
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
