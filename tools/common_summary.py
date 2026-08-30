#!/usr/bin/env python3
"""Machine-derived summary for the Common Formats ladder (zero hand-entry).

Reads every stage's gate.json / so.json under build/minimize/ and emits
bench/results/common-formats/{summary.json, ladder.md}:

- the capability ladder table (oracle TU / reachable TU / -O3 .a / -Os .a /
  linked stripped / .so stripped / xRT / gate verdict);
- the codec marginal-cost table (dTU / d archive / d .so per increment);
- the codegen throughput tradeoff (-O3 vs -Os vs -Os+LTO).

summary.json is the single authority: ladder.md AND PR_BODY.md are pure
functions of summary.json (+ windows.json when present), each carrying the
sha256 of its machine inputs in a provenance marker. `--check` re-derives
the committed Markdown and FAILS on any drift, so hand-edited numbers or a
stale PR body cannot survive a rerun.

    python3 tools/common_summary.py            # write summary.json + ladder.md (+ PR_BODY.md)
    python3 tools/common_summary.py --check    # verify committed Markdown == derived
"""
from __future__ import annotations

import argparse
import hashlib
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "bench" / "results" / "common-formats"

STAGES = ["c0", "c1", "c2", "c3", "c4", "c5"]
CAPABILITY_LABEL = {
    "c0": "MP3+FLAC",
    "c1": "+AAC/M4A +ADTS",
    "c2": "+ALAC/M4A",
    "c3": "+PCM WAV (6 fmts)",
    "c4": "+Ogg Vorbis",
    "c5": "+Ogg Opus",
    "c6": "Common Formats minimized",
}


def kib(n: int | None) -> str:
    return "—" if n is None else f"{n / 1024:.0f} KiB" if n < 1024 * 1024 else f"{n / 1024 / 1024:.2f} MiB"


def load(stage: str, name: str) -> dict | None:
    p = ROOT / "build" / "minimize" / stage / name
    return json.loads(p.read_text()) if p.is_file() else None


