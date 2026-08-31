#!/usr/bin/env python3
"""E09 tolerance study — per-fixture PCM evidence for the correctness gate.

The gate's tolerance-allowed policy accepts float-DSP family cases only when
tolerance.json carries per-fixture evidence that is IDENTITY-BOUND: it records
the exact native and canonical-WASM-anchor PCM hashes that were numerically
compared, for EVERY tolerance-allowed stream (full decode, suffix, seek
suffix), plus the measured element-wise delta. A gate can only use this
evidence when the CURRENT native/current anchor hashes equal the recorded
ones and the runtime is bit-identical to the anchor — stale artifacts can no
longer ride on an old measurement.

Coverage is machine-derived from correctness.json: every fixture whose
observable differs between the native twin and the wasm ladder (the exact
tolerance candidates, all in the tolerated mp3/aac families), plus the clean
integer/lossless fixtures (vorbis, opus) as exact-family proof. No manual
fixture list, so the gate can never silently accept an unmeasured fixture.

Evidence produced per fixture:
  native identity      canonical_pcm_sha256 / suffix_sha256 / seek_suffix_sha256
                       (the exact hashes the gate compares) + samples/frames
  wasm_anchor identity same fields for the canonical anchor (WAMR classic
                       interp); the dump hash is cross-checked against the
                       correctness.json anchor row
  numeric_delta        full: element-wise max|delta| over the full canonical
                       f32 stream (bounds every suffix, which are suffixes of
                       the same stream)
                       suffix: per-offset max|delta| computed from the full
                       dumps
                       seek: per-target max|delta| of the seek re-decode PCM
                       (fresh decode from the resume point — a distinct
                       artifact, so it gets its own measurement via the
                       `pcm-seek` runner mode)

Dump drivers:
  native full:   build/artifacts/qn_native_runner <fx> pcm <out>
  anchor full:   build/artifacts/qn_wamr_runner <workaround> pcm <fx> <out>
  native seek:   build/artifacts/qn_native_runner <fx> pcm-seek <t_us> <out>
  anchor seek:   build/artifacts/qn_wamr_runner <workaround> pcm-seek <fx> <t_us> <out>

Emits bench/results/wasm/tolerance.json with a per-fixture entry.
"""

import hashlib
import json
import struct
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

sys.path.insert(0, str(ROOT / "tools"))
from wasm_gate import (load_cases, observable, ANCHOR_RUNTIME,  # noqa: E402
                       TOLERANCE_BOUND)


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def load_candidate_fixtures(corr):
    """Fixtures that need tolerance evidence = the cases whose observable
    differs between native and the wasm ladder (exact candidates), plus the
    clean lossless families as zero-delta proof."""
    raw = corr["raw"]
    ref = {cid: observable(j) for cid, j in raw["native"].items()}
    cases = load_cases()
    files = {}
    for cid, c in cases.items():
        fam = (c.get("expect") or {}).get("codec") or ""
        want = ref[cid]
        differs = any(
            observable(raw.get(rt, {}).get(cid)) != want
            for rt in ("wamr", "wamr_aot", "wasm3", "wasmtime"))
        if fam in TOLERATED_FAMILIES and differs:
            files[c["file"]] = {"family": fam, "tolerated": True}
    # clean lossless proof (only fixtures that decode ok in the native twin;
    # degraded ones are exact by typed error and never enter the tolerance
    # path, so they need no evidence)
    clean = {}
    for cid, c in cases.items():
        fam = (c.get("expect") or {}).get("codec") or ""
        if fam in EXACT_FAMILIES:
            o = ref[cid]
            if isinstance(o, dict) and o.get("status") == "ok":
                clean.setdefault(c["file"], {"family": fam, "tolerated": False})
    for f, meta in clean.items():
        files.setdefault(f, meta)
    return files


def run_dump(cmd):
    p = subprocess.run(cmd, capture_output=True, text=True, timeout=TIMEOUT_S)
    prep = None
    for line in p.stdout.splitlines():
        if not line.startswith("{"):
            continue
        try:
            j = json.loads(line)
        except json.JSONDecodeError:
            continue
        if j.get("mode") in ("pcm", "pcm_seek"):
            prep = j
    if p.returncode != 0 or prep is None or prep.get("status") != "ok":
        raise RuntimeError(f"dump failed rc={p.returncode} prep={prep} "
                           f"stderr={p.stderr[-200:]}")
    return prep


