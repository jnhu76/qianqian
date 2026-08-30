#!/usr/bin/env python3
"""Build + gate + compare one minimization stage in one step.

Full stage pipeline (unless the projected manifest already exists):

  1. build the stage's FULL import manifest archive (all derived TUs)
  2. link-audit that archive against the stage's own manifest
  3. project the manifest to the reachable closure (manifest-projected.json)
  4. clean rebuild from the projected manifest
  5. run the full gate
  6. compare behavior against the base stage

    python3 tools/minimize_run_stage.py --stage s3-iconv --base s1
"""
from __future__ import annotations

import argparse
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def run(cmd: list[str]) -> None:
    print("+", " ".join(map(str, cmd)), flush=True)
    p = subprocess.run(list(map(str, cmd)), cwd=ROOT)
    if p.returncode:
        raise SystemExit(f"step failed ({p.returncode}): {' '.join(map(str, cmd))}")

def clean_build(av_manifest: str) -> None:
    run(["xmake", "f", "-m", "release", f"--av_manifest={av_manifest}", "-y"])
    shutil.rmtree(ROOT / "build" / "xmake", ignore_errors=True)
    artifacts = ROOT / "build" / "artifacts"
    if artifacts.exists():
        shutil.rmtree(artifacts)
    run(["xmake", "build", "qn_pcm_dump"])


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--stage", required=True)
    ap.add_argument("--base", default="s0")
    ap.add_argument("--skip-project", action="store_true",
                    help="reuse existing manifest-projected.json")
    args = ap.parse_args()

    stage_rel = f"build/minimize/{args.stage}"
    stage_dir = ROOT / stage_rel
    full_manifest = stage_dir / "manifest.json"
    projected = stage_dir / "manifest-projected.json"
    if not projected.is_file() and not args.skip_project:
        if not full_manifest.is_file():
            raise SystemExit(f"missing {full_manifest}; run the import step first")

    if not projected.is_file() and not args.skip_project:
        # 1-2: full archive + its own reachability audit
        clean_build(f"{stage_rel}/manifest.json")
        run([sys.executable, "tools/link_audit.py",
             "--manifest", f"{stage_rel}/manifest.json",
             "--out", stage_dir])
        # 3: project the stage manifest to the reachable closure
        run([sys.executable, "tools/minimize_manifest.py",
             "--stage", args.stage,
             "--audit", f"{stage_rel}/reachable-objects.json",
             "--base-manifest", f"{stage_rel}/manifest.json",
             "--out", f"{stage_rel}/manifest-projected.json"])

    # 4: clean rebuild from the projected closure
    clean_build(f"{stage_rel}/manifest-projected.json")
    # 5-6
    run([sys.executable, "tools/minimize_gate.py", "--stage", args.stage])
    run([sys.executable, "tools/minimize_compare.py", args.base, args.stage])


if __name__ == "__main__":
    main()