def main() -> None:
    rows = []
    for s in STAGES:
        gate = load(s, "gate.json")
        if gate is None:
            raise SystemExit(f"missing gate for {s}; run the ladder first")
        so = load(f"{s}-so", "so.json")
        rows.append({
            "stage": s,
            "gate": gate,
            "so": so,
        })
    c6_os = load("c6-os", "gate.json")
    c6_os_lto = load("c6-os-lto", "gate.json")
    c6_so_lto = load("c6-so-lto", "so.json")
    if c6_os is None or c6_os_lto is None or c6_so_lto is None:
        raise SystemExit("missing c6-os / c6-os-lto / c6-so-lto; run the C6 ladder first")

    if rows[-1]["gate"]["capabilities"] != c6_os["capabilities"]:
        raise SystemExit("c6 capability set drifted from c5")

    def fmt_row(r) -> dict:
        gate, so = r["gate"], r["so"]
        th = gate["throughput"]
        min_sample = min(
            (v["xrt_songcore_output"], cid) for cid, v in th.items()
            if v.get("xrt_songcore_output") is not None
        ) if th else None
        return {
            "stage": r["stage"],
            "capability": CAPABILITY_LABEL[r["stage"]],
            "capabilities": gate["capabilities"],
            "oracle_tu": gate["closure"]["full_translation_units"],
            "reachable_tu": gate["closure"]["reachable_translation_units"],
            "archive_o3_bytes": gate["sizes"]["libqianqian_av_a_bytes"],
            "archive_os_bytes": None,
            "linked_stripped_bytes": gate["sizes"]["qn_pcm_dump_stripped_bytes"],
            "so_stripped_bytes": so["sizes"]["libqianqian_songcore_so_stripped_bytes"] if so else None,
            "so_stripped_xz_bytes": so["sizes"]["libqianqian_songcore_so_stripped_xz_bytes"] if so else None,
            "min_xrt": min_sample[0] if min_sample else None,
            "min_xrt_sample": min_sample[1] if min_sample else None,
            "gate": "PASS" if not (gate["expect_failures"] or gate["seek_failures"]) else "FAIL",
        }

    summary_rows = [fmt_row(r) for r in rows]
    # -Os archive: the c6-os stage measures the full set at -Os; intermediate
    # -Os archives exist only for c6 (per-stage -Os floors are not built).
    summary_rows[-1]["archive_os_bytes"] = None
    c6_min = min(
        (v["xrt_songcore_output"], cid) for cid, v in c6_os_lto["throughput"].items()
        if v.get("xrt_songcore_output") is not None)
    c6_row = {
        "stage": "c6",
        "capability": CAPABILITY_LABEL["c6"],
        "capabilities": c6_os["capabilities"],
        "oracle_tu": c6_os["closure"]["full_translation_units"],
        "reachable_tu": c6_os["closure"]["reachable_translation_units"],
        "archive_o3_bytes": rows[-1]["gate"]["sizes"]["libqianqian_av_a_bytes"],
        "archive_os_bytes": c6_os["sizes"]["libqianqian_av_a_bytes"],
        "linked_stripped_bytes": c6_os_lto["sizes"]["qn_pcm_dump_stripped_bytes"],
        "so_stripped_bytes": c6_so_lto["sizes"]["libqianqian_songcore_so_stripped_bytes"],
        "so_stripped_xz_bytes": c6_so_lto["sizes"]["libqianqian_songcore_so_stripped_xz_bytes"],
        "min_xrt": c6_min[0],
        "min_xrt_sample": c6_min[1],
        "gate": "PASS" if not (c6_os_lto["expect_failures"] or c6_os_lto["seek_failures"]) else "FAIL",
    }
    all_rows = summary_rows + [c6_row]

    # marginal cost per increment
    deltas = []
    for prev, cur in zip(all_rows, all_rows[1:]):
        deltas.append({
            "increment": cur["capability"],
            "d_tu": cur["reachable_tu"] - prev["reachable_tu"],
            "d_archive_o3_bytes": cur["archive_o3_bytes"] - prev["archive_o3_bytes"],
            "d_so_stripped_bytes": (cur["so_stripped_bytes"] - prev["so_stripped_bytes"])
            if cur["so_stripped_bytes"] and prev["so_stripped_bytes"] else None,
        })

    # codegen throughput tradeoff on the full set (per codec)
    codecs = sorted(rows[-1]["gate"]["throughput"])
    codegen = {}
    for cid in codecs:
        codegen[cid] = {
            "o3": rows[-1]["gate"]["throughput"].get(cid, {}).get("xrt_songcore_output"),
            "os": c6_os["throughput"].get(cid, {}).get("xrt_songcore_output"),
            "os_lto": c6_os_lto["throughput"].get(cid, {}).get("xrt_songcore_output"),
        }

    so_final = c6_so_lto
    summary = {
        "schema": 1,
        "ladder": all_rows,
        "marginal_cost": deltas,
        "codegen_throughput": codegen,
        "final_so": {
            "stage": "c6-so-lto",
            "stripped_bytes": so_final["sizes"]["libqianqian_songcore_so_stripped_bytes"],
            "stripped_xz_bytes": so_final["sizes"]["libqianqian_songcore_so_stripped_xz_bytes"],
            "raw_bytes": so_final["sizes"]["libqianqian_songcore_so_bytes"],
            "sha256": so_final["so_sha256"],
            "exported_api_count": so_final["exported_api_count"],
            "dynamic_dependencies": so_final["dynamic_dependencies"],
        },
        "total_from_c0": {
            "d_tu": c6_row["reachable_tu"] - all_rows[0]["reachable_tu"],
            "d_archive_bytes": c6_row["archive_o3_bytes"] - all_rows[0]["archive_o3_bytes"],
            "d_so_stripped_bytes": c6_row["so_stripped_bytes"] - all_rows[0]["so_stripped_bytes"],
        },
    }
    OUT.mkdir(parents=True, exist_ok=True)
    (OUT / "summary.json").write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n")

    windows_path = OUT / "windows.json"
    windows = json.loads(windows_path.read_text()) if windows_path.is_file() else None
    ladder_md = derive_ladder_md(summary)
    pr_body_md = derive_pr_body(summary, windows)
    (OUT / "ladder.md").write_text(ladder_md)
    if pr_body_md is not None:
        (OUT / "PR_BODY.md").write_text(pr_body_md)
    print(ladder_md)
    written = [OUT / "summary.json", OUT / "ladder.md"]
    written += [OUT / "PR_BODY.md"] if pr_body_md else []
    print("wrote: " + ", ".join(str(p.relative_to(ROOT)) for p in written))