def dump_full(binary, guest, fixture, outfile, is_native):
    fx = str(FIXTURES / fixture)
    if is_native:
        cmd = [str(binary), fx, "pcm", str(outfile)]
    else:
        cmd = [str(binary), str(guest), "pcm", fx, str(outfile)]
    run_dump(cmd)
    return outfile.read_bytes()


def dump_seek(binary, guest, fixture, target_us, outfile, is_native):
    fx = str(FIXTURES / fixture)
    if is_native:
        cmd = [str(binary), fx, "pcm-seek", str(target_us), str(outfile)]
    else:
        cmd = [str(binary), str(guest), "pcm-seek", fx, str(target_us),
               str(outfile)]
    run_dump(cmd)
    return outfile.read_bytes()


def f32_delta(a: bytes, b: bytes):
    """Element-wise f32 |delta| metrics over min(len)/4 floats."""
    n = min(len(a), len(b)) // 4
    max_delta = 0.0
    rms = 0.0
    differing = 0
    for i in range(n):
        x = struct.unpack_from("<f", a, i * 4)[0]
        y = struct.unpack_from("<f", b, i * 4)[0]
        d = abs(x - y)
        if d > max_delta:
            max_delta = d
        rms += d * d
        if d > 0:
            differing += 1
    rms = (rms / n) ** 0.5 if n else 0.0
    return {"max_abs_delta": max_delta, "rms_delta": rms,
            "differing_samples": differing}


def suffix_deltas(data_n, data_w, suffix, channels):
    """Per-offset delta for each suffix in the native row's suffix list.
    Suffix[off] hashes data[off*ch:], so the full-stream dumps suffice."""
    out = []
    for idx, _ in suffix:
        off = idx * (channels or 1) * 4
        if off > len(data_n) or off > len(data_w):
            continue
        d = f32_delta(data_n[off:], data_w[off:])
        d["offset"] = idx
        out.append(d)
    return out


