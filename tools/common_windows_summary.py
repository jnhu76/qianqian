#!/usr/bin/env python3
"""Collect the Windows phase results into bench/results/common-formats/windows.json.

Gathers: toolchain identity, target-specific closure sizes, reachability,
static archive, both DLL variants (stock -Os and -Os+LTO) with their PE
gates, and the native correctness report — including a cross-platform PCM
comparison against the Linux stage gates (exact sha equality where the
decoder contract demands it; recorded per-case otherwise).

    python3 tools/common_windows.py --summary
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
    reach = load(WIN / "win-reachability.json")
    dll = load(WIN / "dll" / "dll.json")
    dll_lto = load(WIN / "dll-lto" / "dll.json")
    correct = load(WIN / "correctness" / "correctness.json")
    artifacts = WIN.parent.parent / "artifacts"

    # cross-platform PCM: Windows shas vs Linux SongCore shas (c5 gate)
    linux_gate = load(ROOT / "build" / "minimize" / "c5" / "gate.json")
    pcm = []
    for cid, entry in correct.get("failures", {}).items():
        pass  # failures were already fatal at gate time
    for cid, entry in sorted(correct["failures"].items()) and []:
        pass
    cases = {c["id"]: c for c in load_cases(STAGE_CAPABILITIES["c5"])}
    win_shas = {}
    # correctness.json keeps per-case entries only for failures; re-derive the
    # full table by re-reading the raw per-case output saved during the run
    raw = correct.get("per_case", {})
    for cid, entry in raw.items():
        linux = linux_gate["songcore_pcm"].get(cid, {}).get("pcm_sha256")
        win_shas[cid] = {
            "capability": cases[cid]["capability"] if cid in cases else None,
            "windows_sha256": entry.get("sequential_sha256"),
            "linux_sha256": linux,
            "identical": entry.get("sequential_sha256") == linux
            if entry.get("sequential_sha256") and linux else None,
        }

    summary = {
        "schema": 1,
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
            "reachable_tu": reach["pulled_members"],
            "archive_members": reach["archive_members"],
            "lld_map_member_set_equal": reach["lld_map_member_set_equal"],
        },
        "static_archive_bytes": artifacts.joinpath("libqianqian_av.a").stat().st_size
        if artifacts.joinpath("libqianqian_av.a").is_file() else dll["sizes"]["libqianqian_av_a_bytes"],
        "dll": dll,
        "dll_lto": dll_lto,
        "correctness": {
            "verdict": correct["verdict"],
            "cases": correct["cases"],
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
    print(f"pcm cross-platform: {ident} identical, {len(diff)} differing {diff[:8]}")


if __name__ == "__main__":
    main()
