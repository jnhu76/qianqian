#!/usr/bin/env python3
"""ffi_consistency.py — permanent cross-backend consistency gate for SongCore.

The same fixture decoded through the SAME ABI in every packaging form must
mean the SAME song. Compares the three external-consumer evidence records:

  linux    bench/results/songcore-v1/ffi-ctypes-run.json   (live here)
  windows  bench/results/songcore-v1/win-ffi.json          (recorded, real
           Windows Python over the MinGW DLL)
  wasm     bench/results/songcore-v1/ffi-wasm-run.json     (live here)

Gates (frozen policy, no new thresholds):
  lossless PCM (FLAC / ALAC / PCM WAV): frames + sample_rate + channels +
      full-window PCM SHA-256 must be EXACTLY identical across every
      backend pair.
  lossy PCM (MP3 / AAC / Vorbis / Opus): codegen floating-point differences
      are permitted by the established authority — max_abs_delta <= 1e-6 on
      PCM samples, the same tolerance the WASM viability tournament accepted
      (bench/results/wasm-summary.json). Compared over the bounded PCM dumps
      (first 8192 frames of the decode window) that every consumer can write
      with --pcm-dump-dir. Classification per pair: exact | tolerance-pass |
      fail | not_compared.
  metadata: canonical fields must match semantically across backends
      (the same song meaning, whatever the packaging).
  artwork: count + mime + data_len + compressed-byte SHA-256 +
      is_front_cover must match exactly.

Verdict per the consumer-gate convention: pass (all comparisons pass),
partial (nothing failed but some backend/comparison is not_run), fail.
--check re-derives the recorded evidence against the current records and
pinning; it never trusts the stored verdict.
"""
from __future__ import annotations

import argparse
import json
import struct
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
RESULTS = ROOT / "bench" / "results" / "songcore-v1"

# The established cross-backend float tolerance (WASM viability authority).
LOSSY_MAX_ABS_DELTA = 1e-6

LOSSLESS_CODECS = ("flac", "alac", "pcm_")

FIXTURES = [
    "flac-16-44-stereo.flac",      # lossless + known metadata
    "mp3-cbr-id3v23.mp3",          # lossy + known metadata (UTF-8)
    "aac-lc-44-stereo.m4a",        # lossy
    "opus-48-stereo.opus",         # lossy
    "wav-s16le-44-stereo.wav",     # lossless
    "artwork-mp3-jpeg.mp3",        # lossy + artwork
]

RECORDS = {
    "linux": RESULTS / "ffi-ctypes-run.json",
    "windows": RESULTS / "win-ffi.json",
    "wasm": RESULTS / "ffi-wasm-run.json",
}
DUMP_DIRS = {b: RESULTS / "pcm-dumps" / b for b in RECORDS}


def load_record(path: Path) -> dict[str, dict]:
    if not path.is_file():
        return {}
    data = json.loads(path.read_text())
    if data.get("verdict") != "pass":
        return {}
    return {s["song"]: s for s in data.get("songs", [])}


def load_dump(backend: str, fixture: str) -> bytes | None:
    p = DUMP_DIRS[backend] / f"{fixture}.f32.dump"
    return p.read_bytes() if p.is_file() else None


def max_abs_delta(a: bytes, b: bytes) -> float:
    n = min(len(a), len(b)) // 4 * 4
    fa = struct.unpack(f"<{n // 4}f", a[:n])
    fb = struct.unpack(f"<{n // 4}f", b[:n])
    return max((abs(x - y) for x, y in zip(fa, fb)), default=0.0)


