#!/usr/bin/env python3
"""Benchmark harness v0: run every profile against the Stage A corpus and
emit machine-readable results.

Checks performed here (the bench binary only reports observed facts):
  correctness   open -> probe -> metadata -> artwork -> decode -> seek -> EOF
                with typed failures (open/probe/decode/seek/timeout/crash)
  pcm           strict  : canonical f32 sha256 must equal the corpus manifest
                          (lossless FLAC, spec-forced decode)
                consistency: canonical f32 sha256 must be identical across all
                          profiles for the same fixture (catches trimming-
                          induced decoder differences, MP3 and FLAC alike)
                swr bypass : n3-min vs n3-min-noswr must be byte-identical
  size          per profile from build/<p>/size.json (static libs, linked,
                stripped, compressed, symbols)
  throughput    warm-up + N iterations, median/min/max, x realtime, for
                decode-core (no conversion) and songcore-output (Float32) paths

Usage:
  python3 bench/harness/run_bench.py --out bench/results/runs/<run-id>
      [--profiles n0-full,n1-audio,n2-stage-a,n3-min,n3-min-noswr]
      [--bench-iterations 5] [--timeout 120]
"""
import argparse
import importlib.util
import json
import os
import platform
import shutil
import subprocess
import sys
import time
from importlib.machinery import SourceFileLoader
from importlib.util import module_from_spec

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))


def _load_module(path, name):
    loader = SourceFileLoader(name, path)
    spec = importlib.util.spec_from_loader(name, loader)
    mod = module_from_spec(spec)
    loader.exec_module(mod)
    return mod


bp = _load_module(os.path.join(ROOT, "scripts", "build-profile"),
                  "qianqian_build_profile")  # reuse sha256_file / git_sha

MANIFEST = os.path.join(ROOT, "corpus", "manifest", "stage-a.json")
PIN = os.path.join(ROOT, "bench", "ffmpeg-pin.json")
DEFAULT_PROFILES = ["n0-full", "n1-audio", "n2-stage-a", "n3-min", "n3-min-noswr"]


def run_bench_binary(exe, mode, fixture, extra=None, timeout=120):
    cmd = [exe, mode, fixture] + (extra or [])
    try:
        r = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout)
    except subprocess.TimeoutExpired:
        return {"harness_status": "timeout", "timeout_s": timeout}
    if r.returncode != 0:
        if r.returncode < 0:
            return {"harness_status": "crash", "signal": -r.returncode,
                    "stderr_tail": r.stderr[-500:]}
        # typed failures (open_failed/probe_failed) still emit a JSON report
        try:
            out = json.loads(r.stdout)
            out["exit_code"] = r.returncode
            return out
        except json.JSONDecodeError:
            return {"harness_status": "failed_exit", "exit_code": r.returncode,
                    "stderr_tail": r.stderr[-500:]}
    try:
        return json.loads(r.stdout)
    except json.JSONDecodeError:
        return {"harness_status": "bad_output", "stdout_tail": r.stdout[-500:]}


def observed_metadata(obs):
    m = {}
    for e in obs.get("metadata", []):
        m[e["key"].upper()] = e["value"]
    return m


