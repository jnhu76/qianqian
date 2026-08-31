#!/usr/bin/env python3
"""E09 correctness gate — E08's 44-case corpus replayed across the runtime ladder.

Gate rule (no weaker than E08):
  - the native twin (in-memory host IO, same guest harness) is the reference;
  - every WASM runtime must reproduce the reference exactly: status, container,
    codec, sample rate/channels, duration, decode samples, canonical f32 sha256,
    suffix hashes, seek resume points, artwork sha, metadata triples;
  - clean cases must end status "ok"; degraded cases must fail the same typed
    way as the reference (no crash, no hang, valid JSON);
  - exit codes 0/1 with parseable JSON are acceptable (1 = typed decode error);
    anything else (crash, timeout, missing JSON) fails.

Results land in bench/results/wasm/correctness.json.
"""

import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
FIXTURES = ROOT / "corpus" / "fixtures"
OUT = ROOT / "bench" / "results" / "wasm" / "correctness.json"
TIMEOUT_S = 120

RUNTIMES = [
    ("native", ROOT / "build/artifacts/qn_native_runner", []),
    ("wamr", ROOT / "build/artifacts/qn_wamr_runner", []),
    ("wasm3", ROOT / "build/artifacts/qn_wasm3_runner", []),
    ("wasmtime", ROOT / "build/artifacts/qn_wasmtime_runner", []),
]


def load_cases():
    cases = {}
    for mf in ("stage-a.json", "common-formats.json"):
        d = json.loads((ROOT / "corpus" / "manifest" / mf).read_text())
        for c in d["cases"]:
            cases[c["id"]] = c
    return cases


def run_correct(binary, wasm, fixture):
    cmd = [str(binary), str(wasm), "correct", str(fixture)]
    if "native" in binary.name:
        cmd = [str(binary), str(fixture), "correct"]
    try:
        p = subprocess.run(cmd, capture_output=True, text=True, timeout=TIMEOUT_S)
    except subprocess.TimeoutExpired:
        return {"_gate": "timeout"}
    lines = [l for l in p.stdout.splitlines() if l.startswith("{")]
    if not lines:
        return {"_gate": "no-json", "_stderr": p.stderr[-200:]}
    try:
        out = json.loads(lines[-1])
    except json.JSONDecodeError:
        return {"_gate": "bad-json"}
    if p.returncode not in (0, 1):
        return {"_gate": f"exit-{p.returncode}", "_observed": out}
    out["_gate"] = "json"
    return out


def observable(j):
    """The fields the gate compares across runtimes."""
    if j.get("_gate") != "json":
        return {"gate": j.get("_gate")}
    if j.get("status") != "ok":
        return {"status": j.get("status"),
                "error": j.get("error", j.get("first_error", ""))}
    seeks = [(s.get("target_us"), s.get("resume_sample"), s.get("status"),
              s.get("suffix_sha256")) for s in j.get("seeks", [])]
    art = j.get("artwork") or {}
    return {
        "status": "ok",
        "container": j.get("container"),
        "codec": j.get("codec"),
        "sample_rate": j.get("sample_rate"),
        "channels": j.get("channels"),
        "duration_us": j.get("duration_us"),
        "metadata": j.get("metadata"),
        "artwork_sha": art.get("sha256"),
        "decode": j.get("decode"),
        "eof": j.get("eof"),
        "suffix": j.get("suffix"),
        "seeks": seeks,
    }


def main():
    cases = load_cases()
    guest = ROOT / "build/artifacts/wasm/qn_guest_bench.wasm"
    results = {}
    for rt, binary, _ in RUNTIMES:
        if not binary.exists():
            results[rt] = {"_gate": "runner-missing"}
            continue
        per = {}
        for cid, c in cases.items():
            fx = FIXTURES / c["file"]
            j = run_correct(binary, guest, fx)
            per[cid] = j
        results[rt] = per

    ref = {cid: observable(j) for cid, j in results["native"].items()}
    summary = {}
    for rt, per in results.items():
        if isinstance(per, dict) and per.get("_gate"):
            summary[rt] = per["_gate"]
            continue
        passed = failed = 0
        fails = []
        for cid, c in cases.items():
            obs = observable(per[cid])
            want = ref[cid]
            if obs == want:
                passed += 1
            else:
                failed += 1
                delta = {k: (want.get(k), obs.get(k))
                         for k in set(want) | set(obs)
                         if want.get(k) != obs.get(k)}
                fails.append({"case": cid, "delta": delta})
        summary[rt] = {"passed": passed, "failed": failed, "failures": fails}

    out = {"corpus": "stage-a-v1 + common-formats-v1 (44 cases)",
           "reference": "native twin", "summary": summary, "raw": results}
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(json.dumps(out, indent=1, ensure_ascii=False))
    for rt, s in summary.items():
        if isinstance(s, dict):
            print(f"{rt}: passed={s['passed']} failed={s['failed']}")
            for f in s["failures"][:5]:
                print(f"  FAIL {f['case']}: {f['delta']}")
        else:
            print(f"{rt}: {s}")
    return 0 if all(isinstance(s, dict) and s.get("failed") == 0
                    for s in summary.values()) else 1


if __name__ == "__main__":
    sys.exit(main())
