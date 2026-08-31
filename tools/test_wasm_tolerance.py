#!/usr/bin/env python3
"""E09 tolerance-gate NEGATIVE tests (the P0 closure proof).

These tests prove the tolerance gate cannot authorize arbitrary PCM:

  old evidence binds to the exact native + canonical-WASM-anchor hashes
  recorded at measurement time; a runtime whose PCM changed in ANY tolerated
  stream (full / suffix / seek suffix) must be REJECTED even though the
  structure (samples/frames/status/metadata/seeks) is unchanged, and stale
  evidence (native or anchor side) must also be REJECTED.

Scenarios (synthetic, built from the REAL committed correctness.json rows):
  - exact path:           obs == native          -> exact
  - happy tolerance:      obs == anchor (wamr)   -> accepted_with_tolerance
  - arbitrary PCM change: obs hash != anchor hash -> REJECT
  - suffix hash change:   same, suffix stream    -> REJECT
  - seek suffix change:   same, seek stream      -> REJECT
  - stale native evidence: evidence.native hash != current native -> REJECT
  - stale anchor evidence: evidence.wasm_anchor != current anchor  -> REJECT
  - bound exceeded:       measured delta > bound -> REJECT

Run:  python3 tools/test_wasm_tolerance.py   (exit 0 = all PASS)
"""

import copy
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CORR = ROOT / "bench" / "results" / "wasm" / "correctness.json"

sys.path.insert(0, str(ROOT / "tools"))
from wasm_gate import (load_cases, observable, classify,  # noqa: E402
                       ANCHOR_RUNTIME, TOLERANCE_BOUND)

FAILURES = []


def check(name, cond, detail=""):
    status = "PASS" if cond else "FAIL"
    print(f"  [{status}] {name}" + (f" — {detail}" if detail and not cond else ""))
    if not cond:
        FAILURES.append(name)


def clone(o):
    return copy.deepcopy(o)


def build_evidence(case, want, anchor):
    """Synthetic but REAL-hash evidence: identity from the actual rows,
    numeric delta nominal (the tests exercise the BINDING, not the measured
    value — the real measurement lives in tolerance.json)."""
    return {
        "samples": want["decode"].get("samples"),
        "frames": want["decode"].get("frames"),
        "native": {
            "canonical_pcm_sha256": want["decode"]["canonical_f32_sha256"],
            "suffix_sha256": [h for _, h in (want.get("suffix") or [])],
            "seek_suffix_sha256": [h for _, _, _, h in (want.get("seeks") or [])],
        },
        "wasm_anchor": {
            "runtime": ANCHOR_RUNTIME,
            "canonical_pcm_sha256": anchor["decode"]["canonical_f32_sha256"],
            "suffix_sha256": [h for _, h in (anchor.get("suffix") or [])],
            "seek_suffix_sha256": [h for _, _, _, h in (anchor.get("seeks") or [])],
        },
        "numeric_delta": {
            "full": {"max_abs_delta": 1e-7, "rms_delta": 1e-9,
                     "differing_samples": 100},
        },
    }


def mutate_canonical_hash(o, new_hash=None):
    o = clone(o)
    o["decode"]["canonical_f32_sha256"] = new_hash or ("0" * 64)
    return o


def mutate_suffix_hash(o):
    o = clone(o)
    suffix = list(o.get("suffix") or [])
    if suffix:
        suffix[0] = (suffix[0][0], "1" * 64)
        o["suffix"] = suffix
    return o


def mutate_seek_hash(o):
    o = clone(o)
    seeks = list(o.get("seeks") or [])
    if seeks:
        t, r, st, _ = seeks[0]
        seeks[0] = (t, r, st, "2" * 64)
        o["seeks"] = seeks
    return o