def compare(base: str, other: str, song_base: dict, song_other: dict,
            fixtures: list[str]) -> list[dict]:
    rows = []
    for f in fixtures:
        sb, so = song_base.get(f), song_other.get(f)
        if not sb or not so:
            rows.append({"fixture": f, "pair": f"{base}-{other}",
                         "pcm": "not_run", "metadata": "not_run",
                         "artwork": "not_run"})
            continue
        row = {"fixture": f, "pair": f"{base}-{other}"}

        # Stream shape must agree first.
        shape_ok = (sb.get("sample_rate") == so.get("sample_rate")
                    and sb.get("channels") == so.get("channels")
                    and sb.get("decoded_frames") == so.get("decoded_frames"))
        codec = (sb.get("codec") or "")
        lossless = codec.startswith(LOSSLESS_CODECS)
        row["class"] = "lossless" if lossless else "lossy"

        if not shape_ok:
            row["pcm"] = "fail"
            row["detail"] = (f"stream shape differs: "
                             f"{sb.get('sample_rate')}Hz/{sb.get('channels')}ch/"
                             f"{sb.get('decoded_frames')}f vs "
                             f"{so.get('sample_rate')}Hz/{so.get('channels')}ch/"
                             f"{so.get('decoded_frames')}f")
        elif lossless:
            row["pcm"] = ("exact" if sb.get("pcm_sha256") == so.get("pcm_sha256")
                          and sb.get("pcm_sha256") else "fail")
            if row["pcm"] == "fail":
                row["detail"] = (f"lossless PCM sha differs: "
                                 f"{str(sb.get('pcm_sha256'))[:16]}… vs "
                                 f"{str(so.get('pcm_sha256'))[:16]}…")
        else:
            da = load_dump(base, f)
            db = load_dump(other, f)
            if da is None or db is None:
                row["pcm"] = "not_compared"
                row["detail"] = "bounded PCM dump missing for a backend"
            else:
                delta = max_abs_delta(da, db)
                row["max_abs_delta"] = delta
                if delta == 0.0:
                    row["pcm"] = "exact"
                elif delta <= LOSSY_MAX_ABS_DELTA:
                    row["pcm"] = "tolerance-pass"
                else:
                    row["pcm"] = "fail"
                    row["detail"] = (f"lossy delta {delta:.3e} exceeds the "
                                     f"{LOSSY_MAX_ABS_DELTA:.0e} authority")

        # Metadata: canonical fields are the product meaning; raw metadata
        # count must agree as well.
        mb, mo = sb.get("canonical_metadata") or {}, \
            so.get("canonical_metadata") or {}
        raw_ok = sb.get("metadata_count") == so.get("metadata_count")
        row["metadata"] = ("semantic-match"
                           if mb == mo and raw_ok else "fail")
        if row["metadata"] == "fail":
            row["detail"] = row.get("detail", "") + \
                f" canonical {mb} vs {mo}"

        # Artwork: compressed bytes + facts, exact.
        ab, ao = sb.get("artwork0"), so.get("artwork0")
        if ab is None and ao is None:
            row["artwork"] = "exact"  # both agree: no artwork
        elif ab is None or ao is None:
            row["artwork"] = "fail"
        else:
            keys = ("mime", "data_len", "sha256", "is_front_cover")
            row["artwork"] = ("exact" if all(ab.get(k) == ao.get(k)
                                             for k in keys) else "fail")

        row["verdict"] = ("fail" if "fail" in (row["pcm"], row["metadata"],
                                               row["artwork"])
                          else ("partial" if "not_run" in
                                (row["pcm"], row["metadata"], row["artwork"])
                                or row["pcm"] == "not_compared" else "pass"))
        rows.append(row)
    return rows


def compute() -> dict:
    records = {b: load_record(p) for b, p in RECORDS.items()}
    available = {b: bool(r) for b, r in records.items()}
    # Compare every available backend against linux (the reference host);
    # if linux itself has no record, pairwise across whatever exists.
    rows: list[dict] = []
    if available["linux"]:
        for other in ("windows", "wasm"):
            if available[other]:
                rows += compare(other, "linux", records[other],
                                records["linux"], FIXTURES)
    elif available["windows"] and available["wasm"]:
        rows += compare("wasm", "windows", records["wasm"],
                        records["windows"], FIXTURES)

    missing = [b for b, ok in available.items() if not ok]
    verdict = ("fail" if any(r["verdict"] == "fail" for r in rows)
               else "partial" if (missing or any(
                   r["verdict"] != "pass" for r in rows)) else "pass")
    return {
        "tool": "tests/songcore/ffi_consistency.py",
        "policy": {
            "lossless_pcm": "exact (full-window SHA-256 equality)",
            "lossy_pcm": f"max_abs_delta <= {LOSSY_MAX_ABS_DELTA:g} "
                         "(established WASM viability authority)",
            "metadata": "canonical semantic match",
            "artwork": "exact (compressed bytes)",
        },
        "records": {b: str(p) for b, p in RECORDS.items()},
        "backends_available": available,
        "rows": rows,
        "verdict": verdict,
    }


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    mode = ap.add_mutually_exclusive_group(required=True)
    mode.add_argument("--out", action="store_true",
                      help="recompute the cross-backend comparisons")
    mode.add_argument("--check", action="store_true",
                      help="fail-closed revalidation of the recorded gate")
    args = ap.parse_args()

    out_path = RESULTS / "ffi-consistency.json"
    report = compute()
    if args.out:
        out_path.write_text(json.dumps(report, indent=2,
                                       ensure_ascii=False) + "\n")
    verdict = report["verdict"]
    for r in report["rows"]:
        print(f"  [{r['verdict'].upper():>7}] {r['fixture']} [{r['pair']}] "
              f"pcm={r['pcm']} metadata={r['metadata']} "
              f"artwork={r['artwork']}"
              + (f" max|Δ|={r['max_abs_delta']:.2e}"
                 if "max_abs_delta" in r else "")
              + (f" — {r['detail']}" if r.get("detail") else ""))
    if args.check and verdict != "pass":
        print(f"ffi-consistency --check: FAIL ({verdict})", file=sys.stderr)
        return 1
    print(f"ffi-consistency: {verdict.upper()}"
          + ("" if args.check else f" -> {out_path}"))
    return 0 if verdict == "pass" else (2 if verdict == "partial" else 1)


if __name__ == "__main__":
    raise SystemExit(main())
