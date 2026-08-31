#!/usr/bin/env python3
"""E09 shipping-footprint measurement (Layer 1).

Shipping = guest module + runtime share + bridge + (AOT) artifact, measured
from the artifacts actually used by the ladder. gzip/brotli sizes are the
browser-distribution caliber; xz the archive caliber; they are never mixed.

Emits bench/results/wasm/shipping.json. Byte counts only -- no judgment.
"""

import json
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
ART = ROOT / "build" / "artifacts"
WASM = ART / "wasm"
OUT = ROOT / "bench" / "results" / "wasm" / "shipping.json"

RUNTIME_LIBS = {
    "wamr_interp": Path.home() / "toolchains/e09/wasm-micro-runtime-WAMR-2.4.5/product-mini/platforms/linux/build/libiwasm.a",
    "wamr_aot_runtime": Path.home() / "toolchains/e09/wasm-micro-runtime-WAMR-2.4.5/product-mini/platforms/linux/build/libiwasm.a",
    "wasm3": None,  # linked from runner objects; measured via runner binary share below
    "wasmtime": Path.home() / "toolchains/e09/wasmtime/lib/libwasmtime.a",
}


def size(p):
    return p.stat().st_size if p and Path(p).exists() else None


def comp(p, alg):
    if p is None or not Path(p).exists():
        return None
    b = Path(p).read_bytes()
    if alg == "gzip":
        r = subprocess.run(["gzip", "-9", "-c"], input=b, capture_output=True)
    elif alg == "brotli":
        r = subprocess.run(["brotli", "-q", "11", "-c"], input=b,
                           capture_output=True)
    elif alg == "xz":
        r = subprocess.run(["xz", "-9e", "-c"], input=b, capture_output=True)
    else:
        return None
    return len(r.stdout)


def main():
    guests = {
        "wasi_songcore": WASM / "SongCore.wasm",
        "wasi_guest_bench": WASM / "qn_guest_bench.wasm",
        "wasi_guest_bench_aot": WASM / "qn_guest_bench.aot",
        "wasi_guest_pb": WASM / "qn_pb_guest.wasm",
        "em_songcore_js": WASM / "SongCore.js",
        "em_songcore_wasm": WASM / "SongCore.wasm",
        "em_guest_bench_js": WASM / "qn_guest_bench.js",
    }
    # em artifacts: the em .wasm/.js are regenerated in the em session; if the
    # canonical dir currently holds WASI binaries the em sizes are absent.
    rows = {}
    for name, p in guests.items():
        rows[name] = {
            "bytes": size(p),
            "gzip9": comp(p, "gzip"),
            "brotli11": comp(p, "brotli"),
            "xz9e": comp(p, "xz"),
        }
    libs = {}
    for name, p in RUNTIME_LIBS.items():
        if name == "wasm3":
            continue
        libs[name] = {"bytes": size(p), "xz9e": comp(p, "xz")}
    # wasm3 object share: measure the runner binary it statically links into
    runner = ART / "qn_wasm3_runner"
    libs["wasm3_runner_total"] = {"bytes": size(runner)}
    r3 = ART / "qn_wamr_runner"
    libs["wamr_runner_total"] = {"bytes": size(r3)}
    wt = ART / "qn_wasmtime_runner"
    libs["wasmtime_runner_total"] = {"bytes": size(wt)}
    nt = ART / "qn_native_runner"
    libs["native_runner_total"] = {"bytes": size(nt)}

    out = {
        "calibers": {
            "shipping_total": "guest + runtime + bridge + aot (uncompressed "
                              "and per-compression codec)",
            "browser": "gzip -9 / brotli -11 (never mixed with xz)",
            "archive": "xz -9e",
        },
        "guests": rows,
        "runtime_libs": libs,
    }
    OUT.parent.mkdir(parents=True, exist_ok=True)
    json.dump(out, open(OUT, "w"), indent=1)
    print("shipping.json written")


if __name__ == "__main__":
    main()
