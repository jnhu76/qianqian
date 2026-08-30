#!/usr/bin/env python3
"""Run one Common Formats ladder stage end to end (issue #8).

Pipeline per stage (all closure derivation is machine-driven; nothing is
hand-added to a previous stage's file list):

  1. import   capability intent -> FFmpeg configure/Make oracle -> manifest
              (build/minimize/<stage>/manifest.json, full closure)
  2. build    replay the FULL closure through Xmake
  3. audit    GNU ld link-reachability over the full archive
              (tools/link_audit.py, -Map multiset hard gate)
  4. project  manifest projection to the reachable closure
  5. rebuild  clean replay of the projected closure
  6. gate     full behavior + size gate (tools/common_gate.py)

    python3 tools/common_stage.py --stage c1
"""
from __future__ import annotations

import argparse
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tools"))


def run(cmd: list[str]) -> None:
    print("+", " ".join(map(str, cmd)), flush=True)
    p = subprocess.run(list(map(str, cmd)), cwd=ROOT)
    if p.returncode:
        raise SystemExit(f"step failed ({p.returncode}): {' '.join(map(str, cmd))}")


def clean_build(av_manifest: str, *, gc: bool = False, lto: bool = False) -> None:
    run(["xmake", "f", "-m", "release", f"--av_manifest={av_manifest}",
         f"--gc_sections={'y' if gc else 'n'}", f"--lto={'y' if lto else 'n'}", "-y"])
    shutil.rmtree(ROOT / "build" / "xmake", ignore_errors=True)
    shutil.rmtree(ROOT / "build" / "artifacts", ignore_errors=True)
    run(["xmake", "build", "qn_pcm_dump"])


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--stage", required=True)
    ap.add_argument("--skip-import", action="store_true")
    args = ap.parse_args()

    from common_corpus import STAGE_PROFILES
    profile = STAGE_PROFILES[args.stage]
    stage_rel = f"build/minimize/{args.stage}"
    stage_dir = ROOT / stage_rel

    if not args.skip_import:
        run([sys.executable, "tools/common_import.py", "--stage", args.stage,
             "--profile", profile])

    # 2: full-closure archive
    clean_build(f"{stage_rel}/manifest.json")
    # 3-4: reachability audit + manifest projection
    run([sys.executable, "tools/link_audit.py",
         "--manifest", f"{stage_rel}/manifest.json", "--out", stage_dir])
    run([sys.executable, "tools/minimize_manifest.py",
         "--stage", args.stage,
         "--audit", f"{stage_rel}/reachable-objects.json",
         "--base-manifest", f"{stage_rel}/manifest.json",
         "--out", f"{stage_rel}/manifest-projected.json"])
    # 5: clean rebuild from the projected closure
    clean_build(f"{stage_rel}/manifest-projected.json")
    # 6: full gate
    run([sys.executable, "tools/common_gate.py", "--stage", args.stage])


if __name__ == "__main__":
    main()
