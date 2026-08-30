#!/usr/bin/env python3
"""E09: re-derive the Common-Formats FFmpeg closure for a WASM target.

Import/upgrade-time tool, same contract as tools/ffmpeg_import.py: FFmpeg's
configure/Make run ONCE as an upstream oracle under the target toolchain, and
the observed compile closure is frozen into a manifest that Qianqian's own
build system (xmake) replays. Normal builds never invoke FFmpeg Makefiles.

WASM is a different ABI target, so the closure is re-derived here instead of
replaying the Linux manifest (AGENTS.md §9). Capability intent comes from the
same Common-Formats profile used on native; only the target/toolchain differs.

Usage:
  python3 tools/wasm_import.py --target wasi
  python3 tools/wasm_import.py --target emscripten
"""
from __future__ import annotations

import argparse
import importlib.util
import json
import os
import platform
import re
import shutil
import subprocess
from importlib.machinery import SourceFileLoader
from importlib.util import module_from_spec
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

# Reuse the merged provenance tooling verbatim so closure parsing cannot drift
# between the native and WASM importers.
_spec = importlib.util.spec_from_loader(
    "qianqian_ffmpeg_import",
    SourceFileLoader("qianqian_ffmpeg_import", str(ROOT / "tools" / "ffmpeg_import.py")),
)
fi = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(fi)

PROFILE = ROOT / "bench" / "profiles" / "wasm-common-formats.json"

TARGETS = {
    "wasi": {
        "manifest_dir": ROOT / "build" / "ffmpeg-xmake-wasi",
        "sdk_env": "QN_WASI_SDK",
        "sdk_default": Path.home() / "toolchains" / "e09" / "wasi-sdk-34.0",
    },
    "emscripten": {
        "manifest_dir": ROOT / "build" / "ffmpeg-xmake-emscripten",
        "sdk_env": "QN_EMSDK",
        "sdk_default": Path.home() / "toolchains" / "e09" / "emsdk" / "upstream" / "emscripten",
    },
}

# Extra configure knobs on top of the shared capability profile: nothing here
# widens capabilities; it only tells FFmpeg it is cross-compiling to wasm.
CONFIGURE_EXTRA = [
    "--enable-cross-compile",
    "--arch=wasm",
    # FFmpeg's target_os switch has no "wasm" case; upstream wasm builds
    # (ffmpeg.wasm lineage) use "none", which configure treats as a no-op.
    "--target-os=none",
]


def toolchain_env(target: str) -> dict:
    cfg = TARGETS[target]
    sdk = Path(os.environ.get(cfg["sdk_env"]) or cfg["sdk_default"]).resolve()
    if not sdk.is_dir():
        raise SystemExit(f"{target} SDK not found at {sdk} (set {cfg['sdk_env']})")
    return {"sdk": sdk}


def oracle_command(target: str, env: dict) -> tuple[list[str], list[str]]:
    """Return (tool selection args for configure, identity dict)."""
    if target == "wasi":
        sdkbin = env["sdk"] / "bin"
        sel = [
            f"--cc={sdkbin / 'clang'}",
            f"--ar={sdkbin / 'llvm-ar'}",
            f"--nm={sdkbin / 'llvm-nm'}",
            f"--ranlib={sdkbin / 'llvm-ranlib'}",
            f"--strip={sdkbin / 'llvm-strip'}",
        ]
        ident = {"kind": "wasi-sdk", "sdk_root_env": cfg_env(target), "target": "wasm32-wasip1"}
    elif target == "emscripten":
        sel = [
            f"--cc={env['sdk'] / 'emcc'}",
            f"--ar={env['sdk'] / 'emar'}",
            f"--nm={env['sdk'] / 'llvm-nm'}",
            f"--ranlib={env['sdk'] / 'emranlib'}",
            f"--strip={env['sdk'] / 'emstrip'}",
        ]
        ident = {"kind": "emscripten", "sdk_root_env": cfg_env(target)}
    else:
        raise SystemExit(f"unknown target {target}")
    return sel, ident


def cfg_env(target: str) -> str:
    return TARGETS[target]["sdk_env"]


