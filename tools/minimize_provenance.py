#!/usr/bin/env python3
"""Accumulate machine-generated minimization provenance entries.

    python3 tools/minimize_provenance.py add --stage s3-iconv --change "..." \
        --verdict removable
Reads each stage's gate.json (and projected manifest) for the numbers.
"""
from __future__ import annotations

import argparse
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PROV = ROOT / "bench" / "provenance" / "source-minimization.json"


def load() -> dict:
    if PROV.is_file():
        return json.loads(PROV.read_text())
    return {"schema": 1, "entries": []}


def stage_numbers(stage: str) -> dict:
    gate = json.loads((ROOT / "build" / "minimize" / stage / "gate.json").read_text())
    sizes = gate["sizes"]
    return {
        "archive_bytes": sizes["libqianqian_av_a_bytes"],
        "tu": sizes["libqianqian_av_members"],
        "linked_bytes": sizes["qn_pcm_dump_bytes"],
        "archive_xz_bytes": sizes["libqianqian_av_xz_bytes"],
        "linked_xz_bytes": sizes["qn_pcm_dump_stripped_xz_bytes"],
        "correctness": "PASS" if gate["verify"]["verdict"] == "PASS" and not gate["strict_failures"] else "FAIL",
    }


def main() -> None:
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)
    add = sub.add_parser("add")
    add.add_argument("--stage", required=True)
    add.add_argument("--base", required=True)
    add.add_argument("--change", required=True)
    add.add_argument("--verdict", required=True,
                    choices=["required", "removable", "no_effect", "configure_forced",
                             "rejected_pcm_change", "rejected_performance"])
    args = ap.parse_args()

    doc = load()
    base_n = stage_numbers(args.base)
    cand_n = stage_numbers(args.stage)
    entry = {
        "candidate": args.stage,
        "change": args.change,
        "bytes_before": base_n["archive_bytes"],
        "bytes_after": cand_n["archive_bytes"],
        "tu_before": base_n["tu"],
        "tu_after": cand_n["tu"],
        "linked_bytes_before": base_n["linked_bytes"],
        "linked_bytes_after": cand_n["linked_bytes"],
        "correctness": cand_n["correctness"],
        "verdict": args.verdict,
    }
    doc["entries"] = [e for e in doc["entries"] if e["candidate"] != args.stage] + [entry]
    PROV.parent.mkdir(parents=True, exist_ok=True)
    PROV.write_text(json.dumps(doc, indent=2, sort_keys=True) + "\n")
    print(f"recorded {args.stage}: {entry['bytes_before']:,} -> {entry['bytes_after']:,} "
          f"({entry['verdict']})")


if __name__ == "__main__":
    main()
