#!/usr/bin/env python3
"""Run ONE configure-dimension deletion experiment (S3).

Each call builds exactly one variant of the canonical n3-min-noswr profile
with a single added --disable dimension, derives a FRESH source closure from
the upstream configure/Make oracle for that variant (never reuses the previous
closure list), and writes a replayable manifest under
build/minimize/<stage>/manifest.json.

The experiment harness then rebuilds via Xmake and runs the full gate; this
tool only performs the machine-derived import step.
"""
from __future__ import annotations

import argparse
import json
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tools"))
import ffmpeg_import as fi  # noqa: E402


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--stage", required=True, help="e.g. s3-iconv")
    ap.add_argument("--add-disable", action="append", default=[],
                    help="configure feature to disable (repeatable, but one experiment each)")
    ap.add_argument("--on-top-of", default=None, metavar="STAGE",
                    help="stack this variant on another stage's accepted profile "
                         "(default: canonical bench/profiles/n3-min-noswr.json)")
    ap.add_argument("--description", default="")
    args = ap.parse_args()

    stage_dir = ROOT / "build" / "minimize" / args.stage
    stage_dir.mkdir(parents=True, exist_ok=True)

    if args.on_top_of:
        base_path = ROOT / "build" / "minimize" / args.on_top_of / "profile.json"
    else:
        base_path = fi.PROFILE
    base = json.loads(base_path.read_text())
    variant = json.loads(json.dumps(base))  # deep copy
    variant["disable"] = sorted(set(base.get("disable", [])) | set(args.add_disable))
    variant["variant"] = {
        "id": args.stage,
        "base_profile": str(base_path),
        "base_profile_sha256": fi.sha256_file(base_path),
        "added_disable": args.add_disable,
        "description": args.description,
    }

    variant_path = stage_dir / "profile.json"
    variant_path.write_text(json.dumps(variant, indent=2, sort_keys=True) + "\n")

    fi.PROFILE = variant_path
    fi.OUT = stage_dir
    fi.ORACLE = stage_dir / "oracle"
    fi.MANIFEST = stage_dir / "manifest.json"
    fi.main()


if __name__ == "__main__":
    main()
