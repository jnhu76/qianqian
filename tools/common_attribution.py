#!/usr/bin/env python3
"""Machine byte attribution for a capability increment (C0 -> C1 by default).

Answers "where do the C1 shipping bytes actually live" at MEMBER granularity:
for every archive member pulled in the increment stage but NOT in the base
stage, its file-backed section bytes (measured by tools/common_gate.py's
unit-bytes.json capture: offset-extracted members, `size -A`, DWARF/bss
excluded) are grouped by family using the MANIFEST SOURCE PATH — so colliding
basenames (libavcodec/aacdec.c = AAC decoder vs libavformat/aacdec.c = raw
ADTS demuxer in FFmpeg n9.0.1) are attributed exactly, never by name guess.

Object-file section bytes are an upper bound of post-gc live bytes; the
live total is cross-checked against the .so linker map in the experiment doc.

    python3 tools/common_attribution.py --base c0 --stage c1
"""
from __future__ import annotations

import argparse
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "bench" / "results" / "common-formats"


def family(source: str) -> str:
    """Capability family by manifest source path (n9.0.1 file layout)."""
    if source.startswith("libavformat/") and (
            source.startswith("libavformat/mov") or source.startswith("libavformat/isom")):
        return "mov/isom"
    if source in ("libavformat/aacdec.c", "libavformat/rawdec.c"):
        # n9.0.1: the raw ADTS demuxer lives in libavformat/aacdec.c (+ shared
        # rawdemux base in rawdec.c) — NOT libavcodec/aacdec.c (the decoder)
        return "adts-demux"
    if source.startswith("libavcodec/aac"):
        return "aac-decoder"
    return "shared"


def load(stage: str):
    """(sources list, unit_object -> pulled?, unit_object -> file_backed_bytes)."""
    stage_dir = ROOT / "build" / "minimize" / stage
    sources = json.loads((stage_dir / "reachable-sources.json").read_text())
    bytes_by_unit = {u["unit_object"]: u["file_backed_bytes"]
                     for u in json.loads((stage_dir / "unit-bytes.json").read_text())}
    pulled = {u["unit_object"]: u["pulled"] and u["object"] in bytes_by_unit
              for u in sources}
    return sources, pulled, bytes_by_unit


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--base", default="c0")
    ap.add_argument("--stage", default="c1")
    ap.add_argument("--out", default=str(OUT / "attribution.json"))
    args = ap.parse_args()

    _, base_pulled, base_bytes = load(args.base)
    stage_sources, stage_pulled, stage_bytes = load(args.stage)
    source_of = {u["object"]: u["source"] for u in stage_sources}

    per_family: dict[str, int] = {}
    members: list[dict] = []
    for unit, pulled_now in stage_pulled.items():
        if not pulled_now or base_pulled.get(unit):
            continue  # increment members only: pulled at this stage, not at base
        b = stage_bytes[unit]
        src = source_of[unit]
        fam = family(src)
        per_family[fam] = per_family.get(fam, 0) + b
        members.append({"unit": unit, "source": src,
                        "family": fam, "file_backed_bytes": b})
    members.sort(key=lambda m: -m["file_backed_bytes"])

    families = ("mov/isom", "adts-demux", "aac-decoder", "shared")
    rows = [{"family": f, "increment_bytes": per_family.get(f, 0)} for f in families]
    total = sum(r["increment_bytes"] for r in rows)
    result = {
        "schema": 2,
        "method": "increment members (pulled at stage, not at base) x file-backed "
                  "section bytes per manifest unit (size -A, DWARF/bss excluded); "
                  "families by manifest source path, so duplicate basenames "
                  "(aacdec decoder vs ADTS demuxer) are attributed exactly. "
                  "Upper bound of post-gc live bytes; live .so total is cross-checked "
                  "in docs/experiments/e08-common-formats.md",
        "base": {"stage": args.base,
                 "pulled_units": sum(1 for v in base_pulled.values() if v)},
        "stage": {"stage": args.stage,
                  "pulled_units": sum(1 for v in stage_pulled.values() if v)},
        "increment_attribution": rows,
        "increment_total_bytes": total,
        "members_top": members[:15],
    }
    out = Path(args.out)
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")

    print(f"{'family':<14}{'increment bytes':>18}{'share':>8}")
    for r in rows:
        share = f"{r['increment_bytes'] / total:6.1%}" if total else "     —"
        print(f"{r['family']:<14}{r['increment_bytes']:>18,}{share:>8}")
    print(f"{'TOTAL':<14}{total:>18,}")
    print(f"wrote {out}")


if __name__ == "__main__":
    main()
