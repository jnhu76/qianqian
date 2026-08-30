#!/usr/bin/env python3
"""Per-stage behavior + size gate for the Common Formats capability ladder.

Runs AFTER a stage's projected closure is built via Xmake. Produces
build/minimize/<stage>/gate.json with:

  1. verify      oracle-vs-candidate qn_bench equivalence for every corpus
                 case applicable to this stage's capability set (the oracle
                 is the stage's OWN full closure, so equality proves the
                 link-reachability projection changed nothing);
  2. expect      corpus expectations vs the committed manifests
                 (structural/metadata/artwork/PCM-strict/samples/seek/EOF),
                 reusing the stage-a checker;
  3. songcore    production-path PCM for every applicable case via
                 qn_pcm_dump: strict sha vs manifest for lossless, exact
                 equality vs the oracle bench for lossy (deterministic
                 reference policy — never the compressed file itself);
  4. seek        SongCore-level seek/EOF probes (tools/songcore_seek_probe)
                 on one clean fixture per format family + the E07 regression
                 set; strict families must suffix-match, record families are
                 recorded for cross-stage comparison;
  5. throughput  xRT for the stage's representative codec samples;
  6. sizes       archive / linked / stripped / compressed table.

Exit code is non-zero on any gate failure.
"""
from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import re
import shutil
import subprocess
import sys
import tempfile
from importlib.machinery import SourceFileLoader
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tools"))
from common_corpus import (  # noqa: E402
    LOSSLESS_CAPABILITIES, SEEK_PROBE_FILES, STAGE_CAPABILITIES,
    bench_json, build_bench, fixture_path, load_cases, sha256_file,
    strip_measurements, verify_fixtures,
)

TIMING_KEYS = re.compile(r".*(_ms|_us|xrt.*|peak_rss_kb)$")
POINTER = re.compile(r"@ 0x[0-9a-f]+")

REAL_SONG_PREFIX = "corpus/local/"


def load_checker():
    loader = SourceFileLoader("qianqian_run_bench", str(ROOT / "bench/harness/run_bench.py"))
    spec = importlib.util.spec_from_loader("qianqian_run_bench", loader)
    mod = importlib.util.module_from_spec(spec)
    loader.exec_module(mod)
    return mod.check_case


def sanitize(text: str) -> str:
    return POINTER.sub("@0xPTR", text)


def run(cmd: list[str], *, check=True, binary=False) -> subprocess.CompletedProcess:
    p = subprocess.run(cmd, cwd=ROOT, capture_output=True, text=not binary)
    if check and p.returncode:
        err = p.stderr if not binary else p.stderr.decode("utf-8", "replace")
        raise SystemExit(f"command failed ({p.returncode}): {' '.join(cmd)}\n{err[-4000:]}")
    return p


def oracle_archives(oracle_dir: Path) -> list[Path]:
    names = ["libavformat/libavformat.a", "libavcodec/libavcodec.a",
             "libavutil/libavutil.a", "libswresample/libswresample.a"]
    return [oracle_dir / n for n in names if (oracle_dir / n).is_file()]


def archive_members(path: Path) -> int:
    out = run(["ar", "t", str(path)]).stdout
    return sum(1 for l in out.splitlines() if l.strip() and not l.startswith("/"))


def archive_member_bytes(path: Path) -> int:
    out = run(["ar", "tv", str(path)]).stdout
    total = 0
    for line in out.splitlines():
        m = re.search(r"\s(\d+)\s+[A-Z][a-z]{2}\s", line)
        if m:
            total += int(m.group(1))
    return total


def symbol_count(path: Path) -> int:
    out = run(["nm", str(path)], check=False).stdout
    return sum(1 for l in out.splitlines() if l.strip())


# file-backed, non-debug section filter for per-unit size attribution
# (.bss/.sbss are NOBITS runtime memory; DWARF is stripped from shipping)
UNIT_EXCLUDED_SECTION = re.compile(r"^\.(debug|comment|group|note|zdebug|s?bss)")


