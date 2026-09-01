#!/usr/bin/env python3
"""Negative gate test: a manifest derived for one target must not satisfy a
build session for another (target provenance is fail-closed).

Mutates the canonical manifest's target identity to windows-x86_64, points a
scratch Xmake session at it, and requires the build to FAIL with the
mismatch instruction. As a positive control, the pristine manifest builds in
the same scratch session. Everything lives under build/ (regenerable).
"""
import json
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
CANONICAL = ROOT / "build" / "ffmpeg-xmake" / "manifest.json"
SCRATCH_CFG = ROOT / "build" / "target-gate-test"
SCRATCH_BLD = "build/xmake-target-gate"


def xmake(args, check=True):
    p = subprocess.run(["xmake", *args], cwd=ROOT, capture_output=True, text=True)
    if check and p.returncode:
        raise SystemExit(f"xmake {' '.join(args)} failed unexpectedly:\n"
                         f"{p.stdout[-2000:]}\n{p.stderr[-2000:]}")
    return p


def with_manifest(mutate_target=None):
    work = SCRATCH_CFG
    shutil.rmtree(work, ignore_errors=True)
    work.mkdir(parents=True)
    manifest = json.loads(CANONICAL.read_text())
    if mutate_target:
        manifest["target"]["id"] = mutate_target
    path = work / "manifest.json"
    path.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")
    return path


def main():
    if not CANONICAL.is_file():
        raise SystemExit("missing canonical manifest; run `xmake ffmpeg-import`")

    # --- negative: windows manifest must not satisfy the linux session ----
    mutated = with_manifest(mutate_target="windows-x86_64")
    xmake(["f", "-o", SCRATCH_BLD, "-m", "release",
           f"--av_manifest={mutated.relative_to(ROOT)}", "-y"])
    p = xmake(["build", "qianqian_av"], check=False)
    combined = p.stdout + p.stderr
    if p.returncode == 0:
        raise SystemExit("GATE BROKEN: a windows-x86_64 manifest satisfied "
                         "the linux-x86_64 session")
    if "manifest target mismatch" not in combined:
        raise SystemExit("build failed, but not by the target-identity gate:\n"
                         f"{combined[-2000:]}")
    print("negative gate: windows-x86_64 manifest rejected by the linux "
          "session (fail-closed) — PASS")

    # --- positive control: the pristine manifest builds the same session --
    pristine = with_manifest()
    xmake(["f", "-o", SCRATCH_BLD, "-m", "release",
           f"--av_manifest={pristine.relative_to(ROOT)}", "-y"])
    xmake(["build", "qianqian_av"])
    print("positive control: linux-x86_64 manifest replayed — PASS")

    shutil.rmtree(SCRATCH_BLD, ignore_errors=True)
    shutil.rmtree(SCRATCH_CFG, ignore_errors=True)
    print("target identity gate: PASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
