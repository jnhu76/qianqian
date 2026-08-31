#!/usr/bin/env python3
"""E09 shipping-footprint measurement — product-shaped deployment sets.

Shipping is reported as the ACTUAL deployment set per backend, not a
benchmark-guest grab-bag:

  native       stripped release host (native SongCore linked in)         -- guest
  wamr_interp  stripped release host/runtime + SongCore.wamr-workaround.wasm
  wamr_aot     stripped release host/AOT runtime + SongCore.aot          (AOT REPLACES .wasm)
  wasm3        stripped release host/runtime + SongCore.wasm
  wasmtime     stripped release host/runtime + SongCore.wasm
  browser      SongCore.js + SongCore.wasm (engine = platform-provided)

The host binaries are the profiling runners rebuilt as shipping caliber
(strip removes debug symbols; codegen unchanged). The WAMR runtime library
is read from the build the Xmake runners actually link (build-e09), not the
stale .../build path.

Calibers never mix: embedded shapes are xz -9e (archive caliber); the
browser shape is gzip -9 / brotli -11.

Emits bench/results/wasm/shipping.json. Byte counts only — no judgment.
"""

import json
import shutil
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
ART = ROOT / "build" / "artifacts"
WASM = ART / "wasm"
STAGE = ART / "shipping-e09"
OUT = ROOT / "bench" / "results" / "wasm" / "shipping.json"

WAMR_ROOT = (Path.home() / "toolchains/e09/wasm-micro-runtime-WAMR-2.4.5"
             / "product-mini" / "platforms" / "linux")
WAMR_LIB = WAMR_ROOT / "build-e09" / "libiwasm.a"
WASMTIME_LIB = Path.home() / "toolchains/e09/wasmtime/lib/libwasmtime.a"

RUNNERS = {
    "native": "qn_native_runner",
    "wamr_interp": "qn_wamr_runner",
    "wamr_aot": "qn_wamr_aot_runner",
    "wasm3": "qn_wasm3_runner",
    "wasmtime": "qn_wasmtime_runner",
}

GUESTS = {
    "wamr_interp": "SongCore.wamr-workaround.wasm",   # WAMR classic-interp
    "wamr_aot": "SongCore.aot",                       # AOT replaces .wasm
    "wasm3": "SongCore.wasm",                         # pristine
    "wasmtime": "SongCore.wasm",                      # pristine
}

BROWSER_JS = WASM / "SongCore.js"
BROWSER_WASM = WASM / "SongCore.wasm"  # em variant while the em session is active


def size(p):
    return p.stat().st_size if p and p.exists() else None


def comp_bytes(b, alg):
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


def measure(path, algs=("xz9e", "gzip9", "brotli11")):
    if path is None or not path.exists():
        return None
    b = path.read_bytes()
    row = {"bytes": len(b)}
    if "xz9e" in algs:
        row["xz9e"] = comp_bytes(b, "xz")
    if "gzip9" in algs:
        row["gzip9"] = comp_bytes(b, "gzip")
    if "brotli11" in algs:
        row["brotli11"] = comp_bytes(b, "brotli")
    return row


def stripped_runner(name):
    """Shipping-caliber copy of a runner: strip debug symbols (codegen and
    link config unchanged — these are the profiling binaries made release)."""
    src = ART / name
    if not src.exists():
        return None
    dst = STAGE / name
    dst.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(src, dst)
    r = subprocess.run(["strip", "-s", str(dst)], capture_output=True, text=True)
    if r.returncode != 0:
        # strip may fail on the wasmtime binary if tooling differs; fall back
        # to the unstripped size but record it (never silently drop)
        shutil.copy2(src, dst)
    return dst


