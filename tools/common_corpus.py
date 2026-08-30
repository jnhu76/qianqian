#!/usr/bin/env python3
"""Shared corpus/capability helpers for the Common Formats ladder (issue #8).

Single source of truth for:
- the capability set of every ladder stage (c0..c6);
- which corpus cases are applicable at a stage (stage-a + common-formats);
- how to build/run qn_bench against a given FFmpeg archive set.

Corpus contracts:
- corpus/manifest/stage-a.json      MP3/FLAC (capability tags derived from format)
- corpus/manifest/common-formats.json  AAC/ALAC/WAV/Vorbis/Opus (explicit tags)
"""
from __future__ import annotations

import hashlib
import json
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

STAGE_CAPABILITIES = {
    "c0": {"mp3", "flac"},
    "c1": {"mp3", "flac", "aac"},
    "c2": {"mp3", "flac", "aac", "alac"},
    "c3": {"mp3", "flac", "aac", "alac", "wav"},
    "c4": {"mp3", "flac", "aac", "alac", "wav", "vorbis"},
    "c5": {"mp3", "flac", "aac", "alac", "wav", "vorbis", "opus"},
}
STAGE_CAPABILITIES["c6"] = STAGE_CAPABILITIES["c5"]

STAGE_PROFILES = {
    "c0": "bench/profiles/n3-min-noswr.json",
    "c1": "bench/profiles/c1-aac.json",
    "c2": "bench/profiles/c2-alac.json",
    "c3": "bench/profiles/c3-wav.json",
    "c4": "bench/profiles/c4-vorbis.json",
    "c5": "bench/profiles/c5-opus.json",
}

# One clean fixture per format family for the SongCore-level seek/EOF probe,
# plus fixtures carrying cross-stage regression contracts from E07.
SEEK_PROBE_FILES = [
    # regression set (must not regress in ANY stage)
    "corpus/fixtures/mp3-cbr-id3v23.mp3",
    "corpus/fixtures/flac-16-44-stereo.flac",
    "corpus/fixtures/flac-24-96.flac",
    "corpus/local/yinxing-de-chibi/mp3-cbr-128.mp3",
    "corpus/local/yinxing-de-chibi/mp3-cbr-320-artwork.mp3",
    "corpus/local/yinxing-de-chibi/flac-16-44-artwork.flac",
    # one clean fixture per new format family
    "corpus/fixtures/aac-lc-44-stereo.m4a",
    "corpus/fixtures/aac-adts-44-stereo.aac",
    "corpus/fixtures/alac-16-44-stereo.m4a",
    "corpus/fixtures/alac-24-96-stereo.m4a",
    "corpus/fixtures/wav-s16le-44-stereo.wav",
    "corpus/fixtures/wav-f64le-44-mono.wav",
    "corpus/fixtures/vorbis-44-stereo.ogg",
    "corpus/fixtures/opus-48-stereo.opus",
]

# Throughput representative: one per codec, short names for tables.
THROUGHPUT_REPRESENTATIVES = {
    "mp3": "mp3-cbr-id3v23",
    "flac": "flac-16-44-stereo",
    "aac": "aac-lc-44-stereo",
    "alac": "alac-long",
    "wav": "wav-s16le-44-stereo",
    "vorbis": "vorbis-44-stereo",
    "opus": "opus-48-stereo",
}

LOSSLESS_CAPABILITIES = {"flac", "alac", "wav"}


def load_cases(capabilities: set[str]) -> list[dict]:
    """All corpus cases whose capability is in `capabilities`, in stable order.

    capability tags for stage-a cases are derived from the fixture format
    (mp3 -> mp3, flac -> flac); common-formats cases carry explicit tags.
    """
    cases: list[dict] = []
    stage_a = json.loads((ROOT / "corpus/manifest/stage-a.json").read_text())
    for case in stage_a["cases"]:
        case = dict(case)
        case["capability"] = "mp3" if case["format"] == "mp3" else "flac"
        case["_corpus"] = "stage-a"
        if case["capability"] in capabilities:
            cases.append(case)
    common = json.loads((ROOT / "corpus/manifest/common-formats.json").read_text())
    for case in common["cases"]:
        case = dict(case)
        case["_corpus"] = "common-formats"
        if case["capability"] in capabilities:
            cases.append(case)
    return cases


def fixture_path(case: dict) -> Path:
    return ROOT / "corpus" / "fixtures" / case["file"]


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for block in iter(lambda: f.read(1 << 20), b""):
            h.update(block)
    return h.hexdigest()


def verify_fixtures() -> None:
    """Corpus integrity: a drifted fixture poisons every result."""
    for manifest in ("corpus/manifest/stage-a.json", "corpus/manifest/common-formats.json"):
        data = json.loads((ROOT / manifest).read_text())
        for case in data["cases"]:
            p = fixture_path(case)
            if not p.is_file() or sha256_file(p) != case["fixture_sha256"]:
                raise SystemExit(f"fixture sha mismatch: {case['file']} — regenerate corpus")


def bench_json(exe: Path, song: Path, mode: str = "correct", extra: list[str] | None = None):
    cmd = [str(exe), mode, str(song)] + (extra or [])
    p = subprocess.run(cmd, cwd=ROOT, capture_output=True, text=True)
    lines = [l for l in p.stdout.splitlines() if l.strip()]
    if not lines:
        return p.returncode, {"harness_status": "no_output", "stderr_tail": p.stderr[-500:]}
    try:
        return p.returncode, json.loads(lines[-1])
    except json.JSONDecodeError:
        return p.returncode, {"harness_status": "bad_output", "stdout_tail": p.stdout[-300:]}


def build_bench(exe: Path, includes: list[Path], archives: list[Path], cc: str = "gcc") -> None:
    cmd = [cc, "-O2", "-Wall", "-Wno-deprecated-declarations",
           *(f"-I{inc}" for inc in includes),
           "-o", str(exe), str(ROOT / "bench/native/qn_bench.c"),
           *map(str, archives), "-lm", "-lpthread"]
    p = subprocess.run(cmd, cwd=ROOT, capture_output=True, text=True)
    if p.returncode:
        raise SystemExit(f"qn_bench build failed: {cmd}\n{p.stderr[-4000:]}")


def strip_measurements(obj):
    """Remove timing/ASLR noise so behavior comparisons are exact."""
    import re
    timing = re.compile(r".*(_ms|_us|xrt.*|peak_rss_kb)$")
    if isinstance(obj, dict):
        return {k: strip_measurements(v) for k, v in obj.items() if not timing.match(k)}
    if isinstance(obj, list):
        return [strip_measurements(v) for v in obj]
    return obj