def check_case(case, obs, expect_cross):
    """Return (status, checks) where status in pass/degraded_pass/fail."""
    checks = {}
    exp = case["expect"]
    st = obs.get("harness_status")

    if st == "timeout":
        return "fail", {"status": "timeout"}
    if st == "crash":
        return "fail", checks | {"status": "crash", "signal": obs.get("signal")}
    if st in ("failed_exit", "bad_output"):
        return "fail", checks | {"status": st, "detail": obs.get("stderr_tail") or obs.get("stdout_tail")}

    if obs.get("status") in ("open_failed", "probe_failed"):
        if exp.get("probe_may_fail"):
            return "degraded_pass", {"status": obs["status"], "allowed": True,
                                     "error": obs.get("error")}
        return "fail", {"status": obs["status"], "error": obs.get("error")}

    fail = []

    # structural
    for k in ("container", "codec", "sample_rate", "channels"):
        if k in exp and obs.get(k) != exp[k]:
            fail.append(f"{k}: observed {obs.get(k)} expected {exp[k]}")
    checks["structural"] = "ok" if not fail else "; ".join(fail)

    # samples
    samples = obs.get("decode", {}).get("samples")
    if exp.get("pcm", {}).get("mode") == "strict":
        if samples != exp["samples"]:
            fail.append(f"samples: observed {samples} expected {exp['samples']} (exact)")
    elif "samples" in exp:
        tol = 2 * 1152
        if abs((samples or 0) - exp["samples"]) > tol:
            fail.append(f"samples: observed {samples} expected ~{exp['samples']} (+/-{tol})")
    if "min_samples" in exp and (samples or 0) < exp["min_samples"]:
        fail.append(f"samples: observed {samples} < min {exp['min_samples']}")
    checks["samples"] = samples

    # pcm strict
    pcm_sha = obs.get("decode", {}).get("canonical_f32_sha256")
    if exp.get("pcm", {}).get("mode") == "strict":
        if pcm_sha != exp["pcm"]["canonical_f32_sha256"]:
            fail.append("pcm_strict: canonical f32 sha256 mismatch")
        checks["pcm_strict"] = pcm_sha == exp["pcm"]["canonical_f32_sha256"]

    # pcm consistency across profiles (collected after all profiles ran)
    checks["pcm_consistent_cross_profile"] = None  # filled by caller

    # metadata
    if exp.get("metadata_absent_or_empty"):
        om = observed_metadata(obs)
        for k in ("TITLE", "ARTIST", "ALBUM"):
            if om.get(k):
                fail.append(f"metadata: expected empty but {k}={om[k]!r}")
    for k, v in (exp.get("metadata") or {}).items():
        if observed_metadata(obs).get(k.upper()) != v:
            fail.append(f"metadata {k}: observed {observed_metadata(obs).get(k.upper())!r} expected {v!r}")
    checks["metadata"] = "ok" if not [f for f in fail if f.startswith("metadata")] else "mismatch"

    # artwork
    art_exp, art_obs = exp.get("artwork"), obs.get("artwork", {})
    if art_exp is None:
        if art_obs.get("found"):
            fail.append("artwork: found but none expected")
    else:
        if not art_obs.get("found"):
            fail.append("artwork: expected but not found")
        elif art_obs.get("sha256") != art_exp["sha256"]:
            fail.append("artwork: sha256 mismatch")
        elif art_obs.get("size") != art_exp["size"]:
            fail.append(f"artwork: size observed {art_obs.get('size')} expected {art_exp['size']}")
    checks["artwork"] = "ok" if not [f for f in fail if f.startswith("artwork")] else "mismatch"

    # decode errors + eof
    eof = obs.get("eof", {})
    errc = obs.get("error_count", 0)
    if exp.get("eof") == "clean":
        if not (eof.get("demux") and eof.get("decoder")):
            fail.append(f"eof: demux={eof.get('demux')} decoder={eof.get('decoder')}")
        if not exp.get("probe_may_fail") and errc and "corrupt" not in case["id"]:
            fail.append(f"decode errors: {errc}")
    else:  # error_or_eof: bounded, no crash, decode terminated
        if not (eof.get("demux") or errc > 0):
            fail.append("degraded: neither eof nor error observed")
    checks["eof"] = eof
    checks["error_count"] = errc

    # seek
    seeks = obs.get("seeks", [])
    if exp.get("seek") == "strict":
        if len(seeks) != 3:
            fail.append(f"seek: expected 3 points, observed {len(seeks)}")
        for i, s in enumerate(seeks):
            if s.get("status") != "done":
                fail.append(f"seek[{i}]: status {s.get('status')}")
                continue
            if not s.get("suffix_match_sequential"):
                fail.append(f"seek[{i}]: suffix mismatch vs sequential decode")
            # AVSEEK_FLAG_BACKWARD must resume at a frame boundary at or
            # before the target (within one generous frame/block window)
            tgt = s.get("target_sample")
            res = s.get("resume_sample", -1)
            if tgt is None or not (tgt - 65536 < res <= tgt):
                fail.append(f"seek[{i}]: resume {res} not at frame boundary <= target {tgt}")
    checks["seek"] = [None if not seeks else
                      {k: s.get(k) for k in ("target_us", "resume_sample",
                                             "suffix_match_sequential", "status")}
                      for s in seeks]

    if fail:
        checks["failures"] = fail
        return "degraded_pass" if case.get("degraded") else "fail", checks
    return ("degraded_pass" if case.get("degraded") else "pass"), checks


