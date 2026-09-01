#!/usr/bin/env python3
"""Derive a target-specific FFmpeg compile closure from intent + target recipe.

Normal conceptual invocation — target in, manifest out:

    python3 tools/ffmpeg_profile_import.py \
        --target linux-x86_64 \
        --profile ffmpeg/profiles/codec-base.json

writes build/manifests/<target>/<profile>/manifest.json. The FFmpeg
configure/Make oracle runs ONCE for this (profile, target) pair; the target's
configure facts come from ffmpeg/targets/<target>.json, not from hand-typed
--configure-extra. Xmake replays the manifest into the platform artifact and
fails closed when the pointed manifest was derived for a different target.

--stage remains for test/DSP capability stages (build/minimize/<stage>/);
those still bind the native host target identity so the Xmake gate can
verify them.
"""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tools"))
import ffmpeg_import as fi  # noqa: E402


def validate_profile(profile: dict, source: Path) -> None:
    """Capability-intent sanity: contradictions must fail fast, before the
    expensive oracle run. A library enabled and disabled in the same profile
    only 'works' while configure argument ordering happens to favor the last
    one — that is fragility, not intent."""
    problems = []

    def norm(names) -> list[str]:
        out = []
        for n in names or []:
            if n not in out:
                out.append(n)
            else:
                problems.append(f"duplicate entry '{n}'")
        return out

    libs = profile.get("libraries", {})
    enabled = norm(libs.get("enable", []))
    disabled = norm(libs.get("disable", []))
    clash = sorted(set(enabled) & set(disabled))
    if clash:
        problems.append(f"libraries both enabled and disabled: {clash}")
    if "everything-disabled" == profile.get("component_base") and enabled:
        pass  # libraries may still be re-enabled explicitly on the disabled base
    components = profile.get("components", {})
    for cls, names in components.items():
        norm(names)
    if problems:
        raise SystemExit(
            f"profile {source.name} failed validation:\n  - " +
            "\n  - ".join(problems))


def main() -> None:
    ap = argparse.ArgumentParser()
    mode = ap.add_mutually_exclusive_group(required=True)
    mode.add_argument("--target", help="target recipe id, e.g. linux-x86_64")
    mode.add_argument("--stage", help="legacy test/DSP stage id (native host target)")
    ap.add_argument("--profile", required=True, help="capability profile JSON path")
    ap.add_argument("--configure-extra", action="append", default=[],
                    help="extra configure argument (SDK toolchain selection "
                         "for cross oracles; use env/PATH-relative values)")
    ap.add_argument("--force", action="store_true",
                    help="re-derive the closure even if this stage's manifest exists")
    args = ap.parse_args()

    profile_path = Path(args.profile)
    if not profile_path.is_absolute():
        profile_path = ROOT / profile_path
    profile = json.loads(profile_path.read_text())
    validate_profile(profile, profile_path)

    if args.target:
        recipe = fi.load_recipe(args.target)
        stage_dir = ROOT / "build" / "manifests" / recipe["id"] / profile["profile"]
        target_args = fi.recipe_configure_args(recipe)
    else:
        recipe = fi.find_native_recipe()
        stage_dir = ROOT / "build" / "minimize" / args.stage
        target_args = []

    manifest_path = stage_dir / "manifest.json"
    if manifest_path.is_file() and not args.force:
        print(f"{stage_dir.name}: manifest exists, skipping import "
              f"(use --force to re-derive)")
        return

    # Profiles that enable swresample (the Opus decoder's upstream build
    # dependency) must also make its archive an oracle build target, or its
    # translation units would never enter the replay manifest. Same for
    # libavfilter (DSP capability ladder).
    extra_targets = []
    if "swresample" in profile.get("libraries", {}).get("enable", []):
        extra_targets.append("libswresample/libswresample.a")
    if "avfilter" in profile.get("libraries", {}).get("enable", []):
        extra_targets.append("libavfilter/libavfilter.a")
    for t in extra_targets:
        if t not in fi.LIB_TARGETS:
            fi.LIB_TARGETS = fi.LIB_TARGETS + (t,)

    fi.PROFILE = profile_path
    fi.OUT = stage_dir
    fi.ORACLE = stage_dir / "oracle"
    fi.MANIFEST = manifest_path

    # fi module globals carry the stage's profile/oracle paths; the oracle run
    # itself lives here so target-specific oracles can add configure arguments
    # while keeping the deterministic ordering (profile -> recipe -> extra).
    pin = json.loads(fi.PIN.read_text())
    fi.verified_source(pin)

    import os
    import shutil
    oracle = stage_dir / "oracle"
    shutil.rmtree(oracle, ignore_errors=True)
    oracle.mkdir(parents=True)
    cmd = [str(fi.SRC / "configure"), *fi.configure_args(profile),
           *target_args, *args.configure_extra]
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
    cargs = fi.configure_args(profile) + target_args + list(args.configure_extra)
    toolchain = fi.toolchain_identity()

    refs = {}
    for rel in fi.LIB_TARGETS:
        archive = fi.ORACLE / rel
        if not archive.is_file():
            raise SystemExit(f"oracle archive missing: {archive}")
        refs[rel] = {"bytes": archive.stat().st_size, "sha256": fi.sha256_file(archive)}

    source_units = sum(u["origin"] == "source" for u in units)
    manifest = {
        "schema": 2,
        "stage": args.stage or recipe["id"],
        "ffmpeg_tag": pin["ffmpeg_tag"],
        "ffmpeg_commit_sha": pin["ffmpeg_commit_sha"],
        "ffmpeg_source_sha256": pin["source_sha256"],
        "profile": profile["profile"],
        "profile_variant": profile.get("variant"),
        "profile_sha256": fi.sha256_file(profile_path),
        "target": fi.target_identity(
            recipe, fi.sha256_file(fi.TARGETS / f"{recipe['id']}.json")),
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
    print(f"target {recipe['id']} | {args.stage or profile['profile']} closure: "
          f"{len(units)} translation units "
          f"({len(units) - source_units} generated)")
    print(f"toolchain: {toolchain}")
    if configure_warnings:
        print("configure warnings (capability demotions):")
        for w in configure_warnings:
            print(f"  ! {w}")


if __name__ == "__main__":
    main()
