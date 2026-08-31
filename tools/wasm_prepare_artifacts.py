#!/usr/bin/env python3
"""E09 artifact preparation: pristine -> WAMR-workaround copies + product AOT.

The WASI session now emits PRISTINE toolchain output (xmake.lua no longer
patches in after_build). This script derives the artifacts the runtime
ladder actually consumes and records the exact provenance:

  qn_guest_bench.wamr-workaround.wasm    patched copy for WAMR interp/AOT
  qn_guest_bench_cmd.wamr-workaround.wasm
  SongCore.wamr-workaround.wasm          patched copy for WAMR interp/AOT
  qn_pb_guest.wamr-workaround.wasm       (pb microbench, WAMR only)
  qn_guest_bench.aot                     bench-guest AOT from the WORKAROUND
                                         guest (WAMR AOT traps pristine too)
  SongCore.aot                           product AOT from SongCore.wamr-
                                         workaround.wasm (WAMR consumes .aot)

The patch is the deterministic E09-WAMR-1 workaround (unreachable -> nop in
the _initialize/_start init guard) applied ONLY to the copies; Wasmtime /
wasm3 / Node always consume unmodified bytes. Both WAMR code paths — the
classic interpreter AND the wamrc AOT — trap the pristine guard, so the same
workaround copy feeds both.

Emits bench/results/wasm/artifacts.json: per artifact pre/post SHA256 plus
the exact wamrc command line used for each AOT compile.
"""

import hashlib
import json
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
WASM = ROOT / "build" / "artifacts" / "wasm"
OUT = ROOT / "bench" / "results" / "wasm" / "artifacts.json"
WAMRC = (Path.home() / "toolchains/e09/wasm-micro-runtime-WAMR-2.4.5"
         / "wamr-compiler" / "build-e09" / "wamrc")

sys.path.insert(0, str(ROOT / "tools"))
from wasm_patch_initialize_guard import patch as patch_guard  # noqa: E402


def sha256(p: Path) -> str:
    return hashlib.sha256(p.read_bytes()).hexdigest()


def prepare_workaround(src_name: str, dst_name: str, record: dict):
    src, dst = WASM / src_name, WASM / dst_name
    if not src.exists():
        record[dst_name] = {"_err": "source-missing"}
        return
    shutil.copy2(src, dst)
    before = sha256(dst)
    patch_guard(str(dst))
    after = sha256(dst)
    record[dst_name] = {
        "source": src_name,
        "sha256_before": before,
        "sha256_after": after,
        "patch": "E09-WAMR-1 WAMR init-guard workaround (unreachable->nop; "
                 "WAMR interp and AOT both trap the pristine guard)",
    }


def compile_aot(src_name: str, dst_name: str, record: dict):
    src, dst = WASM / src_name, WASM / dst_name
    if not src.exists():
        record[dst_name] = {"_err": "source-missing"}
        return
    cmd = [str(WAMRC), "-o", str(dst), str(src)]
    r = subprocess.run(cmd, capture_output=True, text=True)
    if r.returncode != 0:
        record[dst_name] = {"_err": f"wamrc failed: {r.stderr[-300:]}"}
        return
    record[dst_name] = {
        "source": src_name,
        "sha256": sha256(dst),
        "bytes": dst.stat().st_size,
        "wamrc_cmd": " ".join(str(c) for c in cmd),
    }


def main():
    record = {}
    for src in ("qn_guest_bench.wasm", "qn_guest_bench_cmd.wasm",
                "SongCore.wasm", "qn_pb_guest.wasm"):
        dst = src.replace(".wasm", ".wamr-workaround.wasm")
        prepare_workaround(src, dst, record)
    # WAMR AOT traps the pristine guard as well (verified: qn_wamr_aot_runner
    # + pristine-derived .aot -> "unreachable" at _initialize), so the AOT
    # artifacts derive from the workaround copies, same as the interp path.
    compile_aot("qn_guest_bench.wamr-workaround.wasm", "qn_guest_bench.aot",
                record)
    compile_aot("SongCore.wamr-workaround.wasm", "SongCore.aot", record)

    out = {"note": "pristine WASI artifacts stay unmodified; workaround "
                   "copies are consumed only by WAMR (classic-interp AND AOT "
                   "both trap the pristine init guard, E09-WAMR-1); AOT "
                   "compiled by wamrc from the workaround guest",
           "wamrc": str(WAMRC),
           "artifacts": record}
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(json.dumps(out, indent=1, ensure_ascii=False))
    for name, v in record.items():
        status = v.get("sha256_after", v.get("sha256", v.get("_err", "?")))
        print(f"{name}: {status}")
    print("artifacts.json written")


if __name__ == "__main__":
    main()
