#!/usr/bin/env python3
"""Manifest union: codec closure + filter closure accounting (E10-C0).

Deterministic machine tool for:

    codec manifest (+) filter manifest = combined product accounting

Unit identity is (origin, normalized path). The tool detects conflicts and
FAILs instead of silently picking one. It also records why a naive
concatenation of separately-configured manifests is NOT a valid product when
generated config roots diverge (task §12): the shipping authority stays the
combined-profile configure oracle, which this tool cross-validates against
the codec closure.

  python3 tools/ffmpeg_manifest_union.py \
      --codec-manifest build/minimize/avf-c0/manifest.json \
      --combined-manifest build/minimize/avf-c8/manifest.json \
      --out bench/results/avfilter-minimize/manifest-union.json
"""
from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def unit_ident(u: dict) -> tuple:
    return (u["origin"], u["path"])


def cfg_lines(oracle_dir: Path) -> dict:
    out = {}
    for name in ("config.h", "config_components.h"):
        p = oracle_dir / name
        if p.is_file():
            for line in p.read_text(errors="replace").splitlines():
                m = re.fullmatch(r"#define (CONFIG_\w+) (0|1)", line.strip())
                if m:
                    out[f"{name}:{m.group(1)}"] = m.group(2)
    return out


def union(codec_manifest: Path, combined_manifest: Path) -> dict:
    codec = json.loads(codec_manifest.read_text())
    combined = json.loads(combined_manifest.read_text())

    C = {unit_ident(u): u for u in codec["units"]}
    K = {unit_ident(u): u for u in combined["units"]}
    shared = sorted(set(C) & set(K))
    codec_only = sorted(set(C) - set(K))
    filter_only = sorted(set(K) - set(C))

    conflicts = []
    for key in shared:
        if C[key].get("flags") != K[key].get("flags"):
            conflicts.append({
                "unit": key[1],
                "codec_flags": C[key].get("flags"),
                "combined_flags": K[key].get("flags"),
            })

    c0cfg = cfg_lines(codec_manifest.parent / "oracle")
    c8cfg = cfg_lines(combined_manifest.parent / "oracle")
    diff_keys = sorted(k for k in set(c0cfg) | set(c8cfg)
                       if c0cfg.get(k) != c8cfg.get(k))

    def by_library(keys):
        libs = sorted({k[1].split("/")[0] for k in keys})
        return {lib: sum(1 for k in keys if k[1].startswith(lib + "/"))
                for lib in libs}

    return {
        "unit_identity": "origin + normalized path (+ library context implied by path)",
        "codec_manifest": str(codec_manifest),
        "combined_manifest": str(combined_manifest),
        "codec_only_units": len(codec_only),
        "codec_only_detail": [k[1] for k in codec_only][:50],
        "filter_only_units": len(filter_only),
        "filter_only_by_library": by_library(filter_only),
        "shared_units": len(shared),
        "shared_by_library": by_library(shared),
        "combined_units": len(K),
        "flag_conflicts": conflicts,
        "config_macro_diff_count": len(diff_keys),
        "config_macro_diff_sample": diff_keys[:80],
        "config_root_note": (
            "separately-configured manifests have divergent generated config "
            "roots; when config macros differ the union of their TUs is NOT a "
            "valid product (task §12). The shipping authority is the "
            "combined-profile configure oracle manifest, cross-validated "
            "against the codec closure by this tool"),
        "validations": {
            "codec_subset_of_combined": len(codec_only) == 0,
            "shared_flag_conflicts": len(conflicts) == 0,
            "combined_same_ffmpeg_pin": (
                codec.get("ffmpeg_commit_sha") == combined.get("ffmpeg_commit_sha")),
            "config_roots_diverge": len(diff_keys) > 0,
        },
    }


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--codec-manifest", required=True)
    ap.add_argument("--combined-manifest", required=True)
    ap.add_argument("--out", required=True)
    args = ap.parse_args()

    data = union(Path(args.codec_manifest), Path(args.combined_manifest))
    data["verdict"] = ("PASS" if data["validations"]["codec_subset_of_combined"]
                       and data["validations"]["shared_flag_conflicts"]
                       and data["validations"]["combined_same_ffmpeg_pin"]
                       else "FAIL")
    out = Path(args.out)
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(data, indent=1, sort_keys=True) + "\n")
    print(f"wrote {out}")
    print(f"codec-only={data['codec_only_units']} filter-only={data['filter_only_units']} "
          f"shared={data['shared_units']} combined={data['combined_units']} "
          f"conflicts={len(data['flag_conflicts'])} verdict={data['verdict']}")
    return 0 if data["verdict"] == "PASS" else 1


if __name__ == "__main__":
    sys.exit(main())
