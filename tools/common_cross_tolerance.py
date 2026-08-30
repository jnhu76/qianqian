#!/usr/bin/env python3
"""Cross-platform AAC PCM tolerance metric (issue #8 §11 evidence).

The 7 AAC cases decode to different bytes under gcc/Linux vs clang/Windows
(cross-compiler float codegen in the AAC decoder). Per the issue's rule this
must be recorded with a deterministic tolerance metric, never silently
relaxed gates. Both sides are the SAME pinned FFmpeg n9.0.1 decode path plus
SongCore's own conversion; only the compiler/codegen differs.

Requires a built Linux qn_pcm_dump (post-clean-room: build/artifacts/) and
the Windows one (build/minimize/win-c6/dll/fixtures lives next to the DLL;
the Windows qn_pcm_dump.exe is rebuilt from the win-c6-dll closure).

    python3 tools/common_cross_tolerance.py
"""
from __future__ import annotations

import json
import struct
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tools"))
from common_corpus import STAGE_CAPABILITIES, load_cases  # noqa: E402

OUT = ROOT / "bench" / "results" / "common-formats" / "windows.json"
WIN_EXE = ROOT / "build/minimize/win-c6/dll/qn_pcm_dump.exe"


def read_qpcm(payload: bytes):
    if payload[:4] != b"QPCM":
        return None
    sr = int.from_bytes(payload[4:8], "little")
    ch = int.from_bytes(payload[8:10], "little")
    body = payload[12:]
    n = len(body) // 4
    return sr, ch, struct.unpack(f"<{n}f", body[: n * 4])


def main() -> None:
    linux_exe = ROOT / "build/artifacts/qn_pcm_dump"
    if not linux_exe.is_file() or not WIN_EXE.is_file():
        raise SystemExit("need both platform binaries: run the Linux clean-room "
                         "and `python3 tools/common_windows.py --build --dll` first")

    summary = json.loads(OUT.read_text())
    windows = summary["dll"]["dll"]  # noqa: F841 (documentation anchor)
    cases = {c["id"]: c for c in load_cases(STAGE_CAPABILITIES["c5"])}

    metrics = {}
    for cid, rec in summary["pcm_cross_platform"].items():
        if rec["identical"] is not False:
            continue
        case = cases[cid]
        fixture = str(ROOT / "corpus/fixtures" / case["file"])
        lin = read_qpcm(subprocess.run(
            [str(linux_exe), fixture], capture_output=True, check=True).stdout)
        win = read_qpcm(subprocess.run(
            [str(WIN_EXE), fixture], capture_output=True, check=True).stdout)
        if not lin or not win:
            raise SystemExit(f"{cid}: bad QPCM stream")
        n = min(len(lin[2]), len(win[2]))
        diffs = [abs(a - b) for a, b in zip(lin[2][:n], win[2][:n]) if a != b]
        metrics[cid] = {
            "linux_frames": len(lin[2]) // lin[1],
            "windows_frames": len(win[2]) // win[1],
            "frames_equal": len(lin[2]) == len(win[2]),
            "differing_samples": len(diffs),
            "of_total": n,
            "max_abs_delta": max(diffs) if diffs else 0.0,
            "mean_abs_delta": (sum(diffs) / len(diffs)) if diffs else 0.0,
        }
        print(cid, json.dumps(metrics[cid]))

    worst = max((m["max_abs_delta"] for m in metrics.values()), default=0.0)
    summary["aac_cross_compiler_tolerance"] = {
        "method": "Linux n9.0.1 gcc build vs Windows n9.0.1 clang build, "
                  "same pinned FFmpeg decode + SongCore conversion; max/mean |delta| "
                  "over Float32 samples",
        "per_case": metrics,
        "max_abs_delta_all": worst,
        "policy": "cross-platform byte equality is NOT required for lossy "
                  "float decoders under differing codegen; within-platform "
                  "determinism remains byte-exact everywhere",
    }
    OUT.write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n")
    print(f"worst max|delta| = {worst}; wrote {OUT}")


if __name__ == "__main__":
    main()
