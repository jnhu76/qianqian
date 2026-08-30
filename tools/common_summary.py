#!/usr/bin/env python3
"""Machine-derived summary for the Common Formats ladder (zero hand-entry).

Reads every stage's gate.json / so.json under build/minimize/ and emits
bench/results/common-formats/{summary.json, ladder.md}:

- the capability ladder table (oracle TU / reachable TU / -O3 .a / -Os .a /
  linked stripped / .so stripped / xRT / gate verdict);
- the codec marginal-cost table (dTU / d archive / d .so per increment);
- the codegen throughput tradeoff (-O3 vs -Os vs -Os+LTO).

Every number is derived; the script asserts cross-source consistency
(e.g. c6 == c5 capability set) and fails loudly rather than inventing data.

    python3 tools/common_summary.py
"""
from __future__ import annotations

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
        xrt = min((v["xrt_songcore_output"] for v in th.values()), default=None)
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
            "min_xrt": xrt,
            "gate": "PASS" if not (gate["expect_failures"] or gate["strict_failures"]) else "FAIL",
        }

    summary_rows = [fmt_row(r) for r in rows]
    # -Os archive: the c6-os stage measures the full set at -Os; intermediate
    # -Os archives exist only for c6 (per-stage -Os floors are not built).
    summary_rows[-1]["archive_os_bytes"] = None
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
        "min_xrt": min(v["xrt_songcore_output"] for v in c6_os_lto["throughput"].values()),
        "gate": "PASS" if not (c6_os_lto["expect_failures"] or c6_os_lto["strict_failures"]) else "FAIL",
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

    # ---- markdown ----
    md = ["# Common Formats capability ladder (machine-generated)",
          "",
          "| Stage | Capability | Oracle TU | Reachable TU | `-O3 .a` | `-Os .a` | linked stripped | `.so` stripped | min xRT | Gate |",
          "|---|---|---:|---:|---:|---:|---:|---:|---:|---|"]
    for r in all_rows:
        md.append(
            f"| {r['stage']} | {r['capability']} | {r['oracle_tu']} | {r['reachable_tu']} "
            f"| {kib(r['archive_o3_bytes'])} | {kib(r['archive_os_bytes'])} "
            f"| {kib(r['linked_stripped_bytes'])} | {kib(r['so_stripped_bytes'])} "
            f"| {r['min_xrt']:.0f}× | {r['gate']} |")
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
           f"Final size-minimal `.so` (c6-so-lto): raw {kib(so_final['sizes']['libqianqian_songcore_so_bytes'])}, "
           f"stripped {kib(so_final['sizes']['libqianqian_songcore_so_stripped_bytes'])}, "
           f"stripped+xz {kib(so_final['sizes']['libqianqian_songcore_so_stripped_xz_bytes'])}, "
           f"exports {so_final['exported_api_count']} SongCore APIs, "
           f"deps: {so_final['dynamic_dependencies'].replace(chr(10), '; ')}",
           ""]
    (OUT / "ladder.md").write_text("\n".join(md))
    print("\n".join(md))
    print(f"\nwrote {OUT / 'summary.json'} and {OUT / 'ladder.md'}")


if __name__ == "__main__":
    main()
