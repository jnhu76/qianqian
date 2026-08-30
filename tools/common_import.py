#!/usr/bin/env python3
"""Import an arbitrary capability profile as a Common Formats ladder stage.

Like tools/config_experiment.py, but takes a full profile path instead of
mutating the canonical one: the Common Formats ladder needs complete
component intent per stage (c1..c5), not one-configure-dimension deltas.

The FFmpeg configure/Make oracle runs ONCE for this stage's capability set;
the resulting compile closure is frozen into build/minimize/<stage>/manifest.
Normal builds replay the manifest through Xmake and never touch FFmpeg Make.

    python3 tools/common_import.py --stage c1 --profile bench/profiles/c1-aac.json
"""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tools"))
import ffmpeg_import as fi  # noqa: E402


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--stage", required=True, help="stage id, e.g. c1")
    ap.add_argument("--profile", required=True, help="capability profile JSON path")
    ap.add_argument("--configure-extra", action="append", default=[],
                    help="extra configure argument (target-specific oracles, e.g. Windows cross)")
    ap.add_argument("--force", action="store_true",
                    help="re-derive the closure even if this stage's manifest exists")
    args = ap.parse_args()

    stage_dir = ROOT / "build" / "minimize" / args.stage
    manifest_path = stage_dir / "manifest.json"
    if manifest_path.is_file() and not args.force:
        print(f"stage {args.stage}: manifest exists, skipping import "
              f"(use --force to re-derive)")
        return

    profile_path = Path(args.profile)
    if not profile_path.is_absolute():
        profile_path = ROOT / profile_path
    profile = json.loads(profile_path.read_text())

    # Profiles that enable swresample (the Opus decoder's upstream build
    # dependency) must also make its archive an oracle build target, or its
    # translation units would never enter the replay manifest.
    if "swresample" in profile.get("libraries", {}).get("enable", []):
        swr = "libswresample/libswresample.a"
        if swr not in fi.LIB_TARGETS:
            fi.LIB_TARGETS = fi.LIB_TARGETS + (swr,)

    fi.PROFILE = profile_path
    fi.OUT = stage_dir
    fi.ORACLE = stage_dir / "oracle"
    fi.MANIFEST = manifest_path

    # fi module globals carry the stage's profile/oracle paths; the oracle run
    # itself lives here so target-specific oracles can add configure arguments
    # (e.g. Windows cross flags) while keeping the deterministic ordering.
    pin = json.loads(fi.PIN.read_text())
    fi.verified_source(pin)

    import os
    import shutil
    oracle = stage_dir / "oracle"
    shutil.rmtree(oracle, ignore_errors=True)
    oracle.mkdir(parents=True)
    cmd = [str(fi.SRC / "configure"), *fi.configure_args(profile), *args.configure_extra]
    configure_log = fi.run(cmd, cwd=oracle, capture=True)
    (stage_dir / "configure.log").write_text(configure_log)
    # Capability-intent mismatches surface as configure demotions (e.g. a
    # decoder silently disabled because a dependency library is absent).
    configure_warnings = sorted({
        l.strip() for l in configure_log.splitlines()
        if "Disabled" in l and "because" in l})
    jobs = str(max(1, os.cpu_count() or 4))
    log = fi.run(["make", "-j", jobs, "V=1", *fi.LIB_TARGETS], cwd=oracle, capture=True)

    units = fi.closure_from_log(log)
    cargs = fi.configure_args(profile) + list(args.configure_extra)
    toolchain = fi.toolchain_identity()

    refs = {}
    for rel in fi.LIB_TARGETS:
        archive = fi.ORACLE / rel
        if not archive.is_file():
            raise SystemExit(f"oracle archive missing: {archive}")
        refs[rel] = {"bytes": archive.stat().st_size, "sha256": fi.sha256_file(archive)}

    source_units = sum(u["origin"] == "source" for u in units)
    manifest = {
        "schema": 1,
        "stage": args.stage,
        "ffmpeg_tag": pin["ffmpeg_tag"],
        "ffmpeg_commit_sha": pin["ffmpeg_commit_sha"],
        "profile": profile["profile"],
        "profile_variant": profile.get("variant"),
        "profile_sha256": fi.sha256_file(profile_path),
        "toolchain": toolchain,
        "source_root": "build/ffmpeg-src",
        "config_root": fi.ORACLE.relative_to(ROOT).as_posix(),
        "configure_args": cargs,
        "configure_warnings": configure_warnings,
        "closure": {
            "translation_units": len(units),
            "upstream_sources": source_units,
            "generated_sources": len(units) - source_units,
        },
        "reference_archives": refs,
        "units": units,
    }
    fi.assert_portable_manifest(manifest)
    stage_dir.mkdir(parents=True, exist_ok=True)
    manifest_path.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")
    (stage_dir / "oracle-build.log").write_text(log)

    print(f"wrote {manifest_path.relative_to(ROOT)}")
    print(f"stage {args.stage} closure: {len(units)} translation units "
          f"({len(units) - source_units} generated)")
    print(f"toolchain: {toolchain}")
    if configure_warnings:
        print("configure warnings (capability demotions):")
        for w in configure_warnings:
            print(f"  ! {w}")


if __name__ == "__main__":
    main()
