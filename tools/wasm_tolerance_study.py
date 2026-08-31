#!/usr/bin/env python3
"""E09 tolerance study — per-fixture PCM evidence for the correctness gate.

The gate's tolerance-allowed policy accepts float-DSP family cases only when
tolerance.json carries per-fixture evidence: equal sample/frame counts and a
bounded element-wise |delta| between the native twin's canonical f32 PCM and
the wasm canonical f32 PCM. This script produces that evidence.

Coverage is machine-derived from correctness.json: every fixture whose
observable differs between the native twin and the wasm ladder (the exact
tolerance candidates, all in the tolerated mp3/aac families), plus the clean
integer/lossless fixtures (vorbis, opus) as exact-family proof. No manual
fixture list, so the gate can never silently accept an unmeasured fixture.

  native dump:  build/artifacts/qn_native_runner <fx> pcm <out>
  wasm dump:    build/artifacts/qn_wamr_runner <workaround> pcm <fx> <out>
                (all five wasm runtimes are inter-runtime bit-identical; the
                gate + correctness-em.json verify that including V8)

Emits bench/results/wasm/tolerance.json with a per-fixture entry.
"""

import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
ART = ROOT / "build" / "artifacts"
WASM = ART / "wasm"
FIXTURES = ROOT / "corpus" / "fixtures"
OUT = ROOT / "bench" / "results" / "wasm" / "tolerance.json"
CORR = ROOT / "bench" / "results" / "wasm" / "correctness.json"

NATIVE = ART / "qn_native_runner"
WAMR = ART / "qn_wamr_runner"
GUEST_WAMR = WASM / "qn_guest_bench.wamr-workaround.wasm"

TOLERATED_FAMILIES = {"mp3", "aac"}
# bonus: integer/lossless families proven bit-exact (0 delta)
EXACT_FAMILIES = {"vorbis", "opus"}
TIMEOUT_S = 900


def load_candidate_fixtures():
    """Fixtures that need tolerance evidence = the cases whose observable
    differs between native and the wasm ladder (exact candidates), plus the
    clean lossless families as zero-delta proof."""
    sys.path.insert(0, str(ROOT / "tools"))
    from wasm_gate import load_cases, observable  # noqa: E402
    cases = load_cases()
    corr = json.loads(CORR.read_text())
    raw = corr["raw"]
    ref = {cid: observable(j) for cid, j in raw["native"].items()}
    files = {}
    for cid, c in cases.items():
        fam = (c.get("expect") or {}).get("codec") or ""
        want = ref[cid]
        differs = any(
            observable(raw.get(rt, {}).get(cid)) != want
            for rt in ("wamr", "wamr_aot", "wasm3", "wasmtime"))
        if fam in TOLERATED_FAMILIES and differs:
            files[c["file"]] = {"family": fam}
    # clean lossless proof (only fixtures that decode ok in the native twin;
    # degraded ones are exact by typed error and never enter the tolerance
    # path, so they need no evidence)
    clean = {}
    for cid, c in cases.items():
        fam = (c.get("expect") or {}).get("codec") or ""
        if fam in EXACT_FAMILIES:
            o = ref[cid]
            if isinstance(o, dict) and o.get("status") == "ok":
                clean.setdefault(c["file"], {"family": fam})
    for f, meta in clean.items():
        files.setdefault(f, meta)
    return files


def dump(binary, guest, fixture, outfile, is_native):
    fx = str(FIXTURES / fixture)
    if is_native:
        cmd = [str(binary), fx, "pcm", str(outfile)]
    else:
        cmd = [str(binary), str(guest), "pcm", fx, str(outfile)]
    p = subprocess.run(cmd, capture_output=True, text=True, timeout=TIMEOUT_S)
    prep = None
    for line in p.stdout.splitlines():
        if not line.startswith("{"):
            continue
        try:
            j = json.loads(line)
        except json.JSONDecodeError:
            continue
        if j.get("mode") == "pcm":
            prep = j
    if p.returncode != 0 or prep is None or prep.get("status") != "ok":
        raise RuntimeError(f"dump failed rc={p.returncode} prep={prep} "
                           f"stderr={p.stderr[-200:]}")
    data = outfile.read_bytes()
    return prep, data


def compare(prep_n, data_n, prep_w, data_w):
    samples = prep_n.get("samples")
    frames = prep_n.get("frames")
    channels = prep_n.get("channels")
    if data_n != data_w and (len(data_n) != len(data_w)):
        raise RuntimeError(f"byte length mismatch native={len(data_n)} "
                           f"wasm={len(data_w)}")
    n = min(len(data_n), len(data_w)) // 4
    import struct
    import math
    max_delta = 0.0
    rms = 0.0
    differing = 0
    for i in range(n):
        a = struct.unpack_from("<f", data_n, i * 4)[0]
        b = struct.unpack_from("<f", data_w, i * 4)[0]
        d = abs(a - b)
        if d > max_delta:
            max_delta = d
        rms += d * d
        if d > 0:
            differing += 1
    rms = math.sqrt(rms / n) if n else 0.0
    lsb = max_delta * 32768.0 if max_delta > 0 else 0.0
    return {
        "samples": samples,
        "frames": frames,
        "channels": channels,
        "max_abs_delta": max_delta,
        "rms_delta": rms,
        "differing_samples": differing,
        "max_abs_delta_in_16bit_lsb": lsb,
    }


def main():
    if not CORR.exists():
        print("error: correctness.json missing (run the gate first)")
        return 2
    fixtures = load_candidate_fixtures()
    out = {"method": "canonical f32 PCM dumped from the native twin "
                     "(qn_native_runner pcm <fx> <out>) and WAMR Mode B "
                     "(qn_wamr_runner pcm <fx> <out>); element-wise f32 "
                     "delta; fixture set machine-derived from manifests "
                     "(tolerated families mp3/aac + exact families "
                     "vorbis/opus)",
           "gate_bound_note": "correctness gate accepts a tolerated case only "
                              "if this file carries its fixture with equal "
                              "sample/frame counts and max_abs_delta within "
                              "the gate policy bound",
           "fixtures": {}}
    tmp = ROOT / "build" / "e09-tolerance-tmp"
    tmp.mkdir(parents=True, exist_ok=True)
    for fx, meta in sorted(fixtures.items()):
        out_n = tmp / "native.pcm"
        out_w = tmp / "wasm.pcm"
        try:
            prep_n, data_n = dump(NATIVE, None, fx, out_n, True)
            prep_w, data_w = dump(WAMR, GUEST_WAMR, fx, out_w, False)
            entry = compare(prep_n, data_n, prep_w, data_w)
            entry["family"] = meta["family"]
            out["fixtures"][fx] = entry
            print(f"{fx}: samples={entry['samples']} frames={entry['frames']} "
                  f"max_delta={entry['max_abs_delta']:.2e} "
                  f"differing={entry['differing_samples']}")
        except RuntimeError as e:
            out["fixtures"][fx] = {"_err": str(e)}
            print(f"{fx}: ERROR {e}")
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(json.dumps(out, indent=1, ensure_ascii=False))
    n_err = sum(1 for v in out["fixtures"].values() if "_err" in v)
    print(f"tolerance.json written ({len(out['fixtures'])} fixtures, "
          f"{n_err} errors)")
    return 1 if n_err else 0


if __name__ == "__main__":
    sys.exit(main())
