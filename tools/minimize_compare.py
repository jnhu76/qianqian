#!/usr/bin/env python3
"""Compare two minimization stage gates.

Behavior contract: seek probes, real-song PCM decodes, and the corpus verify
report must be IDENTICAL between stages (modulo recorded timings and ASLR
pointer text). Sizes and throughput are reported as deltas, never gated on
equality.
"""
from __future__ import annotations

import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tools"))
from minimize_gate import strip_measurements  # noqa: E402


def load(stage: str) -> dict:
    return json.loads((ROOT / "build" / "minimize" / stage / "gate.json").read_text())


def behavior(stage: str, gate: dict) -> dict:
    verify = strip_measurements(gate["verify"])
    verify.pop("single_archive_bytes", None)
    return {
        "verify": verify,
        "seek_probe": strip_measurements(gate["seek_probe"]),
        "real_song_pcm": gate["real_song_pcm"],
        "strict_failures": gate["strict_failures"],
    }


def main() -> None:
    base_id, cand_id = sys.argv[1], sys.argv[2]
    base, cand = load(base_id), load(cand_id)

    b, c = behavior(base_id, base), behavior(cand_id, cand)
    diffs = []
    for section in b:
        if b[section] != c[section]:
            diffs.append(section)

    bs, cs = base["sizes"], cand["sizes"]
    print(f"stage {base_id} -> {cand_id}")
    print(f"  archive: {bs['libqianqian_av_a_bytes']:,} -> {cs['libqianqian_av_a_bytes']:,} "
          f"({cs['libqianqian_av_a_bytes'] - bs['libqianqian_av_a_bytes']:+,})")
    print(f"  members: {bs['libqianqian_av_members']} -> {cs['libqianqian_av_members']}")
    print(f"  symbols: {bs['libqianqian_av_symbols']} -> {cs['libqianqian_av_symbols']}")
    print(f"  archive xz: {bs['libqianqian_av_xz_bytes']:,} -> {cs['libqianqian_av_xz_bytes']:,} "
          f"({cs['libqianqian_av_xz_bytes'] - bs['libqianqian_av_xz_bytes']:+,})")
    print(f"  linked: {bs['qn_pcm_dump_bytes']:,} -> {cs['qn_pcm_dump_bytes']:,} "
          f"({cs['qn_pcm_dump_bytes'] - bs['qn_pcm_dump_bytes']:+,})")
    print(f"  linked xz: {bs['qn_pcm_dump_stripped_xz_bytes']:,} -> {cs['qn_pcm_dump_stripped_xz_bytes']:,} "
          f"({cs['qn_pcm_dump_stripped_xz_bytes'] - bs['qn_pcm_dump_stripped_xz_bytes']:+,})")
    for rel, t in cand["throughput"].items():
        prev = base["throughput"].get(rel, {})
        print(f"  xrt {rel.split('/')[-1]:32s} "
              f"{prev.get('xrt_songcore_output', 0):8.1f} -> {t['xrt_songcore_output']:8.1f}")
    if diffs:
        print(f"  BEHAVIOR DRIFT in: {diffs}")
        sys.exit(1)
    print("  behavior: IDENTICAL (corpus/PCM/seek/real-song)")
    print(f"  VERDICT: {cand_id} equivalent to {base_id}")


if __name__ == "__main__":
    main()