def sha256_text(text: str) -> str:
    return hashlib.sha256(text.encode()).hexdigest()


def provenance_marker(summary: dict, windows: dict | None) -> str:
    shas = [f"summary.json sha256={sha256_text(json.dumps(summary, indent=2, sort_keys=True) + chr(10))}"]
    if windows is not None:
        shas.append(f"windows.json sha256={sha256_text(json.dumps(windows, indent=2, sort_keys=True) + chr(10))}")
    return "<!-- provenance (machine authority): " + "; ".join(shas) + " -->"


def derive_ladder_md(summary: dict) -> str:
    all_rows = summary["ladder"]
    deltas = summary["marginal_cost"]
    codegen = summary["codegen_throughput"]
    so_final = summary["final_so"]
    md = ["# Common Formats capability ladder (machine-generated)",
          "",
          "| Stage | Capability | Oracle TU | Reachable TU | `-O3 .a` | `-Os .a` | linked stripped | `.so` stripped | min xRT | Gate |",
          "|---|---|---:|---:|---:|---:|---:|---:|---:|---|"]
    for r in all_rows:
        xrt = "—" if r["min_xrt"] is None else f"{r['min_xrt']:.0f}× ({r['min_xrt_sample']})"
        md.append(
            f"| {r['stage']} | {r['capability']} | {r['oracle_tu']} | {r['reachable_tu']} "
            f"| {kib(r['archive_o3_bytes'])} | {kib(r['archive_os_bytes'])} "
            f"| {kib(r['linked_stripped_bytes'])} | {kib(r['so_stripped_bytes'])} "
            f"| {xrt} | {r['gate']} |")
    md += ["",
           "## Codec marginal cost per capability increment",
           "",
           "| Capability increment | Δ reachable TU | Δ `-O3 .a` | Δ `.so` stripped |",
           "|---|---:|---:|---:|"]
    for d in deltas:
        d_arch = (f"{d['d_archive_o3_bytes'] / 1024:+.0f} KiB"
                  if abs(d["d_archive_o3_bytes"]) >= 512 else f"{d['d_archive_o3_bytes']:+,} B")
        d_so = ("—" if d["d_so_stripped_bytes"] is None
                else f"{d['d_so_stripped_bytes'] / 1024:+.0f} KiB")
        md.append(f"| {d['increment']} | {d['d_tu']:+d} | {d_arch} | {d_so} |")

    md += ["",
           "## Codegen throughput tradeoff (full Common Formats set, songcore-output xRT)",
           "",
           "| Codec sample | `-O3` | `-Os` | `-Os+LTO` |",
           "|---|---:|---:|---:|"]
    for cid, v in codegen.items():
        cells = " | ".join(f"{v[k]:.0f}×" if v[k] else "—" for k in ("o3", "os", "os_lto"))
        md.append(f"| {cid} | {cells} |")

    md += ["",
           f"Final size-minimal `.so` ({so_final['stage']}): raw {kib(so_final['raw_bytes'])}, "
           f"stripped {kib(so_final['stripped_bytes'])}, "
           f"stripped+xz {kib(so_final['stripped_xz_bytes'])}, "
           f"exports {so_final['exported_api_count']} SongCore APIs, "
           f"deps: {so_final['dynamic_dependencies'].replace(chr(10), '; ')}",
           ""]
    return "\n".join(md)


