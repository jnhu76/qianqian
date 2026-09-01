#!/usr/bin/env python3
"""Measure the current SongCore ABI v1 reference artifacts (machine-derived).

Writes bench/results/songcore-v1/reference-artifacts.json: raw/stripped/
xz -9e sizes, sha256, shared exports, and dynamic dependencies for
libsongcore.a and libsongcore.so. The shipping size authority is the final
linked artifact — never a source-count proxy.

    python3 tools/measure_songcore_artifacts.py          # measure + write
    python3 tools/measure_songcore_artifacts.py --stdout # print only
"""
from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
STATIC = ROOT / "build" / "artifacts" / "libsongcore.a"
SO = ROOT / "build" / "artifacts" / "shared" / "libsongcore.so"
OUT = ROOT / "bench" / "results" / "songcore-v1" / "reference-artifacts.json"

CONTRACT = [
    "song_audio_stream_count", "song_audio_stream_info", "song_close",
    "song_get_artwork_count", "song_get_artwork_item", "song_get_metadata",
    "song_get_metadata_count", "song_get_metadata_entry", "song_last_error",
    "song_open", "song_probe", "song_read_pcm", "song_seek",
    "song_select_stream", "songcore_abi_version",
]


def sha256(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for block in iter(lambda: f.read(1 << 20), b""):
            h.update(block)
    return h.hexdigest()


def xz9e(data: bytes) -> int:
    import lzma
    return len(lzma.compress(data, format=lzma.FORMAT_XZ,
                             preset=9 | lzma.PRESET_EXTREME))


def so_exports(path: Path) -> list[str]:
    r = subprocess.run(["nm", "-D", "--defined-only", str(path)],
                       capture_output=True, text=True, check=True)
    return sorted(line.split()[-1] for line in r.stdout.splitlines() if line.split())


def so_deps(path: Path) -> list[str]:
    r = subprocess.run(["ldd", str(path)], capture_output=True, text=True)
    deps = []
    for line in r.stdout.splitlines():
        name = line.strip().split(" ")[0]
        if name and not name.startswith("linux-vdso"):
            deps.append(name)
    return sorted(deps)


def measure_static(path: Path) -> dict:
    raw = path.read_bytes()
    with tempfile.TemporaryDirectory() as td:
        stripped = Path(td) / "stripped.a"
        subprocess.run(["cp", str(path), str(stripped)], check=True)
        subprocess.run(["strip", str(stripped)], check=True)
        return {
            "raw_bytes": len(raw),
            "stripped_bytes": stripped.stat().st_size,
            "xz9e_bytes": xz9e(stripped.read_bytes()),
            "sha256_raw": sha256(path),
        }


def measure_shared(path: Path) -> dict:
    raw = path.read_bytes()
    with tempfile.TemporaryDirectory() as td:
        stripped = Path(td) / "stripped.so"
        subprocess.run(["cp", str(path), str(stripped)], check=True)
        subprocess.run(["strip", str(stripped)], check=True)
        return {
            "raw_bytes": len(raw),
            "stripped_bytes": stripped.stat().st_size,
            "xz9e_bytes": xz9e(stripped.read_bytes()),
            "sha256_raw": sha256(path),
        }


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--stdout", action="store_true")
    args = ap.parse_args()

    if not STATIC.is_file():
        raise SystemExit("missing build/artifacts/libsongcore.a; "
                         "run `xmake build songcore_static`")
    if not SO.is_file():
        raise SystemExit("missing build/artifacts/shared/libsongcore.so; "
                         "run `xmake build songcore_shared`")

    exports = so_exports(SO)
    report = {
        "abi_version": 1,
        "static": measure_static(STATIC),
        "shared": {
            **measure_shared(SO),
            "exports": exports,
            "export_count": len(exports),
            "exact_contract": exports == CONTRACT,
            "dynamic_dependencies": so_deps(SO),
        },
    }

    text = json.dumps(report, indent=2, sort_keys=True) + "\n"
    if args.stdout:
        print(text)
        return 0
    OUT.write_text(text)
    print(f"wrote {OUT.relative_to(ROOT)}")
    print(f"static:  {report['static']['raw_bytes']} B raw / "
          f"{report['static']['stripped_bytes']} B stripped / "
          f"{report['static']['xz9e_bytes']} B xz")
    print(f"shared:  {report['shared']['raw_bytes']} B raw / "
          f"{report['shared']['stripped_bytes']} B stripped / "
          f"{report['shared']['xz9e_bytes']} B xz / "
          f"{report['shared']['export_count']} exports "
          f"(contract match: {report['shared']['exact_contract']})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
