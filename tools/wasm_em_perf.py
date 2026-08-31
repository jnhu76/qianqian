#!/usr/bin/env python3
"""E09 Emscripten performance driver (reproduces em_performance.json).

The em session's guest artifacts overwrite the shared WASI ones (E09-xmake-1),
so em numbers are collected here, inside the em session, via the Node
harness with the same contracts as the C runners:

  Mode A:  node ... bench bench <fx> [3]       -> bench JSON (guest timing)
  Mode B:  node ... bench pcm <fx>             -> view/copy/hash split
  Mode C:  node ... bench pcm <fx> <frames>    -> chunked pull
  pb:      node ... pb pull [chunk] [iters]    -> isolated boundary microbench

Emits bench/results/wasm/em_performance.json.
"""

import json
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
HARNESS = ROOT / "tools" / "wasm_em_node_harness.mjs"
OUT = ROOT / "bench" / "results" / "wasm" / "em_performance.json"
SHIP_EM = ROOT / "bench" / "results" / "wasm" / "shipping-em.json"
WASM_ART = ROOT / "build" / "artifacts" / "wasm"

FIXTURES = [
    "flac-16-44-stereo.flac", "mp3-cbr-id3v23.mp3", "aac-lc-44-stereo.m4a",
    "opus-48-stereo.opus", "mp3-long.mp3",
]
CHUNKS = [256, 1024, 4096]


def run(args):
    p = subprocess.run(["node", str(HARNESS)] + args,
                       capture_output=True, text=True, timeout=1800)
    for line in reversed(p.stdout.splitlines()):
        if line.startswith("{"):
            try:
                return json.loads(line)
            except json.JSONDecodeError:
                continue
    return {"_err": f"rc={p.returncode} stderr={p.stderr[-200:]}"}


def main():
    fx_path = lambda fx: str(ROOT / "corpus" / "fixtures" / fx)
    mode_a = {}
    for fx in FIXTURES:
        wrap = run(["bench", "bench", fx_path(fx), "3"])
        # unwrap the harness wrapper: `bench` must hold the guest bench JSON
        # (same shape as the C runners' performance.json rows)
        mode_a[fx] = {"bench": wrap.get("json") if isinstance(wrap, dict) else None}
        if not mode_a[fx]["bench"]:
            mode_a[fx] = {"bench": {"_err": str(wrap)[:200]}}
        print(f"[em] {fx} bench ok", flush=True)

    mode_b = run(["bench", "pcm", fx_path("flac-16-44-stereo.flac")])
    print("[em] Mode B ok", flush=True)
    mode_c = {}
    for ch in CHUNKS:
        mode_c[str(ch)] = run(["bench", "pcm",
                               fx_path("flac-16-44-stereo.flac"), str(ch)])
        print(f"[em] Mode C {ch} ok", flush=True)
    em_pb = run(["pb", "pull", "65536", "200"])
    print("[em] pb ok", flush=True)
    em_lifecycle = {}
    for fx in ("flac-16-44-stereo.flac", "mp3-long.mp3"):
        em_lifecycle[fx] = run(["bench", "lifecycle", fx_path(fx)])
        print(f"[em] lifecycle {fx} ok", flush=True)

    out = {"em_mode_a": mode_a, "em_mode_b": mode_b,
           "em_mode_c": mode_c, "em_pb": em_pb,
           "em_lifecycle": em_lifecycle}
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(json.dumps(out, indent=1, ensure_ascii=False))
    print("em_performance.json written")

    # Emscripten shipping shape (browser deployment set) measured here, in
    # the em session, so wasm_shipping.py can join it when the WASI
    # artifacts are restored.
    import hashlib

    def comp_bytes(b, alg):
        if alg == "gzip":
            r = subprocess.run(["gzip", "-9", "-c"], input=b, capture_output=True)
        elif alg == "brotli":
            r = subprocess.run(["brotli", "-q", "11", "-c"], input=b,
                               capture_output=True)
        else:
            return None
        return len(r.stdout)

    def em_measure(name):
        p = WASM_ART / name
        if not p.exists():
            return None
        b = p.read_bytes()
        return {"bytes": len(b),
                "sha256": hashlib.sha256(b).hexdigest(),
                "gzip9": comp_bytes(b, "gzip"),
                "brotli11": comp_bytes(b, "brotli")}

    guests = {f"em_{n.replace('-', '_').replace('.', '_').lower()}": em_measure(n)
              for n in ("SongCore.js", "SongCore.wasm", "qn_guest_bench.js",
                        "qn_guest_bench.wasm", "qn_pb_guest.js",
                        "qn_pb_guest.wasm")}
    ship = {"note": "Emscripten artifacts measured from the em session "
                    "(with -g1: export names preserved for the JS glue); "
                    "the door imports are wired by signature, not name",
            "guests": {k: v for k, v in guests.items() if v is not None}}
    SHIP_EM.write_text(json.dumps(ship, indent=1, ensure_ascii=False))
    print("shipping-em.json written")


if __name__ == "__main__":
    main()
