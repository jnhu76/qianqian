#!/usr/bin/env python3
"""Generate a minimized FFmpeg compile-closure manifest from a link audit.

Reads the S1 audit (build/minimize/<stage>/reachable-objects.json) and the
pristine import manifest (build/ffmpeg-xmake/manifest.json), then emits a
manifest that replays ONLY the link-reachable translation units through the
same Xmake machinery. The pristine FFmpeg tree is never modified; candidate
closures are pure manifest projections.

Output manifest schema matches schema 1 (plus minimization provenance), so
xmake's qianqian_av target consumes it unchanged via --av_manifest=...
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
    ap.add_argument("--stage", required=True, help="stage id, e.g. s1")
    ap.add_argument("--audit", default=None, help="reachable-objects.json to derive from")
    ap.add_argument("--base-manifest", default="build/ffmpeg-xmake/manifest.json")
    ap.add_argument("--out", default=None,
                    help="default: build/minimize/<stage>/manifest-projected.json")
    args = ap.parse_args()

    audit_path = (Path(args.audit) if args.audit else ROOT / "build" / "minimize" / args.stage / "reachable-objects.json").resolve()
    out_path = (Path(args.out) if args.out else ROOT / "build" / "minimize" / args.stage / "manifest-projected.json").resolve()
    base = json.loads((ROOT / args.base_manifest).read_text())
    audit = json.loads(audit_path.read_text())

    pulled_units = {e["unit_object"] for e in audit if e["pulled"]}
    units = [u for u in base["units"] if u["object"] in pulled_units]
    if len(units) != len(pulled_units):
        missing = pulled_units - {u["object"] for u in units}
        raise SystemExit(f"audit references units missing from manifest: {sorted(missing)}")

    try:
        audit_rel = str(audit_path.resolve().relative_to(ROOT))
    except ValueError:
        audit_rel = str(audit_path)
    manifest = dict(base)
    manifest["units"] = units
    manifest["minimization"] = {
        "stage": args.stage,
        "derived_from_manifest": args.base_manifest,
        "derived_from_manifest_sha256": sha256_file(Path(args.base_manifest) if not Path(args.base_manifest).is_absolute() else Path(args.base_manifest)),
        "audit": audit_rel,
        "audit_sha256": sha256_file(audit_path),
        "method": "link-reachability projection; candidate closure, gated by corpus/PCM/seek/real-song evidence",
        "status": "CANDIDATE_PENDING_FULL_GATE",
    }
    manifest["closure"] = {
        "translation_units": len(units),
        "upstream_sources": sum(1 for u in units if u["origin"] == "source"),
        "generated_sources": sum(1 for u in units if u["origin"] == "generated"),
        "base_translation_units": base["closure"]["translation_units"],
    }
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")
    print(f"wrote {out_path.relative_to(ROOT)}: {len(units)}/{base['closure']['translation_units']} units")


if __name__ == "__main__":
    main()
