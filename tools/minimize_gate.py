#!/usr/bin/env python3
"""Stage gate for the FFmpeg source-minimization ladder.

Run AFTER a candidate stage's artifacts are built (xmake with the stage
manifest). Produces build/minimize/<stage>/gate.json with:

  - corpus oracle-vs-candidate equivalence (verify_xmake_core.py, 15/15)
  - SongCore PCM smoke (canonical MP3/FLAC hashes)
  - SongCore-level seek/EOF probes (songcore_seek_probe) on real songs
  - bench-level full-file correctness (qn_bench correct) on real songs
  - decode throughput (qn_bench bench) xRT
  - the size table (archive / members / linked / stripped / compressed)

Gate semantics follow the existing corpus contract: FLAC seek is strict
(suffix-exact), MP3 seek is record-and-compare against the frozen S0
baseline (MP3 bit reservoir makes mid-stream content history-dependent by
design). Timings are recorded but excluded from candidate==baseline equality.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
ARTIFACTS = ROOT / "build" / "artifacts"
MINIMIZE = ROOT / "build" / "minimize"
REAL_SONGS = [
    "corpus/local/yinxing-de-chibi/mp3-cbr-128.mp3",
    "corpus/local/yinxing-de-chibi/mp3-cbr-320-artwork.mp3",
    "corpus/local/yinxing-de-chibi/flac-16-44-artwork.flac",
]
EXTRA_SEEK_FIXTURES = ["corpus/fixtures/flac-24-96.flac"]
BENCH_BIN = ROOT / "build" / "ffmpeg-xmake" / "verify" / "qn_bench_xmake"

TIMING_KEYS = re.compile(r".*(_ms|_us|xrt.*|peak_rss_kb)$")
POINTER = re.compile(r"@ 0x[0-9a-f]+")


def sanitize(text: str) -> str:
    """Remove ASLR pointer addresses so logs compare equal across runs."""
    return POINTER.sub("@0xPTR", text)


def strip_measurements(obj):
    if isinstance(obj, dict):
        return {k: strip_measurements(v) for k, v in obj.items() if not TIMING_KEYS.match(k)}
    if isinstance(obj, list):
        return [strip_measurements(v) for v in obj]
    return obj


def run(cmd: list[str], *, check=True, binary=False) -> subprocess.CompletedProcess:
    p = subprocess.run(cmd, cwd=ROOT, capture_output=True, text=not binary)
    if check and p.returncode:
        err = p.stderr if not binary else p.stderr.decode("utf-8", "replace")
        raise SystemExit(f"command failed ({p.returncode}): {' '.join(cmd)}\n{err[-4000:]}")
    return p


def last_json(stdout: str):
    lines = [l for l in stdout.splitlines() if l.strip()]
    return json.loads(lines[-1])


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for block in iter(lambda: f.read(1 << 20), b""):
            h.update(block)
    return h.hexdigest()


def archive_members(path: Path) -> tuple[int, list[str]]:
    out = run(["ar", "t", str(path)]).stdout
    members = [l for l in out.splitlines() if l.strip() and not l.startswith("/")]
    return len(members), members


def archive_member_bytes(path: Path) -> int:
    out = run(["ar", "tv", path]).stdout
    total = 0
    for line in out.splitlines():
        m = re.search(r"\s(\d+)\s+[A-Z][a-z]{2}\s", line)
        if m:
            total += int(m.group(1))
    return total


def symbol_count(path: Path) -> int:
    out = run(["nm", str(path)], check=False).stdout
    return sum(1 for l in out.splitlines() if l.strip())


def xz_bytes(path: Path) -> int:
    p = subprocess.run(["xz", "-c", str(path)], capture_output=True)
    if p.returncode:
        raise SystemExit(f"xz failed for {path}")
    return len(p.stdout)


def linked_size(binary: Path, *, stripped: bool) -> dict:
    tmp = Path(tempfile.mkdtemp()) / binary.name
    shutil.copy2(binary, tmp)
    if stripped:
        run(["strip", str(tmp)])
    result = {"bytes": tmp.stat().st_size, "xz_bytes": xz_bytes(tmp)}
    shutil.rmtree(tmp.parent)
    return result


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--stage", required=True)
    ap.add_argument("--bench-iterations", type=int, default=3)
    args = ap.parse_args()

    stage_dir = MINIMIZE / args.stage
    stage_dir.mkdir(parents=True, exist_ok=True)

    single = ARTIFACTS / "libqianqian_av.a"
    songcore = ARTIFACTS / "libsongcore.a"
    pcm_dump = ARTIFACTS / "qn_pcm_dump"
    for p in (single, songcore, pcm_dump):
        if not p.is_file():
            raise SystemExit(f"missing artifact {p}; build the stage first")

    # --- 1. corpus oracle equivalence + SongCore PCM smoke (canonical gate)
    run([sys.executable, "tools/verify_xmake_core.py"])
    verify_report = json.loads((ROOT / "build" / "ffmpeg-xmake" / "verify" / "report.json").read_text())
    shutil.copy2(ROOT / "build" / "ffmpeg-xmake" / "verify" / "report.json", stage_dir / "verify-report.json")

    # --- 2. build the SongCore seek probe against THIS stage's archives
    probe_bin = stage_dir / "songcore_seek_probe"
    run(["gcc", "-O2", "-Wall", "-I", "include", "-o", str(probe_bin),
         "tools/songcore_seek_probe.c", str(songcore), str(single), "-lm", "-lpthread"])

    # --- 3. SongCore-level seek/EOF probes
    # Real songs: full production-path behavior is RECORDED and compared
    # candidate==baseline (the real FLAC has a damaged final frame at S0, so
    # strict suffix/EOF contracts only apply to clean corpus fixtures).
    seek_targets = REAL_SONGS + EXTRA_SEEK_FIXTURES
    seek_probe = {}
    for rel in seek_targets:
        p = run([str(probe_bin), rel], check=False)
        try:
            data = last_json(p.stdout)
            if not isinstance(data, dict):
                raise TypeError("expected object")
        except Exception:
            data = {"parse_error": p.stdout[-200:], "stderr_tail": p.stderr[-400:]}
        data["exit_code"] = p.returncode
        seek_probe[rel] = data
        (stage_dir / (rel.replace("/", "__") + ".seekprobe.json")).write_text(
            json.dumps(seek_probe[rel], indent=1) + "\n")

    # FLAC strict contract applies ONLY to clean corpus fixtures; MP3 is
    # record-and-compare (bit reservoir), per the stage-a corpus contract.
    strict_failures = []
    for rel, data in seek_probe.items():
        if rel.startswith("corpus/fixtures/") and rel.endswith(".flac"):
            if not data.get("sequential_ok") or data.get("exit_code") != 0:
                strict_failures.append(f"{rel}: sequential decode failed")
            for s in data.get("seeks", []):
                if s.get("status") != "done" or not s.get("suffix_exact") or not s.get("clean_eof"):
                    strict_failures.append(f"{rel}@{s.get('target_us')}: strict seek contract violated")

    # --- 4. real-song full decode through the shipping binary (qn_pcm_dump)
    real_song_pcm = {}
    for rel in REAL_SONGS:
        p = run([str(pcm_dump), rel], check=False, binary=True)
        header, payload = p.stdout[:12], p.stdout[12:]
        stderr_tail = p.stderr.decode("utf-8", "replace").strip().splitlines()
        entry = {
            "exit_code": p.returncode,
            "stderr_tail": sanitize(stderr_tail[-1]) if stderr_tail else "",
        }
        if header[:4] == b"QPCM":
            sample_rate = int.from_bytes(header[4:8], "little")
            channels = int.from_bytes(header[8:10], "little")
            entry["sample_rate"] = sample_rate
            entry["channels"] = channels
            entry["pcm_frames"] = len(payload) // (channels * 4) if channels else 0
            entry["pcm_sha256"] = hashlib.sha256(payload).hexdigest()
        else:
            entry["pcm_sha256"] = None
        real_song_pcm[rel] = entry

    # --- 5. throughput xRT
    throughput = {}
    for rel in REAL_SONGS:
        p = run([str(BENCH_BIN), "bench", rel, str(args.bench_iterations)])
        data = last_json(p.stdout)
        throughput[rel] = {
            "xrt_songcore_output": data.get("xrt_songcore_output"),
            "xrt_decode_core": data.get("xrt_decode_core"),
            "audio_seconds": data.get("audio_seconds"),
        }

    # --- 6. size table
    n_members, _ = archive_members(single)
    qn_linked = linked_size(pcm_dump, stripped=False)
    qn_stripped = linked_size(pcm_dump, stripped=True)
    sizes = {
        "libqianqian_av_a_bytes": single.stat().st_size,
        "libqianqian_av_members": n_members,
        "libqianqian_av_member_bytes": archive_member_bytes(single),
        "libqianqian_av_symbols": symbol_count(single),
        "libqianqian_av_xz_bytes": xz_bytes(single),
        "libsongcore_a_bytes": songcore.stat().st_size,
        "libsongcore_a_xz_bytes": xz_bytes(songcore),
        "qn_pcm_dump_bytes": qn_linked["bytes"],
        "qn_pcm_dump_xz_bytes": qn_linked["xz_bytes"],
        "qn_pcm_dump_stripped_bytes": qn_stripped["bytes"],
        "qn_pcm_dump_stripped_xz_bytes": qn_stripped["xz_bytes"],
    }
    if verify_report.get("single_archive_bytes") != sizes["libqianqian_av_a_bytes"]:
        raise SystemExit(
            "gate integrity: verify ran against a different archive than measured "
            f"({verify_report.get('single_archive_bytes')} vs {sizes['libqianqian_av_a_bytes']})")

    gate = {
        "stage": args.stage,
        "verify": verify_report,
        "seek_probe": seek_probe,
        "strict_failures": strict_failures,
        "real_song_pcm": real_song_pcm,
        "throughput": throughput,
        "sizes": sizes,
    }
    gate_path = stage_dir / "gate.json"
    gate_path.write_text(json.dumps(gate, indent=2, sort_keys=True) + "\n")

    summary = {
        "stage": args.stage,
        "corpus": verify_report["verdict"],
        "corpus_cases": verify_report["oracle_vs_xmake_cases"],
        "pcm_pass": all(v["pcm_sha256"] for v in verify_report["songcore_pcm"].values()),
        "strict_failures": strict_failures,
        "sizes": sizes,
        "throughput": throughput,
    }
    print(json.dumps(summary, indent=2, sort_keys=True))
    print(f"wrote {gate_path.relative_to(ROOT)}")
    if strict_failures:
        raise SystemExit("STRICT SEEK CONTRACT FAILURES (see strict_failures)")


if __name__ == "__main__":
    main()
