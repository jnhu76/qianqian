#!/usr/bin/env python3
"""Produce a flags-augmented projection of a stage manifest (S4/S5).

    python3 tools/minimize_flags.py --stage s4-fdata --from-stage s3-pthreads \
        --add-flag "-ffunction-sections" --add-flag "-fdata-sections"

Reads the source stage's manifest-projected.json and emits
build/minimize/<stage>/manifest-projected.json with the extra flags appended
to every unit (idempotent). The source closure is unchanged: this only alters
codegen granularity, so the gate must remain byte-identical.
"""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for block in iter(lambda: f.read(1 << 20), b""):
            h.update(block)
    return h.hexdigest()


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--stage", required=True)
    ap.add_argument("--from-stage", default=None,
                    help="stage whose manifest(-projected).json is the source closure")
    ap.add_argument("--from-manifest", default=None,
                    help="arbitrary manifest path (e.g. the canonical import manifest)")
    ap.add_argument("--add-flag", action="append", default=[])
    ap.add_argument("--replace-opt", metavar="OPT",
                    help="replace the existing -O<level> with -OPT (e.g. Os)")
    ap.add_argument("--keep-manifest", action="store_true",
                    help="write manifest.json instead of manifest-projected.json")
    args = ap.parse_args()
    if bool(args.from_stage) == bool(args.from_manifest):
        raise SystemExit("exactly one of --from-stage / --from-manifest is required")

    if args.from_manifest:
        src = ROOT / args.from_manifest
    else:
        src = ROOT / "build" / "minimize" / args.from_stage / "manifest-projected.json"
        if not src.is_file():
            src = ROOT / "build" / "minimize" / args.from_stage / "manifest.json"
    manifest = json.loads(src.read_text())

    extra = list(args.add_flag)
    replaced = []
    for unit in manifest["units"]:
        flags = unit.get("flags") or []
        if args.replace_opt:
            flags = [f for f in flags if not (f.startswith("-O") and f != "-O0")]
            flags = [f for f in flags if f not in replaced] if replaced else flags
            if args.replace_opt not in replaced:
                replaced.append(args.replace_opt)
            flags = flags + [f"-{args.replace_opt}"]
        for f in extra:
            if f not in flags:
                flags = flags + [f]
        unit["flags"] = flags

    manifest["flag_mutation"] = {
        "stage": args.stage,
        "derived_from": str(src.relative_to(ROOT)),
        "derived_from_sha256": sha256_file(src),        "added_flags": extra,
        "replaced_opt": args.replace_opt,
        "reason": "S4/S5 codegen experiment; source closure unchanged, gate must remain byte-identical",
    }
    out = ROOT / "build" / "minimize" / args.stage / (
        "manifest.json" if args.keep_manifest else "manifest-projected.json")
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")
    print(f"wrote {out.relative_to(ROOT)} ({len(manifest['units'])} units, +{extra})")


if __name__ == "__main__":
    main()