def validate_results(out_dir, profiles):
    """Minimal contract check mirroring bench/results/schema.json (stdlib only)."""
    problems = []

    def load(name):
        with open(os.path.join(out_dir, f"{name}.json")) as f:
            return json.load(f)

    meta = load("manifest")
    for k in ("run_id", "started_at", "qianqian_git_sha", "ffmpeg_tag",
              "ffmpeg_commit_sha", "ffmpeg_version", "corpus_id", "platform",
              "arch", "cpu", "profiles", "profile_build"):
        if k not in meta:
            problems.append(f"manifest missing {k}")
    if meta.get("ffmpeg_tag") != "n9.0.1":
        problems.append("manifest ffmpeg_tag != n9.0.1")
    for prof in profiles:
        pb = meta.get("profile_build", {}).get(prof, {})
        if "configure_args_hash" not in pb or pb.get("build_type") != "release-static":
            problems.append(f"manifest profile_build[{prof}] incomplete")

    correctness = load("correctness")
    for prof in profiles:
        for cid, r in correctness.get(prof, {}).items():
            if r.get("status") not in ("pass", "degraded_pass", "fail"):
                problems.append(f"correctness[{prof}][{cid}] bad status")

    sizes = load("size")
    for prof in profiles:
        s = sizes.get(prof, {})
        if "static_libs_total_bytes" not in s or "artifacts" not in s:
            problems.append(f"size[{prof}] incomplete")

    throughput = load("throughput")
    for prof, cases in throughput.items():
        for cid, v in cases.items():
            if v.get("iterations", 0) < 3 or v.get("warmup") != 1:
                problems.append(f"throughput[{prof}][{cid}] insufficient iterations")

    summary = load("summary")
    for entry in summary.get("ladder", []):
        for k in ("profile", "correctness", "static_libs_bytes"):
            if k not in entry:
                problems.append(f"ladder entry missing {k}")

    if problems:
        for p in problems:
            print(f"  [schema] {p}")
    else:
        print("  [schema] results match the contract (bench/results/schema.json)")
    return problems


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", required=True)
    ap.add_argument("--profiles", default=",".join(DEFAULT_PROFILES))
    ap.add_argument("--bench-iterations", type=int, default=5)
    ap.add_argument("--timeout", type=int, default=120)
    args = ap.parse_args()
    profiles = args.profiles.split(",")

    manifest = json.load(open(MANIFEST))
    pin = json.load(open(PIN))

    # corpus integrity first: a drifted fixture poisons every result
    for case in manifest["cases"]:
        p = os.path.join(ROOT, "corpus", "fixtures", case["file"])
        actual = bp.sha256_file(p)
        if actual != case["fixture_sha256"]:
            raise SystemExit(f"fixture sha mismatch: {case['file']} — regenerate corpus")

    os.makedirs(args.out, exist_ok=True)
    results = {"correctness": {}, "throughput": {}, "sizes": {}, "meta": {}}
    pcm_by_case = {}   # case_id -> {profile: sha}
    cross_checks = []

    qsha, dirty = bp.git_sha()
    results["meta"] = {
        "run_id": os.path.basename(args.out.rstrip("/")),
        "started_at": time.strftime("%Y-%m-%dT%H:%M:%S%z"),
        "qianqian_git_sha": qsha,
        "qianqian_git_dirty": dirty,
        "ffmpeg_tag": pin["ffmpeg_tag"],
        "ffmpeg_commit_sha": pin["ffmpeg_commit_sha"],
        "ffmpeg_version": pin["ffmpeg_version"],
        "corpus_id": manifest["corpus_id"],
        "corpus_generator": manifest["generator"]["tool"],
        "platform": platform.system().lower(),
        "arch": platform.machine(),
        "kernel": platform.release(),
        "cpu": platform.processor() or "unknown",
        "bench_iterations": args.bench_iterations,
        "bench_warmup": 1,
        "timeout_s": args.timeout,
        "profiles": profiles,
        "xrt_definition": "audio_seconds / median(decode wall time); "
                          "decode_core = demux+decode only; songcore_output = "
                          "demux+decode+Float32 interleave conversion",
    }

    for prof in profiles:
        pdir = os.path.join(ROOT, "build", prof)
        exe = os.path.join(pdir, "qn_bench")
        meta_p = os.path.join(pdir, "build-meta.json")
        if not (os.path.exists(exe) and os.path.exists(meta_p)):
            raise SystemExit(f"profile {prof} not built — run scripts/build-profile {prof}")
        bmeta = json.load(open(meta_p))
        results["meta"].setdefault("profile_build", {})[prof] = {
            k: bmeta[k] for k in ("configure_args_hash", "compiler_version",
                                  "build_type", "built_at")}

        print(f"=== {prof} ===")
        per_case = {}
        for case in manifest["cases"]:
            fixture = os.path.join(ROOT, "corpus", "fixtures", case["file"])
            obs = run_bench_binary(exe, "correct", fixture, timeout=args.timeout)
            status, checks = check_case(case, obs, cross_checks)
            checks["observed"] = {k: obs.get(k) for k in
                                  ("container", "codec", "sample_rate", "channels",
                                   "duration_us", "nb_streams", "open_ms", "probe_ms")}
            checks["observed"]["decode"] = {k: obs.get("decode", {}).get(k) for k in
                                            ("frames", "samples", "canonical_f32_sha256",
                                             "canonical_len", "decode_ms")}
            sha = (obs.get("decode") or {}).get("canonical_f32_sha256")
            pcm_by_case.setdefault(case["id"], {})[prof] = sha
            per_case[case["id"]] = {"status": status, "checks": checks}
            flag = {"pass": "PASS", "degraded_pass": "DEGRADED-PASS", "fail": "FAIL"}[status]
            print(f"  [{flag}] {case['id']}")
            if status == "fail":
                print(f"         {checks.get('failures')}")
            results["correctness"].setdefault(prof, {})[case["id"]] = \
                per_case[case["id"]]

            if case.get("throughput"):
                tobs = run_bench_binary(exe, "bench", fixture,
                                        [str(args.bench_iterations)], timeout=args.timeout)
                if tobs.get("status") == "ok":
                    results["throughput"].setdefault(prof, {})[case["id"]] = {
                        "iterations": tobs.get("iterations"),
                        "warmup": 1,
                        "audio_seconds": tobs.get("audio_seconds"),
                        "decode_core": tobs.get("decode_core_ms"),
                        "songcore_output": tobs.get("songcore_output_ms"),
                        "xrt_decode_core": tobs.get("xrt_decode_core"),
                        "xrt_songcore_output": tobs.get("xrt_songcore_output"),
                        "peak_rss_kb": tobs.get("peak_rss_kb"),
                        "canonical_f32_sha256": tobs.get("canonical_f32_sha256"),
                        "open_first_ms": tobs.get("open_first_ms"),
                    }
                    t = tobs.get("xrt_songcore_output")
                    print(f"  [bench] {case['id']}: {t}x realtime (songcore-output)")

        # sizes
        size_p = os.path.join(pdir, "size.json")
        if os.path.exists(size_p):
            results["sizes"][prof] = json.load(open(size_p))

    # cross-profile PCM consistency + swr bypass identity
    for case in manifest["cases"]:
        by_prof = pcm_by_case.get(case["id"], {})
        shas = {p: s for p, s in by_prof.items() if s}
        # None = nothing to compare (e.g. every profile correctly refuses to
        # open a pathological fixture); False only when PCM actually differs.
        consistent = (len(set(shas.values())) == 1) if len(shas) >= 2 else None
        cross_checks.append({
            "case": case["id"],
            "consistent": consistent,
            "per_profile": by_prof,
        })
        for prof, per_case in results["correctness"].items():
            if case["id"] in per_case:
                per_case[case["id"]]["checks"]["pcm_consistent_cross_profile"] = consistent
        if consistent is False:
            print(f"  [CROSS-FAIL] {case['id']}: PCM differs across profiles")
    bypass_pairs = [(pcm_by_case[c["id"]].get("n3-min-noswr"),
                     pcm_by_case[c["id"]].get("n3-min"))
                    for c in manifest["cases"]
                    if pcm_by_case[c["id"]].get("n3-min-noswr")
                    and pcm_by_case[c["id"]].get("n3-min")]
    noswr = bool(bypass_pairs) and all(a == b for a, b in bypass_pairs)
    results["meta"]["swr_bypass_pcm_identical"] = noswr
    results["meta"]["swr_bypass_cases_compared"] = len(bypass_pairs)

    # ladder summary
    ladder = []
    for prof in profiles:
        s = results["sizes"].get(prof, {})
        static = s.get("static_libs_total_bytes")
        art = s.get("artifacts", {})
        cases = results["correctness"].get(prof, {})
        npass = sum(1 for r in cases.values() if r["status"] == "pass")
        ndeg = sum(1 for r in cases.values() if r["status"] == "degraded_pass")
        nfail = sum(1 for r in cases.values() if r["status"] == "fail")
        xrt = {cid: v.get("xrt_songcore_output")
               for cid, v in results["throughput"].get(prof, {}).items()}
        ladder.append({
            "profile": prof,
            "correctness": {"pass": npass, "degraded_pass": ndeg, "fail": nfail,
                            "total": len(cases)},
            "static_libs_bytes": static,
            "static_libs_stripped_bytes": s.get("static_libs_stripped_bytes"),
            "static_libs_stripped_xz_bytes": s.get("static_libs_stripped_xz_bytes"),
            "bench_linked_bytes": art.get("bench_linked_bytes"),
            "bench_stripped_bytes": art.get("bench_stripped_bytes"),
            "bench_stripped_xz_bytes": art.get("bench_stripped_xz_bytes"),
            "defined_symbols": s.get("static_libs_defined_symbols"),
            "throughput_xrt_songcore_output": xrt,
        })
    results["ladder"] = ladder

    # copy enabled component lists into the run for provenance work
    comp_dir = os.path.join(args.out, "components")
    os.makedirs(comp_dir, exist_ok=True)
    for prof in profiles:
        src = os.path.join(ROOT, "build", prof, "meta", "enabled-components.txt")
        if os.path.exists(src):
            shutil.copyfile(src, os.path.join(comp_dir, f"enabled-components.{prof}.txt"))
        for f in ("configure-args.txt", "build-meta.json"):
            src = os.path.join(ROOT, "build", prof, f)
            if os.path.exists(src):
                shutil.copyfile(src, os.path.join(comp_dir, f"{f}.{prof}"))

    for name, data in (("manifest", results["meta"]),
                       ("correctness", results["correctness"]),
                       ("size", results["sizes"]),
                       ("throughput", results["throughput"]),
                       ("summary", {"ladder": ladder,
                                    "pcm_cross_profile": cross_checks,
                                    "swr_bypass_pcm_identical": noswr})):
        with open(os.path.join(args.out, f"{name}.json"), "w") as f:
            json.dump(data, f, indent=2, ensure_ascii=False)
            f.write("\n")

    validate_results(args.out, profiles)

    nfail = sum(l["correctness"]["fail"] for l in ladder)
    pcm_concrete = [c for c in cross_checks if c["consistent"] is not None]
    print(f"\nrun written to {args.out}")
    print(f"correctness: {sum(l['correctness']['pass'] for l in ladder)} pass, "
          f"{sum(l['correctness']['degraded_pass'] for l in ladder)} degraded, "
          f"{nfail} fail; swr bypass identical: {noswr}; "
          f"pcm cross-profile: {sum(c['consistent'] for c in pcm_concrete)}"
          f"/{len(pcm_concrete)} comparable cases consistent")
    return 1 if nfail else 0


if __name__ == "__main__":
    sys.exit(main())
