#!/usr/bin/env python3
"""Cross-stage regression comparison for the Common Formats ladder.

Contract: for every corpus case / seek probe present in BOTH stage gates,
the recorded behavior must be IDENTICAL (modulo timings and ASLR text).
Keys that exist only in the newer stage are validated inside that stage's
own gate (oracle equality + expectations) and are ignored here.

    python3 tools/common_compare.py c0 c1
"""
from __future__ import annotations

import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tools"))
from common_corpus import strip_measurements  # noqa: E402


def load(stage: str) -> dict:
    p = ROOT / "build" / "minimize" / stage / "gate.json"
    if not p.is_file():
        raise SystemExit(f"missing {p}; gate the stage first")
    return json.loads(p.read_text())


def behavior(gate: dict) -> dict:
    verify = strip_measurements(gate["verify"])
    verify.pop("applicable_cases", None)  # grows with the capability set by design
    return {
        "verify": verify,
        "expectations": strip_measurements(gate["expectations"]),
        "songcore_pcm": gate["songcore_pcm"],
        "seek_probe": strip_measurements(gate["seek_probe"]),
        "seek_failures": gate["seek_failures"],
    }


def prune(newer, older):
    """Keep only keys present in the older gate (regression subset)."""
    if isinstance(newer, dict) and isinstance(older, dict):
        return {k: prune(v, older[k]) for k, v in newer.items() if k in older}
    return newer


def main() -> None:
    base_id, cand_id = sys.argv[1], sys.argv[2]
    base, cand = load(base_id), load(cand_id)

    b = behavior(base)
    c = prune(behavior(cand), b)

    diffs = [section for section in b if b[section] != c[section]]
    bs, cs = base["sizes"], cand["sizes"]

    def fmt(delta: int) -> str:
        return f"{delta:+,}" if delta else "+0"

    print(f"stage {base_id} -> {cand_id}")
    print(f"  archive: {bs['libqianqian_av_a_bytes']:,} -> {cs['libqianqian_av_a_bytes']:,} "
          f"({fmt(cs['libqianqian_av_a_bytes'] - bs['libqianqian_av_a_bytes'])})")
    print(f"  reachable TU: {cand['closure']['reachable_translation_units']} "
          f"(base {base['closure']['reachable_translation_units']})")
    print(f"  linked stripped: {bs['qn_pcm_dump_stripped_bytes']:,} -> "
          f"{cs['qn_pcm_dump_stripped_bytes']:,} "
          f"({fmt(cs['qn_pcm_dump_stripped_bytes'] - bs['qn_pcm_dump_stripped_bytes'])})")
    for cid in sorted(cand["throughput"]):
        prev = base["throughput"].get(cid)
        now = cand["throughput"][cid]
        if prev:
            print(f"  xrt {cid:22s} {prev['xrt_songcore_output']:8.1f} -> "
                  f"{now['xrt_songcore_output']:8.1f}")
        else:
            print(f"  xrt {cid:22s} (new)            {now['xrt_songcore_output']:8.1f}")

    if diffs:
        for section in diffs:
            print(f"  BEHAVIOR DRIFT in {section}")
            bb, cc = b[section], c[section]
            if isinstance(bb, dict):
                for k in sorted(set(bb) & set(cc)):
                    if bb[k] != cc[k]:
                        print(f"    {k}:")
                        print(f"      base: {json.dumps(bb[k], sort_keys=True)[:400]}")
                        print(f"      cand: {json.dumps(cc[k], sort_keys=True)[:400]}")
        sys.exit(1)
    print("  behavior: IDENTICAL on the shared (regression) corpus")
    print(f"  VERDICT: {cand_id} does not regress {base_id}")


if __name__ == "__main__":
    main()
