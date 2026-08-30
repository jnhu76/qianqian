#!/usr/bin/env python3
"""Collect the Windows phase results into bench/results/common-formats/windows.json.

Gathers: toolchain identity, the full-closure and projected-closure numbers
(oracle TU / archive members / reachable units / fixpoint evidence), the
static projected archive, both DLL variants (-Os and -Os+LTO) with their PE
gates, the canonical shipping artifact (smallest accepted variant by
stripped+xz — every variant must carry its own native correctness PASS), and
the native correctness report — including a cross-platform PCM comparison
against the Linux stage gates (exact sha equality where the decoder contract
demands it; recorded per-case otherwise).

    python3 tools/common_windows_summary.py
"""
from __future__ import annotations

import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tools"))
from common_corpus import STAGE_CAPABILITIES, load_cases  # noqa: E402

WIN = ROOT / "build" / "minimize" / "win-c6"
OUT = ROOT / "bench" / "results" / "common-formats"


def load(p: Path) -> dict:
    return json.loads(p.read_text())


def main() -> None:
    manifest = load(WIN / "manifest.json")
    reach = load(WIN / "reachability" / "report.json")
    projection = load(WIN / "projection.json")
    dll = load(WIN / "dll" / "dll.json")
    dll_lto_path = WIN / "dll-lto" / "dll.json"
    dll_lto = load(dll_lto_path) if dll_lto_path.is_file() else None
    correct = load(WIN / "correctness" / "correctness.json")
    correct_lto_path = WIN / "correctness" / "correctness-lto.json"
    correct_lto = load(correct_lto_path) if correct_lto_path.is_file() else None

    # canonical shipping artifact: min stripped+xz among ACCEPTED variants
    # (accepted = export/import gates passed at build time AND its own
    # native correctness PASS; --correct aborts the run otherwise)
    variants = [("dll", dll, correct)]
    if dll_lto is not None and correct_lto is not None:
        variants.append(("dll-lto", dll_lto, correct_lto))
    canon_name, canon, canon_corr = min(
        variants, key=lambda v: v[1]["sizes"]["dll_stripped_xz_bytes"])

    # cross-platform PCM: Windows shas vs Linux SongCore shas (c5 gate);
    # clean cases only — degraded cases are classified, not sha-compared
    linux_gate = load(ROOT / "build" / "minimize" / "c5" / "gate.json")
    cases = {c["id"]: c for c in load_cases(STAGE_CAPABILITIES["c5"])}
    win_shas = {}
    for cid, entry in sorted(correct["per_case"].items()):
        if entry.get("mode") == "robust":
            continue
        linux = linux_gate["songcore_pcm"].get(cid, {}).get("pcm_sha256")
        win_shas[cid] = {
            "capability": cases[cid]["capability"] if cid in cases else None,
            "windows_sha256": entry.get("sequential_sha256"),
            "linux_sha256": linux,
            "identical": entry.get("sequential_sha256") == linux
            if entry.get("sequential_sha256") and linux else None,
        }

    summary = {
        "schema": 2,
        "toolchain": {
            "kind": "llvm-mingw (cross from WSL, binaries executed natively on Windows)",
            "sdk": str(Path.home() / "toolchains/llvm-mingw"),
            "cc": manifest["toolchain"]["cc"],
            "cc_ident": manifest["toolchain"]["cc_ident"],
            "target_triple": "x86_64-w64-mingw32",
            "runtime": "ucrt",
            "configure_args": manifest["configure_args"],
        },
        "closure": {
            "oracle_tu": manifest["closure"]["translation_units"],
            "full_archive_members": reach["archive_members"],
            "duplicate_member_basenames": reach["duplicate_member_basenames"],
            "full_archive_pulled_members": reach["pulled_members"],
            "projected_units": projection["projected_units"],
            "projection_iterations": projection["iterations"],
            "compiled_units_verified": projection["pulled_units"],
            "reachability_proof": reach["verification"]["method"],
        },
        "static_archive_bytes": dll["sizes"]["libqianqian_av_a_bytes"],
        "dll": dll,
        "dll_lto": dll_lto,
        "canonical_shipping": {
            "variant": canon_name,
            "stage": canon["stage"],
            "lto": canon["lto"],
            "stripped_bytes": canon["sizes"]["dll_stripped_bytes"],
            "stripped_xz_bytes": canon["sizes"]["dll_stripped_xz_bytes"],
            "selection": "min dll_stripped_xz_bytes among variants with own native correctness PASS",
            "correctness_verdicts": {n: c["verdict"] for n, _, c in variants},
        },
        "correctness": {
            "verdict": correct["verdict"],
            "verdict_lto": correct_lto["verdict"] if correct_lto else None,
            "total_applicable": correct["total_applicable"],
            "clean_cases": correct["clean_cases"],
            "degraded_cases": correct["degraded_cases"],
            "executed": correct["executed"],
            "skipped": correct["skipped"],
            "seek_tiers": correct["seek_tiers"],
            "unicode_path_gate": correct["unicode_path_gate"]["pass"],
            "largefile_gate": correct["largefile_gate"]["pass"],
            "unicode_observed": correct["unicode_path_gate"].get("observed"),
            "largefile_observed": correct["largefile_gate"].get("observed"),
        },
        "pcm_cross_platform": win_shas,
    }
    OUT.mkdir(parents=True, exist_ok=True)
    out = OUT / "windows.json"
    out.write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n")
    ident = sum(1 for v in win_shas.values() if v["identical"] is True)
    diff = [k for k, v in win_shas.items() if v["identical"] is False]
    print(f"wrote {out}")
    print(f"closure: oracle {summary['closure']['oracle_tu']} TU / "
          f"{summary['closure']['full_archive_members']} members -> "
          f"projected {summary['closure']['projected_units']} TU "
          f"(fixpoint after {summary['closure']['projection_iterations']} iteration(s))")
    print(f"canonical shipping: {canon_name} "
          f"{canon['sizes']['dll_stripped_bytes']} B stripped / "
          f"{canon['sizes']['dll_stripped_xz_bytes']} B xz")
    print(f"correctness: {correct['verdict']} (clean {correct['clean_cases']} + "
          f"degraded {correct['degraded_cases']}, tiers {correct['seek_tiers']})")
    print(f"pcm cross-platform: {ident} identical, {len(diff)} differing {diff[:8]}")


if __name__ == "__main__":
    main()
