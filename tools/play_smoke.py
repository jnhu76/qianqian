#!/usr/bin/env python3
"""Audible acceptance for SongCore — thin wrapper over the canonical FFI gate.

The decode-and-play machinery lives in tools/songcore_ffi_smoke.py (--play):
Python ctypes -> libsongcore.so/.dylib/songcore.dll -> the embedded FFmpeg
closure, with sounddevice as the only test-only sink. This wrapper just
forwards so there is ONE canonical audible path; the old qn_pcm_dump pipe
is retired.

    python3 tools/play_smoke.py song.flac [more.flac ...] [--seconds N]
"""
from __future__ import annotations

import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def main() -> int:
    cmd = [sys.executable, str(ROOT / "tools" / "songcore_ffi_smoke.py"),
           "--play", *sys.argv[1:]]
    return subprocess.run(cmd).returncode


if __name__ == "__main__":
    raise SystemExit(main())