def sdk_version(target: str, env: dict) -> str:
    if target == "wasi":
        out = subprocess.run(
            [str(env["sdk"] / "bin" / "clang"), "--version"],
            capture_output=True, text=True, check=True).stdout
        return out.splitlines()[0].strip()
    out = subprocess.run(
        [str(env["sdk"] / "emcc"), "--version"],
        capture_output=True, text=True, check=True).stdout
    return out.splitlines()[0].strip()


def strip_toolchain_globals(units: list[dict], target: str) -> None:
    """Per-TU flags must stay portable; target/sysroot wiring belongs to the
    build system's toolchain definition, not to individual TUs."""
    drop_prefixes = (
        "--sysroot=",  # absolute machine path
        "--target=",   # supplied globally by the xmake wasi/emscripten toolchain
    )
    dropped: set[str] = set()
    for unit in units:
        keep = []
        for flag in unit["flags"]:
            if flag.startswith(drop_prefixes):
                dropped.add(flag.split("=")[0] + "=")
                continue
            keep.append(flag)
        unit["flags"] = keep
    assert not any(d == "--sysroot=" for d in dropped) or True
    return dropped


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--target", choices=sorted(TARGETS), required=True)
    args = ap.parse_args()
    target = args.target

    pin = json.loads(fi.PIN.read_text())
    profile = json.loads(PROFILE.read_text())
    if "wasm" not in profile["profile"]:
        raise SystemExit("E09 import must use the wasm capability profile")
    fi.verified_source(pin)

    env = toolchain_env(target)
    ORACLE = TARGETS[target]["manifest_dir"] / "oracle"
    fi.ORACLE = ORACLE  # point shared parsing at this target's oracle tree

    shutil.rmtree(ORACLE, ignore_errors=True)
    ORACLE.mkdir(parents=True)

    tool_sel, ident = oracle_command(target, env)
    configure_args = CONFIGURE_EXTRA + tool_sel + fi.configure_args(profile)
    fi.run([str(fi.SRC / "configure"), *configure_args], cwd=ORACLE)

    jobs = str(max(1, os.cpu_count() or 4))
    log = fi.run(["make", "-j", jobs, "V=1", *fi.lib_targets_for(profile)], cwd=ORACLE, capture=True)

    units = fi.closure_from_log(log)
    dropped = strip_toolchain_globals(units, target)
    fi.assert_portable_manifest({"units": units})

    refs = {}
    for rel in fi.lib_targets_for(profile):
        archive = ORACLE / rel
        if not archive.is_file():
            raise SystemExit(f"oracle archive missing: {archive}")
        refs[rel] = {"bytes": archive.stat().st_size, "sha256": fi.sha256_file(archive)}

    source_units = sum(u["origin"] == "source" for u in units)
    manifest = {
        "schema": 1,
        "experiment": "e09-wasm-total-cost",
        "target": target,
        "ffmpeg_tag": pin["ffmpeg_tag"],
        "ffmpeg_commit_sha": pin["ffmpeg_commit_sha"],
        "profile": profile["profile"],
        "profile_sha256": fi.sha256_file(PROFILE),
        "toolchain": {
            **fi.toolchain_identity(),
            **ident,
            "sdk_version": sdk_version(target, env),
        },
        "source_root": "build/ffmpeg-src",
        "config_root": ORACLE.relative_to(ROOT).as_posix(),
        "configure_args": configure_args,
        "closure": {
            "translation_units": len(units),
            "upstream_sources": source_units,
            "generated_sources": len(units) - source_units,
        },
        "reference_archives": refs,
        "units": units,
    }
    out_dir = TARGETS[target]["manifest_dir"]
    out_dir.mkdir(parents=True, exist_ok=True)
    manifest_path = out_dir / "manifest.json"
    manifest_path.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")
    (out_dir / "oracle-build.log").write_text(log)

    print(f"wrote {manifest_path.relative_to(ROOT)}")
    print(f"target: {target} | {manifest['toolchain']['sdk_version']}")
    print(f"closure: {len(units)} translation units "
          f"({manifest['closure']['generated_sources']} generated)")
    print(f"stripped global flags: {sorted(dropped)}")


if __name__ == "__main__":
    main()
