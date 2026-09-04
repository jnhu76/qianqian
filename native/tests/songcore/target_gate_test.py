#!/usr/bin/env python3
"""Negative gate test: a manifest derived for one target must not satisfy a
build session for another (target provenance is fail-closed, identity-bound).

Mutates the canonical manifest's target identity and requires the Xmake
build to FAIL with the mismatch instruction:
  1. relabel: linux manifest claimed as windows-mingw-x86_64 -> mismatch,
  2. toolchain drift: cross_prefix from another toolchain family -> stale/
     foreign toolchain,
  3. platform: manifest platform drifted from the recipe -> identity mismatch.
(recipe_sha256 staleness is NOT asserted here: the xmake sandbox cannot
hash files. It is enforced by the pinning validator in consumers.py and
proven by the wasm-sha / windows-identity mutations.)
As a positive control, the pristine manifest builds in the same scratch
session. Everything lives under build/ (regenerable).
"""
import json
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
CANONICAL = ROOT / "build" / "ffmpeg-xmake" / "manifest.json"
SCRATCH_CFG = ROOT / "build" / "target-gate-test"
SCRATCH_BLD = "build/xmake-target-gate"


def xmake(args, check=True):
    p = subprocess.run(["xmake", *args], cwd=ROOT, capture_output=True, text=True)
    if check and p.returncode:
        raise SystemExit(f"xmake {' '.join(args)} failed unexpectedly:\n"
                         f"{p.stdout[-2000:]}\n{p.stderr[-2000:]}")
    return p


def with_manifest(mutate=None):
    work = SCRATCH_CFG
    shutil.rmtree(work, ignore_errors=True)
    work.mkdir(parents=True)
    manifest = json.loads(CANONICAL.read_text())
    if mutate:
        mutate(manifest)
    path = work / "manifest.json"
    path.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")
    return path


def require_rejected(label, needle):
    xmake(["f", "-o", SCRATCH_BLD, "-m", "release",
           f"--av_manifest={with_manifest(label[0]).relative_to(ROOT)}", "-y"])
    p = xmake(["build", "qianqian_av"], check=False)
    combined = p.stdout + p.stderr
    if p.returncode == 0:
        raise SystemExit(f"GATE BROKEN: {label[1]} satisfied the "
                         f"linux-x86_64 session")
    if needle not in combined:
        raise SystemExit(f"build failed, but not by the {needle!r} gate:\n"
                         f"{combined[-2000:]}")
    print(f"negative gate: {label[1]} rejected (fail-closed) — PASS")


def main():
    if not CANONICAL.is_file():
        raise SystemExit("missing canonical manifest; run `xmake ffmpeg-import`")

    require_rejected(
        (lambda m: m["target"].__setitem__("id", "windows-mingw-x86_64"),
         "a windows-mingw-x86_64-labeled manifest"),
        "manifest target mismatch")
    require_rejected(
        (lambda m: m["target"].__setitem__("target_os", "mingw32"),
         "a manifest with a foreign-toolchain target_os"),
        "stale or from another toolchain family")
    require_rejected(
        (lambda m: m["target"].__setitem__("platform", "msdos"),
         "a manifest with drifted platform identity"),
        "manifest target identity mismatch")

    # --- positive control: the pristine manifest builds the same session --
    xmake(["f", "-o", SCRATCH_BLD, "-m", "release",
           f"--av_manifest={with_manifest(None).relative_to(ROOT)}", "-y"])
    xmake(["build", "qianqian_av"])
    print("positive control: linux-x86_64 manifest replayed — PASS")

    shutil.rmtree(SCRATCH_BLD, ignore_errors=True)
    shutil.rmtree(SCRATCH_CFG, ignore_errors=True)
    print("target identity gate: PASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
