#!/usr/bin/env python3
"""Machine byte attribution for a capability increment (C0 -> C1 by default).

Answers "where do the +548 KiB actually live" from the shipping artifact
itself: parses the GNU ld -Map of the size-minimal .so links (contributory
input-section lines contributed by libqianqian_av.a members) and groups LIVE
bytes per member family:

  mov/isom      MOV/ISOM demuxer family        (libavformat mov*.o, isom*.o)
  adts-demux    raw ADTS demuxer               (libavformat aac.o)
  aac-decoder   AAC decoder family             (libavcodec aac*.o except aac.o)
  shared        everything else in the archive (containers/dsp/util support)

Only actually-contributed (post gc-sections) section bytes are counted —
object-file sizes or TU counts are explicitly NOT evidence.

    python3 tools/common_attribution.py --base c0-so --stage c1-so
"""
from __future__ import annotations

import argparse
import json
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "bench" / "results" / "common-formats"

# contribution line: ' .text.foo  0x0000000000001234  0x1a0  <path>(<member>)'
CONTRIB = re.compile(
    r"^\s+\S+\s+0x[0-9a-f]+\s+0x([0-9a-f]+)\s+\S+\(([^)]+\.o)\)\s*$")


def family(member: str) -> str:
    if member.startswith("mov") or member.startswith("isom"):
        return "mov/isom"
    if member == "aac.o":        # libavformat's ADTS demuxer
        return "adts-demux"
    if member.startswith("aac"):
        return "aac-decoder"
    return "shared"


def group_bytes(map_path: Path) -> dict[str, int]:
    per_member: dict[str, int] = {}
    for line in map_path.read_text(errors="replace").splitlines():
        m = CONTRIB.match(line)
        if not m:
            continue
        size, member = int(m.group(1), 16), m.group(2)
        per_member[member] = per_member.get(member, 0) + size
    groups: dict[str, int] = {}
    for member, size in per_member.items():
        groups[family(member)] = groups.get(family(member), 0) + size
    groups["__members__"] = len(per_member)
    groups["__total_bytes__"] = sum(v for k, v in groups.items() if not k.startswith("__"))
    return groups


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--base", default="c0-so")
    ap.add_argument("--stage", default="c1-so")
    ap.add_argument("--out", default=str(OUT / "attribution.json"))
    args = ap.parse_args()

    base_map = ROOT / "build" / "minimize" / args.base / "so.map"
    stage_map = ROOT / "build" / "minimize" / args.stage / "so.map"
    for p in (base_map, stage_map):
        if not p.is_file():
            raise SystemExit(f"missing {p}; the .so stage must be linked with -Wl,-Map=")
    base_g, stage_g = group_bytes(base_map), group_bytes(stage_map)

    families = ("mov/isom", "adts-demux", "aac-decoder", "shared")
    rows = []
    for fam in families:
        b, s = base_g.get(fam, 0), stage_g.get(fam, 0)
        rows.append({
            "family": fam,
            "base_live_bytes": b,
            "stage_live_bytes": s,
            "delta_live_bytes": s - b,
        })
    result = {
        "schema": 1,
        "method": "GNU ld -Map contributory input-section bytes per archive member family, "
                  "post gc-sections (live shipping bytes, not object-file or TU counts)",
        "base": {"stage": args.base, "map": str(base_map.relative_to(ROOT)),
                 "members": base_g["__members__"], "groups": rows},
        "stage": {"stage": args.stage, "map": str(stage_map.relative_to(ROOT)),
                  "members": stage_g["__members__"]},
        "increment_attribution": rows,
        "note": "shared covers container/codec/util support shared with pre-existing "
                "capabilities; only the mov/isom + adts-demux + aac-decoder families "
                "are attributable to the AAC/M4A/ADTS capability bundle",
    }
    out = Path(args.out)
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")

    total_delta = stage_g["__total_bytes__"] - base_g["__total_bytes__"]
    print(f"{'family':<14}{'base':>10}{'stage':>10}{'delta':>10}")
    for r in rows:
        print(f"{r['family']:<14}{r['base_live_bytes']:>10,}{r['stage_live_bytes']:>10,}"
              f"{r['delta_live_bytes']:>+10,}")
    print(f"{'TOTAL':<14}{base_g['__total_bytes__']:>10,}{stage_g['__total_bytes__']:>10,}"
          f"{total_delta:>+10,}")
    print(f"wrote {out}")


if __name__ == "__main__":
    main()
