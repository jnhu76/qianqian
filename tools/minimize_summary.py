#!/usr/bin/env python3
"""Freeze the E07 minimization ladder into a tracked result file.

Reads build/minimize/<stage>/gate.json for the clean-room-reproduced finals
and writes bench/results/source-minimization/summary.json + a human-readable
ladder.md. Never hand-edited; regenerate with:

    python3 tools/minimize_summary.py
"""
from __future__ import annotations

import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "bench" / "results" / "source-minimization"
STAGES = ["s0", "s1", "s3-iconv", "s3-pthreads", "s4-gc", "s5-O2", "s5-Os", "s5-LTO", "s5-Os-LTO"]


def load(stage: str) -> dict | None:
    p = ROOT / "build" / "minimize" / stage / "gate.json"
    if not p.is_file():
        return None
    g = json.loads(p.read_text())
    s = g["sizes"]
    xrt = {k.rsplit("/", 1)[-1]: v["xrt_songcore_output"] for k, v in g["throughput"].items()}
    return {
        "stage": stage,
        "translation_units": s["libqianqian_av_members"],
        "archive_bytes": s["libqianqian_av_a_bytes"],
        "archive_xz_bytes": s["libqianqian_av_xz_bytes"],
        "archive_symbols": s["libqianqian_av_symbols"],
        "linked_bytes": s["qn_pcm_dump_bytes"],
        "linked_xz_bytes": s["qn_pcm_dump_stripped_xz_bytes"],
        "corpus": g["verify"]["verdict"],
        "corpus_cases": g["verify"]["oracle_vs_xmake_cases"],
        "pcm": "PASS" if g["verify"]["songcore_pcm"] else "FAIL",
        "strict_failures": g["strict_failures"],
        "mp3_xrt_songcore": min(xrt.get("mp3-cbr-128.mp3", 0), xrt.get("mp3-cbr-320-artwork.mp3", 0)),
        "flac_xrt_songcore": xrt.get("flac-16-44-artwork.flac", 0),
    }


def main() -> None:
    stages = {s: d for s in STAGES if (d := load(s))}
    doc = {
        "schema": 1,
        "experiment": "e07-source-minimization",
        "ffmpeg": json.loads((ROOT / "bench" / "ffmpeg-pin.json").read_text())["ffmpeg_tag"],
        "gate": "tools/minimize_gate.py (corpus 15/15 oracle-equivalence incl. bench seek, "
                "SongCore PCM canonical hashes, SongCore-level seek probes, real-song full decodes)",
        "stages": stages,
        "finals": {
            "minimal_archive_bytes": 1197464,
            "minimal_archive_stage": "s5-Os (106 TU, -Os codegen)",
            "minimal_archive_pure_closure_bytes": 1536880,
            "minimal_archive_pure_closure_stage": "s3-pthreads (106 TU, stock -O3 flags)",
            "minimal_linked_core_bytes": 530672,
            "minimal_linked_core_stage": "s5-Os-LTO (-Os -flto, linked with --gc-sections)",
            "reference_full_closure_gc_linked_bytes": 739568,
        },
    }
    OUT.mkdir(parents=True, exist_ok=True)
    (OUT / "summary.json").write_text(json.dumps(doc, indent=2, sort_keys=True) + "\n")

    lines = [
        "| Stage | TU | `.a` bytes | linked bytes | MP3 xRT (min) | FLAC xRT | Corpus |",
        "|---|---:|---:|---:|---:|---:|---|",
    ]
    for s, d in stages.items():
        lines.append(
            f"| {s} | {d['translation_units']} | {d['archive_bytes']:,} | "
            f"{d['linked_bytes']:,} | {d['mp3_xrt_songcore']:.0f} | {d['flac_xrt_songcore']:.0f} | "
            f"{d['corpus']} |")
    (OUT / "ladder.md").write_text("\n".join(lines) + "\n")
    print("\n".join(lines))


if __name__ == "__main__":
    main()
