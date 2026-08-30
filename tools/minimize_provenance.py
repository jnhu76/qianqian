#!/usr/bin/env python3
"""Accumulate machine-generated minimization provenance entries.

    python3 tools/minimize_provenance.py add --stage s3-pthreads --base s0 \
        --round 2 --verdict removable --change "..."
    python3 tools/minimize_provenance.py add-so --stage s6-shipped-so --round 2

Numbers always come from the stage's gate.json / so.json — never hand-entered.
Round 1 = iterative first pass; round 2 = clean-room-authoritative rerun
(rm -rf build, strict global-only resolver, real-ld map gate, ELF sha gate).
"""
from __future__ import annotations

import argparse
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PROV = ROOT / "bench" / "provenance" / "source-minimization.json"

VERDICTS = ["required", "removable", "no_effect", "configure_forced",
            "rejected_pcm_change", "rejected_performance", "control_reference"]


def load() -> dict:
    if PROV.is_file():
        doc = json.loads(PROV.read_text())
    else:
        doc = {"schema": 2, "entries": []}
    doc["schema"] = 2
    return doc


def stage_numbers(stage: str) -> dict:
    gate = json.loads((ROOT / "build" / "minimize" / stage / "gate.json").read_text())
    sizes = gate["sizes"]
    return {
        "archive_bytes": sizes["libqianqian_av_a_bytes"],
        "tu": sizes["libqianqian_av_members"],
        "linked_raw_bytes": sizes["qn_pcm_dump_bytes"],
        "linked_stripped_bytes": sizes["qn_pcm_dump_stripped_bytes"],
        "archive_xz_bytes": sizes["libqianqian_av_xz_bytes"],
        "linked_stripped_xz_bytes": sizes["qn_pcm_dump_stripped_xz_bytes"],
        "correctness": "PASS" if gate["verify"]["verdict"] == "PASS" and not gate["strict_failures"] else "FAIL",
    }


def main() -> None:
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)
    add = sub.add_parser("add")
    add.add_argument("--stage", required=True)
    add.add_argument("--base", required=True)
    add.add_argument("--round", type=int, default=2)
    add.add_argument("--change", required=True)
    add.add_argument("--verdict", required=True, choices=VERDICTS)
    add_so = sub.add_parser("add-so")
    add_so.add_argument("--stage", required=True)
    add_so.add_argument("--round", type=int, default=2)
    add_so.add_argument("--change", required=True)
    args = ap.parse_args()

    doc = load()
    if args.cmd == "add":
        base_n = stage_numbers(args.base)
        cand_n = stage_numbers(args.stage)
        entry = {
            "candidate": args.stage,
            "round": args.round,
            "change": args.change,
            "bytes_before": base_n["archive_bytes"],
            "bytes_after": cand_n["archive_bytes"],
            "tu_before": base_n["tu"],
            "tu_after": cand_n["tu"],
            "linked_raw_bytes_before": base_n["linked_raw_bytes"],
            "linked_raw_bytes_after": cand_n["linked_raw_bytes"],
            "linked_stripped_bytes_before": base_n["linked_stripped_bytes"],
            "linked_stripped_bytes_after": cand_n["linked_stripped_bytes"],
            "correctness": cand_n["correctness"],
            "verdict": args.verdict,
        }
    else:
        so = json.loads((ROOT / "build" / "minimize" / args.stage / "so.json").read_text())
        s = so["sizes"]
        entry = {
            "candidate": args.stage,
            "round": args.round,
            "change": args.change,
            "so_raw_bytes": s["libqianqian_songcore_so_bytes"],
            "so_stripped_bytes": s["libqianqian_songcore_so_stripped_bytes"],
            "so_stripped_xz_bytes": s["libqianqian_songcore_so_stripped_xz_bytes"],
            "exported_symbols": so["exported_count"],
            "correctness": "PASS" if so["pic_codegen_gate"]["corpus_verdict"] == "PASS"
                           and all(v.get("exit_code") == 0 for v in so["smoke"].values()) else "FAIL",
            "verdict": "control_reference",
        }

    doc["entries"] = [e for e in doc["entries"] if e["candidate"] != args.stage] + [entry]
    PROV.parent.mkdir(parents=True, exist_ok=True)
    PROV.write_text(json.dumps(doc, indent=2, sort_keys=True) + "\n")
    print(f"recorded {args.stage} (round {args.round}): "
          f"{entry.get('bytes_after', entry.get('so_stripped_bytes')):,} bytes after "
          f"({entry['verdict']})")


if __name__ == "__main__":
    main()
