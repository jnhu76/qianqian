#!/usr/bin/env python3
"""Verify docs/contracts/songcore-api.md stays in lockstep with the frozen ABI.

Extracts the SONGCORE_API declarations from include/songcore.h and requires
the API document to mention every one of them. When the shared artifact is
present, its dynamic export table must equal the header's symbol set —
header, shared library, and documentation must all agree.

    python3 tools/check_api_doc.py
"""
from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
HEADER = ROOT / "native" / "include" / "songcore.h"
DOC = ROOT / "docs" / "contracts" / "songcore-api.md"
SO = ROOT / "build" / "artifacts" / "shared" / "libsongcore.so"


def header_symbols() -> list[str]:
    text = HEADER.read_text()
    # One SONGCORE_API macro per public declaration line.
    names = []
    for m in re.finditer(r"SONGCORE_API\s+(?:void|uint32_t|song_status)\s+"
                         r"([a-z_0-9]+)\s*\(", text):
        names.append(m.group(1))
    if not names:
        raise SystemExit("no SONGCORE_API declarations found in the header")
    return sorted(set(names))


def main() -> int:
    symbols = header_symbols()
    doc = DOC.read_text()
    problems = []
    if len(symbols) != 15:
        problems.append(f"header declares {len(symbols)} symbols, ABI v1 "
                        f"freezes 15 — was the ABI changed without a v2 bump?")
    for sym in symbols:
        if sym not in doc:
            problems.append(f"docs/contracts/songcore-api.md does not document {sym}")

    if SO.is_file():
        r = subprocess.run(["nm", "-D", "--defined-only", str(SO)],
                           capture_output=True, text=True, check=True)
        exports = sorted({line.split()[-1] for line in r.stdout.splitlines()
                          if line.split()})
        if exports != symbols:
            problems.append(f"shared exports != header symbols: "
                            f"missing={sorted(set(symbols) - set(exports))} "
                            f"extra={sorted(set(exports) - set(symbols))}")
        print(f"shared artifact audit: {len(exports)} exports vs header")
    else:
        print("shared artifact not built; header<->doc check only")

    if problems:
        print("API doc check FAIL:")
        for p in problems:
            print(f"  - {p}")
        return 1
    print(f"API doc check PASS ({len(symbols)} symbols documented)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
