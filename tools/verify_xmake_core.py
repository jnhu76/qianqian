#!/usr/bin/env python3
"""Verify that the Xmake-owned single archive reproduces the FFmpeg oracle.

No audio device is required. The test compares the existing benchmark binary
linked two ways (upstream's three archives vs libqianqian_av.a), then checks
that the production SongCore PCM pipe is byte-identical on representative MP3
and FLAC fixtures. Audible playback is a separate manual smoke step.
"""
from __future__ import annotations

import hashlib
import json
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
ORACLE = ROOT / "build" / "ffmpeg-xmake" / "oracle"
MANIFEST = ROOT / "build" / "ffmpeg-xmake" / "manifest.json"
ARTIFACTS = ROOT / "build" / "artifacts"
CORPUS = ROOT / "corpus" / "fixtures"
CORPUS_MANIFEST = ROOT / "corpus" / "manifest" / "stage-a.json"
BENCH_SRC = ROOT / "bench" / "native" / "qn_bench.c"
VERIFY_DIR = ROOT / "build" / "ffmpeg-xmake" / "verify"


def run(cmd: list[str], *, binary: bool = False):
    p = subprocess.run(cmd, cwd=ROOT, capture_output=True, text=not binary)
    if p.returncode:
        stderr = p.stderr if not binary else p.stderr.decode("utf-8", "replace")
        raise SystemExit(f"command failed ({p.returncode}): {' '.join(cmd)}\n{stderr[-4000:]}")
    return p


def bench_json(exe: Path, song: Path) -> tuple[int, dict]:
    p = subprocess.run([str(exe), "correct", str(song)], cwd=ROOT, capture_output=True, text=True)
    lines = [line for line in p.stdout.splitlines() if line.strip()]
    if not lines:
        raise SystemExit(f"no JSON from {exe.name} for {song.name}\n{p.stderr[-2000:]}")
    return p.returncode, json.loads(lines[-1])


def remove_measurement_noise(value):
    if isinstance(value, dict):
        return {
            k: remove_measurement_noise(v)
            for k, v in value.items()
            if not k.endswith("_ms") and k != "peak_rss_kb"
        }
    if isinstance(value, list):
        return [remove_measurement_noise(v) for v in value]
    return value


def build_bench(exe: Path, archives: list[Path]) -> None:
    cmd = [
        "gcc", "-O2", "-Wall", "-Wno-deprecated-declarations",
        f"-I{ORACLE}", f"-I{ROOT / 'build' / 'ffmpeg-src'}",
        "-o", str(exe), str(BENCH_SRC),
        *map(str, archives), "-lm", "-lpthread",
    ]
    run(cmd)


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def songcore_pcm(qn_pcm_dump: Path, song: Path) -> tuple[dict, str, int]:
    p = run([str(qn_pcm_dump), str(song)], binary=True)
    out = p.stdout
    if len(out) < 12 or out[:4] != b"QPCM":
        raise SystemExit(f"bad QPCM stream for {song.name}")
    sample_rate = int.from_bytes(out[4:8], "little")
    channels = int.from_bytes(out[8:10], "little")
    fmt = int.from_bytes(out[10:12], "little")
    if fmt != 1 or channels <= 0:
        raise SystemExit(f"bad QPCM header for {song.name}")
    payload = out[12:]
    frame_bytes = channels * 4
    if len(payload) % frame_bytes:
        raise SystemExit(f"truncated QPCM payload for {song.name}")
    return {"sample_rate": sample_rate, "channels": channels, "format": "f32le"}, sha256(payload), len(payload) // frame_bytes


def main() -> None:
    if not MANIFEST.is_file():
        raise SystemExit("missing FFmpeg manifest; run `xmake ffmpeg-import`")
    manifest = json.loads(MANIFEST.read_text())
    single = ARTIFACTS / "libqianqian_av.a"
    pcm_dump = ARTIFACTS / ("qn_pcm_dump.exe" if sys.platform == "win32" else "qn_pcm_dump")
    if not single.is_file() or not pcm_dump.is_file():
        raise SystemExit("missing Xmake artifacts; run `xmake build qn_pcm_dump`")

    VERIFY_DIR.mkdir(parents=True, exist_ok=True)
    ref_bench = VERIFY_DIR / "qn_bench_oracle"
    replay_bench = VERIFY_DIR / "qn_bench_xmake"
    reference_archives = [
        ORACLE / "libavformat" / "libavformat.a",
        ORACLE / "libavcodec" / "libavcodec.a",
        ORACLE / "libavutil" / "libavutil.a",
    ]
    build_bench(ref_bench, reference_archives)
    build_bench(replay_bench, [single])

    corpus = json.loads(CORPUS_MANIFEST.read_text())
    mismatches = []
    replay_results: dict[str, dict] = {}
    for case in corpus["cases"]:
        song = CORPUS / case["file"]
        reference_rc, reference = bench_json(ref_bench, song)
        replay_rc, replay = bench_json(replay_bench, song)
        replay_results[case["id"]] = replay
        if (reference_rc != replay_rc or
                remove_measurement_noise(reference) != remove_measurement_noise(replay)):
            mismatches.append(case["id"])

    if mismatches:
        raise SystemExit("Xmake archive diverges from oracle on: " + ", ".join(mismatches))

    smoke_ids = ("mp3-cbr-id3v23", "flac-16-44-stereo")
    songcore = {}
    by_id = {case["id"]: case for case in corpus["cases"]}
    for case_id in smoke_ids:
        case = by_id[case_id]
        song = CORPUS / case["file"]
        header, pcm_sha, frames = songcore_pcm(pcm_dump, song)
        expected = replay_results[case_id].get("decode", {}).get("canonical_f32_sha256")
        expected_len = replay_results[case_id].get("decode", {}).get("canonical_len", 0)
        expected_frames = expected_len // max(1, header["channels"])
        if pcm_sha != expected or frames != expected_frames:
            raise SystemExit(
                f"SongCore PCM mismatch for {case_id}: sha={pcm_sha} expected={expected}, "
                f"frames={frames} expected_frames={expected_frames}"
            )
        songcore[case_id] = {**header, "frames": frames, "pcm_sha256": pcm_sha}

    report = {
        "verdict": "PASS",
        "ffmpeg_tag": manifest["ffmpeg_tag"],
        "ffmpeg_commit_sha": manifest["ffmpeg_commit_sha"],
        "profile": manifest["profile"],
        "translation_units": manifest["closure"]["translation_units"],
        "corpus_id": corpus["corpus_id"],
        "oracle_vs_xmake_cases": len(corpus["cases"]),
        "songcore_pcm": songcore,
        "single_archive_bytes": single.stat().st_size,
    }
    out = VERIFY_DIR / "report.json"
    out.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    print(json.dumps(report, indent=2, sort_keys=True))
    print(f"wrote {out.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
