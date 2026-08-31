#!/usr/bin/env python3
"""E09 Emscripten/V8 correctness authority (separate from the C runner gate).

The Emscripten session overwrites the shared WASI guest artifacts (see
E09-xmake-1), so the em row cannot run inside the same correctness gate
invocation. This script runs the 44-case corpus through the Node harness
with the SAME observable + tolerance policy as tools/wasm_gate.py and emits
bench/results/wasm/correctness-em.json, which wasm_summary.py joins into the
single machine authority.

Run inside the Emscripten build session (em .js/.wasm artifacts present in
build/artifacts/wasm). Exit 0 iff the em row has zero rejected cases.
"""

import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
HARNESS = ROOT / "tools" / "wasm_em_node_harness.mjs"
FIXTURES = ROOT / "corpus" / "fixtures"
OUT = ROOT / "bench" / "results" / "wasm" / "correctness-em.json"
TIMEOUT_S = 600

sys.path.insert(0, str(ROOT / "tools"))
from wasm_gate import (load_cases, observable, classify, load_tolerance,  # noqa: E402
                       diff_fields)


def run_correct_em(fixture):
    cmd = ["node", str(HARNESS), "bench", "correct", str(fixture)]
    try:
        p = subprocess.run(cmd, capture_output=True, text=True, timeout=TIMEOUT_S)
    except subprocess.TimeoutExpired:
        return {"_gate": "timeout"}
    lines = [l for l in p.stdout.splitlines() if l.startswith("{")]
    if not lines:
        return {"_gate": "no-json", "_stderr": p.stderr[-200:]}
    try:
        wrap = json.loads(lines[-1])
    except json.JSONDecodeError:
        return {"_gate": "bad-json"}
    j = wrap.get("json")
    if not isinstance(j, dict):
        return {"_gate": "no-guest-json", "_wrap": str(wrap)[:200]}
    if wrap.get("rc") not in (0, 1):
        return {"_gate": f"exit-{wrap.get('rc')}", "_observed": j}
    j["_gate"] = "json"
    return j


def main():
    cases = load_cases()
    tol = load_tolerance()
    ref_path = ROOT / "bench" / "results" / "wasm" / "correctness.json"
    if not ref_path.exists():
        print("error: correctness.json missing (run tools/wasm_gate.py in the "
              "WASI/native session first)")
        return 2
    corr = json.loads(ref_path.read_text())
    ref = {cid: observable(j)
           for cid, j in corr["raw"]["native"].items()}
    # canonical WASM anchor = the WAMR classic-interp row of correctness.json;
    # the Emscripten/V8 row must be bit-identical to it on every
    # tolerance-allowed stream, same contract as the C runners.
    from wasm_gate import ANCHOR_RUNTIME
    anchors = {cid: observable(j)
               for cid, j in corr["raw"].get(ANCHOR_RUNTIME, {}).items()}

    per = {}
    for cid, c in cases.items():
        per[cid] = run_correct_em(FIXTURES / c["file"])

    exact = accepted = rejected = 0
    fails, tol_ok = [], []
    for cid, c in cases.items():
        obs = observable(per[cid])
        kind, reason = classify(c, ref[cid], obs, tol, anchors.get(cid))
        if kind == "exact":
            exact += 1
        elif kind == "tolerated":
            accepted += 1
            tol_ok.append(cid)
        else:
            rejected += 1
            fails.append({"case": cid, "reason": reason,
                          "delta": diff_fields(ref[cid], obs)})

    out = {"corpus": "stage-a-v1 + common-formats-v1 (44 cases)",
           "reference": "native twin (joined from correctness.json)",
           "harness": "node tools/wasm_em_node_harness.mjs bench correct",
           "policy": "same as tools/wasm_gate.py (identity-bound tolerance "
                     "evidence + canonical WASM anchor contract)",
           "summary": {"emscripten": {
               "exact_matches": exact,
               "accepted_with_tolerance": accepted,
               "rejected": rejected,
               "total": len(cases),
               "tolerance_accepted_cases": tol_ok,
               "failures": fails}},
           "raw": {"emscripten": per}}
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(json.dumps(out, indent=1, ensure_ascii=False))
    print(f"emscripten: exact={exact} accepted_with_tolerance={accepted} "
          f"rejected={rejected}")
    for f in fails[:5]:
        print(f"  REJECT {f['case']}: {f['reason']}")
    return 0 if rejected == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