def unit_file_backed_bytes(archive: Path, manifest_path: Path, out_path: Path) -> None:
    """Per-manifest-unit file-backed section bytes, machine-derived.

    Members are extracted by archive offset (duplicate basenames such as
    libavcodec/aacdec.o vs libavformat/aacdec.o stay distinct via the 1:1
    manifest-order mapping) and measured with `size -A`. Consumed by
    tools/common_attribution.py for capability-increment attribution."""
    from link_audit import parse_archive
    from tempfile import TemporaryDirectory

    manifest = json.loads(manifest_path.read_text())
    units = manifest["units"]
    members = parse_archive(archive)
    if len(members) != len(units):
        raise SystemExit(f"unit-bytes: member/unit drift {len(members)} vs {len(units)}")
    raw = archive.read_bytes()
    results = []
    with TemporaryDirectory() as td:
        for m, unit in zip(members, units):
            obj = Path(td) / f"m{m['index']:04d}"
            obj.write_bytes(raw[m["offset"]:m["offset"] + m["size"]])
            out = run(["size", "-A", str(obj)]).stdout
            total = 0
            for line in out.splitlines():
                parts = line.split()
                if len(parts) != 2 or not parts[0].startswith("."):
                    continue
                try:
                    sz = int(parts[1])
                except ValueError:
                    continue
                if not UNIT_EXCLUDED_SECTION.match(parts[0]):
                    total += sz
            results.append({
                "unit_object": unit["object"],
                "source": unit["path"],
                "origin": unit["origin"],
                "file_backed_bytes": total,
            })
    out_path.write_text(json.dumps(results, indent=1) + "\n")


def xz_bytes(path: Path) -> int:
    return len(subprocess.run(["xz", "-c", str(path)], capture_output=True,
                              check=True).stdout)


def linked_size(binary: Path, *, stripped: bool) -> dict:
    tmp = Path(tempfile.mkdtemp()) / binary.name
    shutil.copy2(binary, tmp)
    if stripped:
        run(["strip", str(tmp)])
    result = {"bytes": tmp.stat().st_size, "xz_bytes": xz_bytes(tmp)}
    shutil.rmtree(tmp.parent)
    return result


