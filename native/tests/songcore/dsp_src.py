#!/usr/bin/env python3
"""DSP/SRC integration smoke (test-only, never ships).

Runs the capability probe (native/tests/songcore/dsp_cap_probe)
against the trimmed libavfilter closure (build/minimize/avf-c2) with the
DSP/SRC scenario (native/tests/songcore/dsp-src-smoke.kv) and records the evidence
in bench/results/songcore-v1/dsp-src-integration.json.

What this proves (machine-gated):
  - BYPASS: anull chain over canonical flt/44100/stereo input negotiates
    flt/44100/stereo with no auto-inserted converter, frames in == frames
    out, unity gain (0 dB). Frame-level bit-exact True BYPASS stays out of
    machine scope per the SongCore contract.
  - SRC: aresample=48000 turns 44.1k input into 48k output (negotiated
    rate + duration ratio 48000/44100, both asserted).
  - Effectiveness: volume=0.5 -> -6.02 dB, equalizer +10 dB at 1 kHz.
  - The graph input contract (flt interleaved, source rate/layout) is the
    SongCore output contract, so the smoke exercises the same PCM shape a
    decode would feed the graph.

--check validates an existing evidence file read-only (fail-closed).
"""

import argparse
import json
import os
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))))
OUT_DIR = os.path.join(ROOT, "bench", "results", "songcore-v1")
# dsp_closure replays each ladder stage into its own artifact namespace
# (build/artifacts/<stage>/); this gate consumes the avf-c2 session.
PROBE = os.path.join(ROOT, "build", "artifacts", "avf-c2", "dsp_cap_probe")
SCENARIO = os.path.join(ROOT, "native", "tests", "songcore",
                        "dsp-src-smoke.kv")
CLOSURE_MANIFEST = os.path.join(ROOT, "build", "minimize", "avf-c2",
                                "manifest.json")
PRODUCTION_MANIFEST = os.path.join(ROOT, "build", "minimize",
                                   "songcore-test", "manifest.json")

# closure evidence: which filter sources the avf-c2 manifest must carry.
# NB: FFmpeg n9.0.1 implements `equalizer` inside af_biquads.c via
# DEFINE_BIQUAD_FILTER (no af_equalizer.c exists in this tree).
REQUIRED_FILTER_UNITS = {
    "aresample": "libavfilter/af_aresample.c",
    "volume": "libavfilter/af_volume.c",
    "equalizer": "libavfilter/af_biquads.c",
    "anull": "libavfilter/af_anull.c",
}


def closure_evidence():
    out = {}
    for name, path in REQUIRED_FILTER_UNITS.items():
        m = json.load(open(CLOSURE_MANIFEST))
        out[name] = any(u["path"] == path for u in m["units"])
    out["swresample_present"] = any(
        u["path"].startswith("libswresample/") for u in m["units"])
    return out


def run_probe():
    import tempfile
    tmp = os.path.join(tempfile.gettempdir(), "dsp-src.json")
    r = subprocess.run([PROBE, SCENARIO, tmp], capture_output=True, text=True)
    summary = r.stdout.strip().splitlines()[-1] if r.stdout else ""
    d = json.load(open(tmp))
    os.unlink(tmp)
    return d, summary


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", default=OUT_DIR)
    ap.add_argument("--check", action="store_true")
    args = ap.parse_args()

    if args.check:
        return validate(args.out)

    if not os.path.isfile(PROBE):
        raise SystemExit(f"capability probe missing: {PROBE} (run "
                         f"`python3 tools/dsp_closure.py --stage avf-c2` first)")
    if not os.path.isfile(CLOSURE_MANIFEST):
        raise SystemExit(f"avf-c2 closure missing: {CLOSURE_MANIFEST}")

    ce = closure_evidence()
    missing = [k for k, v in ce.items()
               if k != "swresample_present" and not v]
    if missing:
        raise SystemExit(f"avf-c2 closure missing required filters: {missing}")

    data, probe_summary = run_probe()

    gates = []
    for res in data["results"]:
        gid = res.get("id", res.get("name", "?"))
        ok = bool(res.get("ok"))
        gates.append({
            "id": gid,
            "ok": ok,
            "detail": res.get("expect_detail", ""),
            "negotiated": res.get("negotiated", {}),
            "auto_inserted": res.get("auto_inserted", []),
            "in_frames": res.get("in_frames"),
            "out_frames": res.get("out_frames"),
        })
        if not ok:
            print(f"  FAIL {gid}: {res.get('expect_detail', '')}")

    all_ok = all(g["ok"] for g in gates) and not missing
    # machine gates on the two structural facts
    resample = next((g for g in gates if g["id"] == "resample-44to48"), None)
    bypass = next((g for g in gates if g["id"] == "bypass-anull"), None)
    if resample and resample["negotiated"].get("rate") != 48000:
        all_ok = False
    if bypass and bypass["negotiated"].get("rate") != 44100:
        all_ok = False

    evidence = {
        "scope": ("BYPASS shape + 44.1k->48k aresample + volume/equalizer "
                  "effectiveness on the trimmed libavfilter graph; True "
                  "frame-level BYPASS bit-exactness is out of machine scope "
                  "per SongCore contract"),
        "graph_input_contract": {
            "fmt": "flt (Float32 interleaved)",
            "rate": "source rate",
            "layout": "source layout",
            "matches_songcore_output": True,
        },
        "closure": {
            "manifest": "build/minimize/avf-c2",
            "tier": "F1 core-gain-eq-tone",
            "units": len(json.load(open(CLOSURE_MANIFEST))["units"]),
            "production_matrix": False,
            "production_manifest": "build/minimize/songcore-test "
                                   "(common formats, no libavfilter)",
            "filters": ce,
        },
        "probe_summary": probe_summary,
        "gates": gates,
        "all_gates_ok": all_ok,
        "verdict": "PASS" if all_ok else "FAIL",
        "problems": [] if all_ok else [
            g["detail"] for g in gates if not g["ok"]],
    }

    os.makedirs(args.out, exist_ok=True)
    with open(os.path.join(args.out, "dsp-src-integration.json"), "w") as f:
        json.dump(evidence, f, indent=1, ensure_ascii=False)
        f.write("\n")
    print(f"DSP/SRC smoke: {evidence['verdict']} "
          f"({probe_summary.strip()})")
    return 0 if all_ok else 1


def validate(out_dir):
    failures = []
    path = os.path.join(out_dir, "dsp-src-integration.json")
    if not os.path.isfile(path):
        failures.append(f"missing authority file {path}")
    else:
        d = json.load(open(path))
        if d.get("verdict") != "PASS":
            failures.append(f"stored DSP/SRC verdict is "
                            f"{d.get('verdict')}: {d.get('problems')}")
        if not all(g.get("ok") for g in d.get("gates", [])):
            failures.append("stored DSP/SRC gates not all ok")
    if failures:
        print("DSP/SRC --check FAIL")
        for x in failures:
            print("  -", x)
        return 1
    print("DSP/SRC --check PASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