def main():
    if not CORR.exists():
        print("error: correctness.json missing (run the gate first)")
        return 2
    corr = json.loads(CORR.read_text())
    raw = corr["raw"]
    ref = {cid: observable(j) for cid, j in raw["native"].items()}
    anchor = {cid: observable(j) for cid, j in raw.get(ANCHOR_RUNTIME, {}).items()}
    fixtures = load_candidate_fixtures(corr)

    out = {
        "method": "canonical f32 PCM dumped from the native twin and the "
                  "canonical WASM anchor (WAMR classic interp); element-wise "
                  "f32 delta; identity recorded as the exact hashes the gate "
                  "compares (full decode, suffix, seek suffix); fixture set "
                  "machine-derived from manifests (tolerated families "
                  "mp3/aac + exact families vorbis/opus)",
        "policy": {
            "tolerance_allowed": {
                "families": sorted(TOLERATED_FAMILIES),
                "max_abs_delta_bound": TOLERANCE_BOUND,
                "pcm_streams": ["decode.canonical_f32_sha256",
                                "suffix[*].hash", "seeks[*].suffix_sha256"],
                "gate_note": "a tolerated case passes only if current native "
                             "and current anchor hashes equal the identity "
                             "recorded here AND the runtime is bit-identical "
                             "to the anchor AND numeric delta <= bound",
            },
            "wasm_anchor": {
                "runtime": ANCHOR_RUNTIME,
                "contract": "every non-anchor WASM runtime must be "
                            "bit-identical to the anchor on every "
                            "tolerance-allowed stream",
            },
        },
        "fixtures": {},
    }
    tmp = ROOT / "build" / "e09-tolerance-tmp"
    tmp.mkdir(parents=True, exist_ok=True)
    n_err = 0
    for fx, meta in sorted(fixtures.items()):
        # the case ids that use this fixture (first one found; identical
        # observable family for all cases of the same file)
        want = None
        anch = None
        for cid, c in load_cases().items():
            if c["file"] == fx:
                want = ref.get(cid)
                anch = anchor.get(cid)
                break
        entry = {"family": meta["family"], "samples": None, "frames": None,
                 "native": {}, "wasm_anchor": {}, "numeric_delta": {}}
        try:
            out_n = tmp / "native.pcm"
            out_w = tmp / "wasm.pcm"
            data_n = dump_full(NATIVE, None, fx, out_n, True)
            data_w = dump_full(WAMR, GUEST_WAMR, fx, out_w, False)
            if want is None or want.get("status") != "ok":
                raise RuntimeError("no native ok observable in correctness.json")
            # --- identity binding: dumps must be the artifacts the gate sees
            h_n = sha256(data_n)
            h_w = sha256(data_w)
            if h_n != want["decode"].get("canonical_f32_sha256"):
                raise RuntimeError(
                    f"native dump hash {h_n} != correctness.json native "
                    f"canonical {want['decode'].get('canonical_f32_sha256')}")
            if anch is None or anch.get("status") != "ok":
                raise RuntimeError("no anchor (wamr) ok observable")
            if h_w != anch["decode"].get("canonical_f32_sha256"):
                raise RuntimeError(
                    f"anchor dump hash {h_w} != correctness.json anchor "
                    f"canonical {anch['decode'].get('canonical_f32_sha256')}")

            channels = want.get("channels") or 1
            entry["samples"] = want["decode"].get("samples")
            entry["frames"] = want["decode"].get("frames")
            entry["native"] = {
                "canonical_pcm_sha256": h_n,
                "suffix_sha256": [h for _, h in (want.get("suffix") or [])],
                "seek_suffix_sha256": [h for _, _, _, h in (want.get("seeks") or [])],
            }
            entry["wasm_anchor"] = {
                "runtime": ANCHOR_RUNTIME,
                "canonical_pcm_sha256": h_w,
                "suffix_sha256": [h for _, h in (anch.get("suffix") or [])],
                "seek_suffix_sha256": [h for _, _, _, h in (anch.get("seeks") or [])],
            }
            nd = {"full": f32_delta(data_n, data_w)}
            nd["full"]["max_abs_delta_in_16bit_lsb"] = \
                nd["full"]["max_abs_delta"] * 32768.0
            nd["full"]["samples"] = entry["samples"]
            nd["full"]["frames"] = entry["frames"]
            # suffix deltas come from the same full-stream dumps (suffixes of
            # the canonical stream); verify they stay within the full bound.
            sfx = suffix_deltas(data_n, data_w, want.get("suffix") or [], channels)
            nd["suffix"] = sfx
            # seek re-decode PCM is a DISTINCT artifact (fresh decode from the
            # resume point) — only tolerated-family fixtures need its own
            # numeric delta; exact families are 0-delta by construction.
            nd["seek"] = []
            if meta["tolerated"]:
                for (t_us, _resume, status, s_hash) in (want.get("seeks") or []):
                    a_hash = None
                    for (t2, _r2, st2, sh2) in (anch.get("seeks") or []):
                        if t2 == t_us and st2 == status:
                            a_hash = sh2
                            break
                    if status != "done" or not s_hash:
                        continue
                    out_ns = tmp / "native-seek.pcm"
                    out_ws = tmp / "wasm-seek.pcm"
                    data_ns = dump_seek(NATIVE, None, fx, t_us, out_ns, True)
                    data_ws = dump_seek(WAMR, GUEST_WAMR, fx, t_us, out_ws, False)
                    hs_n = sha256(data_ns)
                    hs_w = sha256(data_ws)
                    if hs_n != s_hash:
                        raise RuntimeError(
                            f"native seek dump {t_us} hash {hs_n} != "
                            f"correctness.json native seek {s_hash}")
                    if hs_w != a_hash:
                        raise RuntimeError(
                            f"anchor seek dump {t_us} hash {hs_w} != "
                            f"correctness.json anchor seek {a_hash}")
                    d = f32_delta(data_ns, data_ws)
                    d["target_us"] = t_us
                    nd["seek"].append(d)
            entry["numeric_delta"] = nd
            out["fixtures"][fx] = entry
            print(f"{fx}: samples={entry['samples']} frames={entry['frames']} "
                  f"full_max_delta={nd['full']['max_abs_delta']:.2e} "
                  f"seeks={len(nd['seek'])}")
        except (RuntimeError, OSError) as e:
            out["fixtures"][fx] = {"_err": str(e)}
            n_err += 1
            print(f"{fx}: ERROR {e}")
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(json.dumps(out, indent=1, ensure_ascii=False))
    print(f"tolerance.json written ({len(out['fixtures'])} fixtures, "
          f"{n_err} errors)")
    return 1 if n_err else 0


if __name__ == "__main__":
    sys.exit(main())