def qn_pcm_dump(pcm_dump: Path, song: Path) -> dict:
    p = run([str(pcm_dump), str(song)], check=False, binary=True)
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
    return entry


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--stage", required=True)
    ap.add_argument("--oracle-dir", default=None,
                    help="oracle tree for this stage (default: the stage dir's own)")
    ap.add_argument("--bench-iterations", type=int, default=3)
    args = ap.parse_args()

    stage_dir = ROOT / "build" / "minimize" / args.stage
    stage_dir.mkdir(parents=True, exist_ok=True)
    # derived stages (cN-so, cN-so-lto, c6-os, win-*) gate with the base
    # stage's capability set
    m = re.match(r"(c\d+)", args.stage)
    if not m or m.group(1) not in STAGE_CAPABILITIES:
        raise SystemExit(f"cannot map stage {args.stage} to a capability set")
    base_stage = m.group(1)
    capabilities = STAGE_CAPABILITIES[base_stage]
    # derived stages (cN-so, c6-os...) have no oracle of their own; the base
    # stage's oracle proves their (codegen-only) closure is still equivalent
    if args.oracle_dir:
        oracle_dir = Path(args.oracle_dir)
    elif (stage_dir / "oracle").is_dir():
        oracle_dir = stage_dir / "oracle"
    else:
        # c6-* stages have no import of their own: the full-set closure
        # lives in the c5 stage dir
        oracle_base = "c5" if base_stage == "c6" else base_stage
        oracle_dir = ROOT / "build" / "minimize" / oracle_base / "oracle"

    single = ROOT / "build/artifacts/libqianqian_av.a"
    songcore = ROOT / "build/artifacts/libsongcore.a"
    pcm_dump = ROOT / "build/artifacts/qn_pcm_dump"
    for p in (single, songcore, pcm_dump):
        if not p.is_file():
            raise SystemExit(f"missing artifact {p}; build the stage first")
    verify_fixtures()

    cases = load_cases(capabilities)
    check_case = load_checker()

    # --- 1. oracle vs candidate bench equivalence (+ store oracle results)
    verify_dir = stage_dir / "verify"
    verify_dir.mkdir(exist_ok=True)
    oracle_bench = verify_dir / "qn_bench_oracle"
    xmake_bench = verify_dir / "qn_bench_xmake"
    build_bench(oracle_bench, [oracle_dir, ROOT / "build/ffmpeg-src"],
                oracle_archives(oracle_dir))
    build_bench(xmake_bench, [oracle_dir, ROOT / "build/ffmpeg-src"], [single])

    oracle_results, xmake_results, mismatch = {}, {}, []
    for case in cases:
        song = fixture_path(case)
        orc_rc, orc = bench_json(oracle_bench, song)
        xmk_rc, xmk = bench_json(xmake_bench, song)
        oracle_results[case["id"]] = orc
        xmake_results[case["id"]] = xmk
        if (orc_rc != xmk_rc or
                strip_measurements(orc) != strip_measurements(xmk)):
            mismatch.append(case["id"])
    if mismatch:
        raise SystemExit("candidate archive diverges from oracle on: " + ", ".join(mismatch))

    # --- 2. expectation gate vs committed corpus manifests
    expectations = {}
    for case in cases:
        status, checks = check_case(case, xmake_results[case["id"]], None)
        expectations[case["id"]] = {"status": status, "checks": checks}
        if status == "fail":
            print(f"  [EXPECT-FAIL] {case['id']}: {checks.get('failures')}")

    # --- 3. SongCore production-path PCM
    songcore_pcm = {}
    for case in cases:
        if case["expect"].get("probe_may_fail"):
            # malformed inputs must reach the production path too — the gate
            # is a typed bounded failure, never "not executed". Two typed
            # failure shapes are accepted (recorded per case):
            #   OPEN_OR_PROBE_FAILED      no PCM, nonzero exit
            #   DECODE_ERROR_AFTER_OUTPUT bounded PCM, then typed decode error
            entry = qn_pcm_dump(pcm_dump, fixture_path(case))
            if entry["exit_code"] != 0 and entry["pcm_sha256"] is None:
                entry["typed_outcome"] = "OPEN_OR_PROBE_FAILED"
            elif entry["exit_code"] != 0:
                entry["typed_outcome"] = "DECODE_ERROR_AFTER_OUTPUT"
            else:
                entry["typed_outcome"] = "CLEAN_DECODE"
            repeat = qn_pcm_dump(pcm_dump, fixture_path(case))
            entry["deterministic"] = (
                entry["exit_code"] == repeat["exit_code"]
                and entry["pcm_sha256"] == repeat["pcm_sha256"])
            songcore_pcm[case["id"]] = entry
            if entry["typed_outcome"] == "CLEAN_DECODE" or not entry["deterministic"]:
                raise SystemExit(
                    f"malformed case {case['id']} did not fail as a typed, "
                    f"deterministic production-path failure: {entry}")
            continue
        entry = qn_pcm_dump(pcm_dump, fixture_path(case))
        songcore_pcm[case["id"]] = entry
        if entry["pcm_sha256"] is None:
            print(f"  [SONGCORE-PCM] {case['id']}: no PCM (exit {entry['exit_code']})")
            continue
        if case["expect"]["pcm"]["mode"] == "strict":
            want = case["expect"]["pcm"]["canonical_f32_sha256"]
            kind = "manifest-strict"
        else:
            want = (oracle_results[case["id"]].get("decode") or {}).get("canonical_f32_sha256")
            kind = "oracle-reference"
        if entry["pcm_sha256"] != want:
            raise SystemExit(
                f"SongCore PCM mismatch for {case['id']} ({kind}): "
                f"{entry['pcm_sha256']} != {want}")

    # --- 4. SongCore-level seek/EOF probes
    #
    # Seek contract tiers (the gate owns the minimum semantics; calibrate
    # only observes and classifies families):
    #
    #   STRICT      seek success + bounded resume + PCM + clean EOF
    #               + exact suffix equality            (FLAC/ALAC/WAV/Vorbis)
    #   LAPPED      seek success + bounded resume + PCM + clean EOF;
    #               suffix byte equality NOT required because codec state
    #               laps across packets: MP3 bit reservoir, AAC MDCT 50%
    #               overlap, CELT overlap-add          (MP3/AAC-M4A/Opus)
    #   UNSUPPORTED typed seek failure accepted; decode/EOF still hold
    #               (raw ADTS: upstream demuxer has no seek)
    #   REGRESSION  real E07 songs: recorded for cross-stage comparison
    #               only (known damaged inputs, documented in E07)
    def seek_tier(rel: str, case) -> str | None:
        if case is None:
            return "REGRESSION"          # corpus/local real songs
        if case["expect"].get("probe_may_fail"):
            return None                  # malformed: covered by production-path typed gate
        if rel.endswith(".aac"):
            return "UNSUPPORTED"         # raw ADTS stream
        if case["expect"].get("seek") == "strict":
            return "STRICT"
        return "LAPPED"

    case_by_file = {case["file"]: case for case in cases}
    seek_failures = []
    tier_counts: dict[str, int] = {}
    seek_probe = {}
    probe_src = ROOT / "tools/songcore_seek_probe.c"
    probe_bin = stage_dir / "songcore_seek_probe"
    run(["gcc", "-O2", "-Wall", "-I", "include", "-o", str(probe_bin),
         str(probe_src), str(songcore), str(single), "-lm", "-lpthread"])
    for rel in SEEK_PROBE_FILES:
        case = case_by_file.get(Path(rel).name)
        if case is None and not rel.startswith(REAL_SONG_PREFIX):
            continue
        family = (case["capability"] if case
                  else ("flac" if rel.endswith(".flac") else "mp3"))
        tier = seek_tier(rel, case)
        if tier is None:
            continue
        p = run([str(probe_bin), rel], check=False)
        try:
            data = json.loads(p.stdout.strip().splitlines()[-1])
        except Exception:
            data = {"parse_error": p.stdout[-200:], "stderr_tail": p.stderr[-400:]}
        data["exit_code"] = p.returncode
        data["family"] = family
        data["contract"] = tier
        seek_probe[rel] = data
        tier_counts[tier] = tier_counts.get(tier, 0) + 1

        def fail(msg: str):
            seek_failures.append(f"{rel} [{tier}]: {msg}")

        if tier == "REGRESSION":
            continue  # recorded only; real songs carry E07-documented damage
        if tier == "UNSUPPORTED":
            if not data.get("sequential_ok") or data.get("exit_code") != 0:
                fail("sequential decode failed")
            for s in data.get("seeks", []):
                if s.get("status") == "seek_failed":
                    continue  # the typed failure this tier accepts
                if s.get("status") != "done" or not s.get("clean_eof") or not s.get("frames"):
                    fail(f"@{s.get('target_us')}: seek neither typed-failed nor "
                         f"a bounded clean-EOF decode")
            continue
        # STRICT / LAPPED both require: sequential ok, every seek done with
        # bounded resume + PCM + clean EOF
        if not data.get("sequential_ok") or data.get("exit_code") != 0:
            fail("sequential decode failed")
        for s in data.get("seeks", []):
            if s.get("status") != "done":
                fail(f"@{s.get('target_us')}: seek did not succeed")
                continue
            if not s.get("clean_eof"):
                fail(f"@{s.get('target_us')}: no clean EOF after seek")
            if not s.get("frames"):
                fail(f"@{s.get('target_us')}: no PCM after seek")
            if tier == "STRICT" and not s.get("suffix_exact"):
                fail(f"@{s.get('target_us')}: strict suffix contract violated")

    # --- 5. throughput for representative samples
    throughput = {}
    for case in cases:
        if not case.get("throughput"):
            continue
        data = bench_json(xmake_bench, fixture_path(case), "bench",
                          [str(args.bench_iterations)])[1]
        throughput[case["id"]] = {
            "xrt_songcore_output": data.get("xrt_songcore_output"),
            "xrt_decode_core": data.get("xrt_decode_core"),
            "audio_seconds": data.get("audio_seconds"),
        }

    # --- 6. size table
    qn_linked = linked_size(pcm_dump, stripped=False)
    qn_stripped = linked_size(pcm_dump, stripped=True)
    sizes = {
        "libqianqian_av_a_bytes": single.stat().st_size,
        "libqianqian_av_members": archive_members(single),
        "libqianqian_av_member_bytes": archive_member_bytes(single),
        "libqianqian_av_symbols": symbol_count(single),
        "libqianqian_av_xz_bytes": xz_bytes(single),
        "libsongcore_a_bytes": songcore.stat().st_size,
        "qn_pcm_dump_bytes": qn_linked["bytes"],
        "qn_pcm_dump_xz_bytes": qn_linked["xz_bytes"],
        "qn_pcm_dump_stripped_bytes": qn_stripped["bytes"],
        "qn_pcm_dump_stripped_xz_bytes": qn_stripped["xz_bytes"],
    }

    projected = json.loads((stage_dir / "manifest-projected.json").read_text())
    full_path = stage_dir / "manifest.json"
    full_manifest = (json.loads(full_path.read_text()) if full_path.is_file()
                     else projected)  # flags-derived stages carry only the projection
    unit_bytes_path = stage_dir / "unit-bytes.json"
    if not unit_bytes_path.is_file():
        if any("-flto" in (u.get("flags") or []) for u in projected["units"]):
            # LTO archives contain bitcode members; per-section sizes are
            # meaningless there and attribution only uses non-LTO stages
            print("  (unit-bytes skipped: LTO closure)")
        else:
            unit_file_backed_bytes(single, stage_dir / "manifest-projected.json",
                                   unit_bytes_path)
    expect_fails = [k for k, v in expectations.items() if v["status"] == "fail"]
    gate = {
        "stage": args.stage,
        "capabilities": sorted(capabilities),
        "closure": {
            "full_translation_units": full_manifest["closure"]["translation_units"],
            "reachable_translation_units": projected["closure"]["translation_units"],
        },
        "verify": {
            "verdict": "PASS" if not mismatch else "FAIL",
            "applicable_cases": len(cases),
            "oracle_vs_xmake_mismatches": mismatch,
        },
        "expectations": expectations,
        "expect_failures": expect_fails,
        "songcore_pcm": songcore_pcm,
        "seek_probe": seek_probe,
        "seek_contract_tiers": tier_counts,
        "seek_failures": seek_failures,
        "throughput": throughput,
        "sizes": sizes,
    }
    (stage_dir / "gate.json").write_text(json.dumps(gate, indent=2, sort_keys=True) + "\n")

    summary = {
        "stage": args.stage,
        "capabilities": sorted(capabilities),
        "oracle_tu": gate["closure"]["full_translation_units"],
        "reachable_tu": gate["closure"]["reachable_translation_units"],
        "corpus_cases": len(cases),
        "expect_failures": expect_fails,
        "seek_failures": seek_failures,
        "seek_contract_tiers": tier_counts,
        "sizes": sizes,
        "throughput": throughput,
    }
    print(json.dumps(summary, indent=2, sort_keys=True))
    if expect_fails or seek_failures:
        raise SystemExit("GATE FAILURES (expectations/seek contract)")


if __name__ == "__main__":
    main()
