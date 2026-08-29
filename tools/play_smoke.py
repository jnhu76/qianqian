#!/usr/bin/env python3
"""Audible acceptance harness for the tiny SongCore path.

The Python package is intentionally test-only. Decoding stays in Qianqian's
native SongCore; sounddevice is merely a thin PortAudio sink for the Float32
PCM pipe emitted by qn_pcm_dump.
"""
from __future__ import annotations

import argparse
from pathlib import Path
import struct
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]


def read_exact(stream, size: int) -> bytes:
    chunks: list[bytes] = []
    left = size
    while left:
        block = stream.read(left)
        if not block:
            break
        chunks.append(block)
        left -= len(block)
    return b"".join(chunks)


def default_decoder() -> Path:
    suffix = ".exe" if sys.platform == "win32" else ""
    return ROOT / "build" / "artifacts" / f"qn_pcm_dump{suffix}"


def main() -> int:
    parser = argparse.ArgumentParser(description="Play one local song through Qianqian SongCore")
    parser.add_argument("song", type=Path)
    parser.add_argument("--decoder", type=Path, default=default_decoder())
    parser.add_argument("--device", default=None, help="sounddevice output device id/name")
    args = parser.parse_args()

    if not args.song.is_file():
        parser.error(f"song not found: {args.song}")
    if not args.decoder.is_file():
        parser.error(f"decoder not found: {args.decoder} (build with `xmake qn_pcm_dump`)")

    try:
        import sounddevice as sd
    except ImportError:
        print(
            "play_smoke: missing test-only dependency `sounddevice`; install with:\n"
            "  python -m pip install sounddevice",
            file=sys.stderr,
        )
        return 2

    proc = subprocess.Popen(
        [str(args.decoder), str(args.song)],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        bufsize=0,
    )
    assert proc.stdout is not None
    assert proc.stderr is not None

    try:
        header = read_exact(proc.stdout, 12)
        if len(header) != 12 or header[:4] != b"QPCM":
            stderr = proc.stderr.read().decode("utf-8", "replace")
            proc.wait()
            print(f"play_smoke: invalid PCM header\n{stderr}", file=sys.stderr)
            return 1
        sample_rate, channels, fmt = struct.unpack("<IHH", header[4:])
        if fmt != 1 or sample_rate <= 0 or channels <= 0:
            proc.terminate()
            print("play_smoke: unsupported PCM transport", file=sys.stderr)
            return 1

        print(
            f"play_smoke: Qianqian SongCore -> {sample_rate} Hz / {channels} ch / Float32",
            file=sys.stderr,
        )
        with sd.RawOutputStream(
            samplerate=sample_rate,
            channels=channels,
            dtype="float32",
            device=args.device,
        ) as stream:
            while True:
                block = proc.stdout.read(64 * 1024)
                if not block:
                    break
                frame_bytes = channels * 4
                if len(block) % frame_bytes:
                    # A pipe read may split a frame. Complete it before handing
                    # bytes to PortAudio; no sample transformation is performed.
                    block += read_exact(proc.stdout, frame_bytes - len(block) % frame_bytes)
                if len(block) % frame_bytes:
                    raise RuntimeError("truncated PCM frame")
                stream.write(block)

        rc = proc.wait()
        stderr = proc.stderr.read().decode("utf-8", "replace")
        if stderr:
            print(stderr, end="", file=sys.stderr)
        if rc != 0:
            print(f"play_smoke: decoder exited {rc}", file=sys.stderr)
            return rc
        return 0
    except KeyboardInterrupt:
        proc.terminate()
        proc.wait()
        return 130
    except Exception as exc:
        proc.terminate()
        proc.wait()
        stderr = proc.stderr.read().decode("utf-8", "replace")
        if stderr:
            print(stderr, end="", file=sys.stderr)
        print(f"play_smoke: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