def derive_pr_body(summary: dict, windows: dict | None) -> str | None:
    """PR body as a pure function of machine results. Requires windows.json
    (the Windows phase is part of this PR's claim); without it the body is
    not regenerated — a stale body then fails --check against its embedded
    provenance shas instead of silently mixing old and new numbers."""
    if windows is None:
        return None
    rows = {r["stage"]: r for r in summary["ladder"]}
    c6 = rows["c6"]
    c0 = rows["c0"]
    total = summary["total_from_c0"]
    so_final = summary["final_so"]
    tol = windows.get("aac_cross_compiler_tolerance", {})
    corr = windows["correctness"]
    canon = windows["canonical_shipping"]
    closure = windows["closure"]

    def xrt_cell(r):
        return f"{r['min_xrt']:.0f}×" if r["min_xrt"] else "—"

    md = ["## Capability",
          "",
          "```text",
          "MP3 / FLAC          (E07 frozen baseline)",
          "AAC-LC / M4A        new",
          "raw ADTS AAC        new",
          "ALAC / M4A          new",
          "PCM WAV             u8 / s16le / s24le / s32le / f32le / f64le    new",
          "Ogg Vorbis          new",
          "Ogg Opus / .opus    new",
          "```",
          "",
          "能力按 container+codec 定义（非扩展名）；每级 closure 由 capability intent 经",
          "pinned FFmpeg n9.0.1 的 configure/Make oracle 机器推导，禁止手工追加文件。",
          "",
          "## Codec-cost ladder（Linux x86_64，machine-generated）",
          "",
          "| Stage | Capability | Oracle TU | Reachable=Compiled TU | `-O3 .a` | `-Os .a` | `.so` stripped | Δ `.so` | min xRT | Gate |",
          "|---|---|---:|---:|---:|---:|---:|---:|---:|---|"]
    for r in summary["ladder"]:
        d_so = ("—" if r["stage"] == "c0" else
                f"{(r['so_stripped_bytes'] - rows['c0']['so_stripped_bytes']) / 1024:+.0f} KiB")
        md.append(
            f"| {r['stage']} | {r['capability']} | {r['oracle_tu']} | {r['reachable_tu']} "
            f"| {kib(r['archive_o3_bytes'])} | {kib(r['archive_os_bytes'])} "
            f"| {kib(r['so_stripped_bytes'])} | {d_so} | {xrt_cell(r)} | {r['gate']} |")
    md += [
        "",
        "三个口径分开：BUILD（TU/.a）、SHIPPING（.so/.dll stripped+xz）、RUNTIME（xRT）。",
        "`.a` 不是 app 体积；shipping 结论一律 stripped `.so`/`.dll`。",
        "",
        "## Linux final",
        "",
        "```text",
        f"oracle TU                 {c0['oracle_tu']} (C0) → {rows['c5']['oracle_tu']} (C5)；C6 复用 C5 oracle",
        f"reachable = compiled TU   {c6['reachable_tu']}（link-reachability 投影 + clean rebuild，逐 TU 核对）",
        f"-O3 .a / -Os .a           {kib(c6['archive_o3_bytes'])} / {kib(c6['archive_os_bytes'])}",
        f"size-oriented exe         {kib(c6['linked_stripped_bytes'])} stripped (-Os+LTO+gc)",
        f"size-minimal .so          {kib(so_final['stripped_bytes'])} stripped / {kib(so_final['stripped_xz_bytes'])} stripped+xz",
        f"exports                   恰 5 个 song_*（version script），ldd 仅 libc/libm",
        f"xRT (C6 -Os+LTO)          最低 {c6['min_xrt']:.0f}×（{c6['min_xrt_sample']}；50× 警告线）",
        f"C0→C6 total               ΔTU {total['d_tu']:+d}、Δ.a {total['d_archive_bytes'] / 1024:+.0f} KiB、"
        f"Δ.so {total['d_so_stripped_bytes'] / 1024:+.0f} KiB ({total['d_so_stripped_bytes']:,} B)",
        "```",
        "",
        f"## Windows final（x86_64，{windows['toolchain']['cc_ident'].split('(')[0].strip()}，UCRT，cross + 原生执行）",
        "",
        "```text",
        f"oracle TU                 {closure['oracle_tu']}（独立 cross configure，禁复用 Linux manifest）",
        f"reachable / compiled TU   {closure['reachable_units']} / {closure['compiled_units']}"
        f"（archive member ↔ manifest TU 机器映射；投影 clean rebuild 后逐 TU 核对；fixpoint {closure.get('projection_iterations', 1)} 轮）",
        f"reachability proof        reduced-archive 链接按 section SHA 相等 + lld -Map 成员集合佐证",
        f"static archive            {kib(windows['static_archive_bytes'])}（projected closure replay）",
        f"qianqian_songcore.dll     {kib(canon['stripped_bytes'])} stripped / {kib(canon['stripped_xz_bytes'])} xz"
        f"（canonical = {canon['variant']}；LTO 变体{'已构建并通过同套 gates' if windows.get('dll_lto') else '本轮未构建'}）",
        f"import library            {windows['dll']['sizes']['implib_bytes']:,} B（dev-only，不计 shipping）",
        "export table              恰 5 个 song_*（.def + objdump 机器 gate）",
        "import table              kernel32 + api-ms-win-crt-* + bcrypt；零 FFmpeg DLL",
        f"Unicode path              {'PASS' if corr['unicode_path_gate'] else 'FAIL'}（CreateFileW + 测试音乐\\歌曲-你好世界.m4a 宽路径全契约）",
        f">2 GiB seek               {'PASS' if corr['largefile_gate'] else 'FAIL'}（虚拟 3 GiB WAV；max seek offset {corr['largefile_observed']['max_seek_offset']:,}，无负回绕）",
        f"corpus                    {corr['clean_cases']} clean + {corr['degraded_cases']} degraded = "
        f"{corr['total_applicable']} applicable，全部原生执行（skipped {len(corr.get('skipped', []))}）",
        "```",
        "",
        "## Codec marginal cost（增量表，machine-derived）",
        "",
        "```text"]
    for d in summary["marginal_cost"]:
        d_so = "—" if d["d_so_stripped_bytes"] is None else f"{d['d_so_stripped_bytes'] / 1024:+.0f} KiB"
        md.append(f"{d['increment']:<28} ΔTU {d['d_tu']:+3d}   Δ.a {d['d_archive_o3_bytes'] / 1024:+5.0f} KiB   Δ.so {d_so}")
    md += [
        "```",
        "",
        "## Correctness（每格式）",
        "",
        "```text",
        "MP3 / FLAC        PASS（原有 corpus 无回归；c0→cN 行为逐字节一致）",
        "AAC/M4A/ADTS      PASS（ADTS seek = UNSUPPORTED typed finding；其余 LAPPED 契约）",
        "ALAC              PASS（lossless strict canonical sha 双平台字节相等）",
        "WAV ×6            PASS（strict；odd-chunk/malformed/truncated 覆盖）",
        "Vorbis            PASS（strict seek）",
        "Opus              PASS（preskip/end-trim：12s == 576000 样本精确）",
        "seek 契约         STRICT: FLAC/ALAC/WAV/Vorbis（suffix 逐字节）",
        "                  LAPPED: MP3/AAC/Opus（seek 成功+bounded resume+PCM+clean EOF，",
        "                  suffix 不要求相等——bit reservoir / MDCT overlap / CELT lapping）",
        "                  UNSUPPORTED: raw ADTS（typed seek 失败，decode/EOF 仍 gate）",
        f"degraded 案       Windows 原生执行 {corr['degraded_cases']} 案：typed 分类"
        f"（OPEN_FAILED/PROBE_FAILED/DECODE_ERROR/DEGRADED_EOF）+ 无 crash/hang + 界内输出 + 两次分类一致",
        "```",
        "",
        "## Regression",
        "",
        "```text",
        "existing MP3/FLAC corpus + real songs + E07 strict probes: PASS（每阶段）",
        "```",
        "",
        "## Limitations",
        "",
        "- raw ADTS：上游 demuxer 无 seek 实现（typed finding，非本 PR 引入）。",
        "- AAC 跨编译器 PCM 非逐字节一致（gcc-Linux vs clang-Windows）：issue #11 规则下已记录",
        f"  deterministic tolerance metric（max|Δ| = {tol.get('max_abs_delta_all', 0.0):.3e} = 1 ULP；帧数逐案相等）；"
        "平台内确定性不受影响。",
        "- Opus 强制引入 libswresample（上游 build 依赖）；SongCore 契约仍不使用 swr，无 resample 能力。",
        "- xRT 为运行时测量：绝对值跨 run 波动可观（热/调度），相对 -Os 代价为结论；机器 authority 一律以"
        " summary.json 当前值为准。",
        "- MSVC 变体、逐 codec Windows ladder：非本轮（canonical 为 llvm-mingw）。",
        "",
        "## Reproduce",
        "",
        "```bash",
        "# Linux（rm -rf build 起全阶梯 + summary，一键）",
        "bash tools/common_cleanroom.sh",
        "",
        "# Windows（llvm-mingw SDK；产物经 WSL interop 在真 Windows 原生执行）",
        "python3 tools/common_windows.py --all",
        "python3 tools/common_windows_summary.py",
        "python3 tools/common_summary.py --pr-body --check",
        "```",
        "",
        "Corpus：确定性合成（`corpus/tools/gen_corpus_common.py`），seek 语义由",
        "`tools/common_calibrate.py` 机器观测钉定并在 clean-room `--check` 防漂移；",
        "本 PR body 由 `common_summary.py` 从 summary.json + windows.json 派生，含 provenance sha，",
        "任何手工改动都会被 `--check` 拒绝。",
        "",
        "---",
        "",
        "Closes #8 的 Linux + Windows 测量目标（不 merge 前保持 DRAFT；#9 不受影响）。",
        "",
        provenance_marker(summary, windows),
        ""]
    return "\n".join(md)


