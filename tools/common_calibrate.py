#!/usr/bin/env python3
"""Pin OBSERVED seek semantics per format family into the common corpus.

Seek strictness must not be guessed: a lossy decoder with cross-packet state
(bit reservoir) can legitimately differ after a seek. This tool runs the
bench-level contract on the FULL Common Formats build (stage c5's projected
closure) and, per format family, records:

    strict  — every clean case suffix-matches the sequential decode at
              25/50/75% with frame-aligned resume; the gate then enforces it
    record  — at least one case mismatches; recorded-and-compared only

Truncated/malformed cases keep "record"/"none" as authored. The updated
manifest is committed; the clean-room ladder re-derives the same observation
and fails loudly if the committed pin ever drifts.

    python3 tools/common_calibrate.py --stage prepare-c5
"""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tools"))
from common_corpus import bench_json, load_cases, verify_fixtures  # noqa: E402


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--stage", default="prepare-c5",
                    help="stage dir whose build tree holds the full-capability qn_bench")
    ap.add_argument("--apply", action="store_true",
                    help="write the observed pins into the manifest (default: report only)")
    args = ap.parse_args()

    verify_fixtures()
    from common_corpus import build_bench
    stage = ROOT / "build" / "minimize" / args.stage
    single = ROOT / "build" / "artifacts" / "libqianqian_av.a"
    if not single.is_file():
        raise SystemExit("missing build/artifacts/libqianqian_av.a; build the stage first")
    exe = stage / "qn_bench"
    build_bench(exe, [stage / "oracle", ROOT / "build" / "ffmpeg-src"], [single])

    manifest_path = ROOT / "corpus" / "manifest" / "common-formats.json"
    manifest = json.loads(manifest_path.read_text())

    evidence: dict[str, dict] = {}
    for case in manifest["cases"]:
        if case["degraded"]:
            continue
        rc, obs = bench_json(exe, ROOT / "corpus" / "fixtures" / case["file"])
        if rc != 0 or obs.get("status") != "ok":
            raise SystemExit(f"calibration: clean case {case['id']} failed to decode: {obs}")
        matches = [s.get("suffix_match_sequential") for s in obs.get("seeks", [])]
        aligned = all(
            s.get("target_sample") is not None
            and s.get("target_sample") - 65536 < s.get("resume_sample", -1) <= s.get("target_sample")
            for s in obs.get("seeks", []))
        evidence[case["id"]] = {
            "capability": case["capability"],
            "suffix_matches": matches,
            "resume_frame_aligned": aligned,
            "strict_eligible": len(matches) == 3 and all(matches) and aligned,
        }

    # family verdict: strict only if EVERY clean case of the family qualifies
    families: dict[str, bool] = {}
    for case_id, ev in sorted(evidence.items()):
        fam = ev["capability"]
        families[fam] = families.get(fam, True) and ev["strict_eligible"]

    changed = []
    for case in manifest["cases"]:
        if case["degraded"]:
            continue
        want = "strict" if families.get(case["capability"], False) else "record"
        if case["expect"].get("seek") != want:
            changed.append(f"{case['id']}: {case['expect'].get('seek')} -> {want}")
        case["expect"]["seek"] = want
        case["expect"]["seek_evidence"] = {
            "method": "bench suffix-match at 25/50/75% vs sequential decode",
            "observed": evidence[case["id"]],
            "family_verdict": "strict" if families.get(case["capability"]) else "record",
        }

    print("family verdicts:")
    for fam, ok in sorted(families.items()):
        print(f"  {fam:8s} -> {'strict' if ok else 'record'}")
    if changed:
        print("changed pins:")
        for line in changed:
            print(f"  {line}")
    else:
        print("all pins unchanged")
    if args.apply:
        manifest_path.write_text(json.dumps(manifest, indent=2, ensure_ascii=False) + "\n")
        print(f"applied to {manifest_path.relative_to(ROOT)}")
    else:
        print("(report only; pass --apply to write)")


if __name__ == "__main__":
    main()