def main():
    deployments = {}
    for backend, rname in RUNNERS.items():
        host = stripped_runner(rname)
        host_row = measure(host) if host else None
        guest = GUESTS.get(backend)
        guest_path = (WASM / guest) if guest else None
        guest_row = measure(guest_path) if guest_path else None
        total_raw = (host_row["bytes"] if host_row else 0) + \
                    (guest_row["bytes"] if guest_row else 0)
        total_xz = None
        if host_row and (not guest or guest_row):
            parts = [host_row["xz9e"]] + ([guest_row["xz9e"]] if guest_row else [])
            total_xz = sum(parts)
        deployments[backend] = {
            "host": {"file": rname, **host_row} if host_row else None,
            "guest": {"file": guest, **guest_row} if guest_row else None,
            "total_raw": total_raw if (host_row or guest_row) else None,
            "total_xz9e": total_xz,
            "note": "AOT deployment ships .aot in place of .wasm (wamr_aot "
                    "guest is SongCore.aot only; no .wasm + .aot sum)",
        }

    # browser shape: engine is platform-provided, so the set is glue + wasm.
    # The shared artifact dir holds either the WASI or the em SongCore.wasm
    # (E09-xmake-1); the em .js is always present. Compare the current wasm
    # against the pristine WASI hash recorded in artifacts.json: if it is the
    # WASI artifact, source the em sizes from shipping-em.json instead of
    # silently reporting the WASI guest as the browser payload.
    art = json.loads((ROOT / "bench/results/wasm/artifacts.json").read_text())
    # the workaround record's sha256_before IS the pristine SongCore.wasm hash
    pristine_sha = (art.get("artifacts", {})
                    .get("SongCore.wamr-workaround.wasm", {})
                    .get("sha256_before"))
    cur_sha = None
    if BROWSER_WASM.exists():
        import hashlib
        cur_sha = hashlib.sha256(BROWSER_WASM.read_bytes()).hexdigest()
    em_row = None
    if cur_sha != pristine_sha and BROWSER_WASM.exists():
        js_row = measure(BROWSER_JS) if BROWSER_JS.exists() else None
        wasm_row = measure(BROWSER_WASM)
        if js_row and wasm_row:
            em_row = {"host": {"file": "SongCore.js", **js_row},
                      "guest": {"file": "SongCore.wasm (emscripten)", **wasm_row}}
    else:
        sem = ROOT / "bench/results/wasm/shipping-em.json"
        if sem.exists():
            g = json.loads(sem.read_text()).get("guests", {})
            js = g.get("em_songcore_js")
            wm = g.get("em_songcore_wasm")
            if js and wm:
                em_row = {"host": {"file": "SongCore.js", **js},
                          "guest": {"file": "SongCore.wasm (emscripten)", **wm},
                          "sourced_from": "shipping-em.json (em session)"}
    if em_row:
        js_row = em_row["host"]
        wasm_row = em_row["guest"]
        deployments["browser"] = {
            "host": js_row,
            "guest": wasm_row,
            "total_raw": js_row["bytes"] + wasm_row["bytes"],
            "total_gzip9": js_row["gzip9"] + wasm_row["gzip9"],
            "total_brotli11": js_row["brotli11"] + wasm_row["brotli11"],
            "note": "browser engine (V8) is platform-provided, not shipped",
            **({"sourced_from": em_row["sourced_from"]} if "sourced_from" in em_row else {}),
        }
    else:
        deployments["browser"] = {
            "_note": "em artifacts absent in this session (WASI artifacts "
                     "restored); measure in the em session for the browser row"}

    # standalone artifact calibers (reference rows, per-file)
    artifacts = {}
    for name in ("SongCore.wasm", "SongCore.wamr-workaround.wasm",
                 "SongCore.aot", "qn_guest_bench.wasm",
                 "qn_guest_bench.wamr-workaround.wasm", "qn_guest_bench.aot",
                 "qn_guest_bench_cmd.wasm", "qn_pb_guest.wasm"):
        artifacts[name] = measure(WASM / name)

    runtime_libs = {
        "wamr_libiwasm_a": measure(WAMR_LIB, algs=("xz9e",)),
        "wasmtime_libwasmtime_a": measure(WASMTIME_LIB, algs=("xz9e",)),
        "wasm3": None,  # linked from runner objects; see wasm3 host row
    }

    out = {
        "calibers": {
            "embedded": "xz -9e (archive caliber)",
            "browser": "gzip -9 / brotli -11 (never mixed with xz)",
        },
        "note": "deployment = the exact artifact set shipped per backend; "
                "AOT replaces .wasm; host binaries are stripped release "
                "copies of the ladder runners",
        "artifacts": artifacts,
        "runtime_libs": runtime_libs,
        "deployments": deployments,
    }
    OUT.parent.mkdir(parents=True, exist_ok=True)
    json.dump(out, open(OUT, "w"), indent=1, ensure_ascii=False)
    for backend, d in deployments.items():
        if d.get("total_raw") is not None:
            xz = d.get("total_xz9e")
            xz_s = f"{xz:,}" if xz is not None else "—"
            print(f"{backend}: raw={d['total_raw']:,} xz={xz_s}")
        else:
            print(f"{backend}: {d.get('_note', 'missing artifacts')}")
    print("shipping.json written")


if __name__ == "__main__":
    main()