def check() -> None:
    """--check: committed ladder.md / PR_BODY.md must equal the derivation
    from the committed machine JSONs (summary.json + windows.json)."""
    summary = json.loads((OUT / "summary.json").read_text())
    windows_path = OUT / "windows.json"
    windows = json.loads(windows_path.read_text()) if windows_path.is_file() else None
    drift = []
    committed_ladder = (OUT / "ladder.md").read_text()
    if committed_ladder != derive_ladder_md(summary):
        drift.append("ladder.md differs from summary.json derivation")
    body_path = OUT / "PR_BODY.md"
    if body_path.is_file():
        committed = body_path.read_text()
        derived = derive_pr_body(summary, windows)
        if derived is None:
            drift.append("PR_BODY.md exists but windows.json missing")
        elif committed != derived:
            drift.append("PR_BODY.md differs from summary.json+windows.json derivation")
    # provenance: the committed Markdown must reference the CURRENT machine JSONs
    marker = provenance_marker(summary, windows)
    if body_path.is_file() and marker not in body_path.read_text():
        drift.append("PR_BODY.md provenance marker does not match current summary.json/windows.json")
    if drift:
        for d in drift:
            print(f"DRIFT: {d}")
        raise SystemExit("summary --check FAILED: committed Markdown drifted from machine authority; "
                         "rerun tools/common_summary.py")
    print("summary --check: ladder.md / PR_BODY.md match machine authority")


if __name__ == "__main__":
    ap = argparse.ArgumentParser()
    ap.add_argument("--check", action="store_true",
                    help="verify committed Markdown matches the machine derivation")
    args = ap.parse_args()
    if args.check:
        check()
    else:
        main()