def main():
    if not CORR.exists():
        print("error: correctness.json missing — run tools/wasm_gate.py first")
        return 2
    corr = json.loads(CORR.read_text())
    cases = load_cases()
    raw = corr["raw"]
    ref = {cid: observable(j) for cid, j in raw["native"].items()}
    anchors = {cid: observable(j) for cid, j in raw.get(ANCHOR_RUNTIME, {}).items()}
    tol_cases = (corr.get("summary") or {}).get(ANCHOR_RUNTIME, {}).get(
        "tolerance_accepted_cases") or []

    if not tol_cases:
        print("error: no tolerated cases in correctness.json — the tolerance "
              "path cannot be exercised")
        return 2
    cid = tol_cases[0]
    case = cases[cid]
    want = ref[cid]
    anchor = anchors.get(cid)
    if anchor is None or want.get("status") != "ok":
        print(f"error: case {cid} missing ok native/anchor row")
        return 2
    print(f"scenario fixture: {case['file']} (case {cid}, family "
          f"{case.get('expect', {}).get('codec')})")
    print(f"policy: tolerance bound={TOLERANCE_BOUND}, anchor={ANCHOR_RUNTIME}")

    tol = {case["file"]: build_evidence(case, want, anchor)}

    # 1. exact path
    kind, _ = classify(case, want, clone(want), tol, anchor)
    check("exact path: obs == native -> exact", kind == "exact", kind)

    # 2. happy tolerance: runtime bit-identical to anchor, evidence binds
    kind, reason = classify(case, want, clone(anchor), tol, anchor)
    check("happy tolerance: obs == canonical anchor -> accepted_with_tolerance",
          kind == "tolerated", reason)

    # 3. arbitrary full-decode PCM regression (structure unchanged)
    kind, reason = classify(case, want, mutate_canonical_hash(anchor), tol, anchor)
    check("NEGATIVE: arbitrary canonical PCM hash -> REJECT",
          kind == "rejected", reason)
    check("  ... reason is the anchor contract, not a silent pass",
          "anchor" in (reason or ""), reason or "")

    # 4. suffix PCM regression (structure unchanged)
    kind, reason = classify(case, want, mutate_suffix_hash(anchor), tol, anchor)
    check("NEGATIVE: arbitrary suffix PCM hash -> REJECT",
          kind == "rejected", reason)

    # 5. seek suffix PCM regression (structure unchanged)
    kind, reason = classify(case, want, mutate_seek_hash(anchor), tol, anchor)
    check("NEGATIVE: arbitrary seek-suffix PCM hash -> REJECT",
          kind == "rejected", reason)

    # 6. stale NATIVE evidence
    stale = {case["file"]: build_evidence(case, want, anchor)}
    stale[case["file"]]["native"]["canonical_pcm_sha256"] = "a" * 64
    kind, reason = classify(case, want, clone(anchor), stale, anchor)
    check("NEGATIVE: stale native evidence (hash mismatch) -> REJECT",
          kind == "rejected", reason)

    # 7. stale ANCHOR evidence
    stale = {case["file"]: build_evidence(case, want, anchor)}
    stale[case["file"]]["wasm_anchor"]["canonical_pcm_sha256"] = "b" * 64
    kind, reason = classify(case, want, clone(anchor), stale, anchor)
    check("NEGATIVE: stale anchor evidence (hash mismatch) -> REJECT",
          kind == "rejected", reason)

    # 8. stale native SUFFIX evidence
    stale = {case["file"]: build_evidence(case, want, anchor)}
    stale[case["file"]]["native"]["suffix_sha256"] = ["c" * 64]
    kind, reason = classify(case, want, clone(anchor), stale, anchor)
    check("NEGATIVE: stale native suffix evidence -> REJECT",
          kind == "rejected", reason)

    # 9. bound exceeded
    over = {case["file"]: build_evidence(case, want, anchor)}
    over[case["file"]]["numeric_delta"]["full"]["max_abs_delta"] = \
        TOLERANCE_BOUND * 10
    kind, reason = classify(case, want, clone(anchor), over, anchor)
    check("NEGATIVE: measured delta > bound -> REJECT", kind == "rejected",
          reason)

    # 10. missing evidence
    kind, reason = classify(case, want, clone(anchor), {}, anchor)
    check("NEGATIVE: no evidence -> REJECT", kind == "rejected", reason)

    # 11. no anchor available
    kind, reason = classify(case, want, clone(anchor), tol, None)
    check("NEGATIVE: no canonical anchor -> REJECT", kind == "rejected", reason)

    print()
    if FAILURES:
        print(f"FAILED: {len(FAILURES)} test(s): {', '.join(FAILURES)}")
        return 1
    print("ALL NEGATIVE TESTS PASS — old tolerance evidence cannot "
          "authorize arbitrary PCM")
    return 0


if __name__ == "__main__":
    sys.exit(main())
