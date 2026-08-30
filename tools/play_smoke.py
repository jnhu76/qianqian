#!/usr/bin/env python3
"""Audible acceptance harness for the tiny SongCore path.

The Python package is intentionally test-only. Decoding stays in Qianqian's
native SongCore; sounddevice is merely a thin PortAudio sink for the Float32
PCM pipe emitted by qn_pcm_dump.

Multiple songs may be supplied. ``--seconds`` limits playback per song so the
real-song acceptance set can be sampled quickly without adding player state.
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


def play_one(sd, decoder: Path, song: Path, device, seconds: float | None) -> int:
    proc = subprocess.Popen(
        [str(decoder), str(song)],
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
            print(f"play_smoke: invalid PCM header for {song}\n{stderr}", file=sys.stderr)
            return 1

        sample_rate, channels, fmt = struct.unpack("<IHH", header[4:])
        if fmt != 1 or sample_rate <= 0 or channels <= 0:
            proc.terminate()
            proc.wait()
            print(f"play_smoke: unsupported PCM transport for {song}", file=sys.stderr)
            return 1

        limit_frames = None
        if seconds is not None:
            limit_frames = max(1, int(seconds * sample_rate))

        print(
            f"play_smoke: {song.name} -> Qianqian SongCore -> "
            f"{sample_rate} Hz / {channels} ch / Float32",
            file=sys.stderr,
        )

        frame_bytes = channels * 4
        played_frames = 0
        truncated_by_limit = False
        with sd.RawOutputStream(
            samplerate=sample_rate,
            channels=channels,
            dtype="float32",
            device=device,
        ) as stream:
            while True:
                block = proc.stdout.read(64 * 1024)
                if not block:
                    break
                if len(block) % frame_bytes:
                    block += read_exact(proc.stdout, frame_bytes - len(block) % frame_bytes)
                if len(block) % frame_bytes:
                    raise RuntimeError("truncated PCM frame")

                block_frames = len(block) // frame_bytes
                if limit_frames is not None:
                    remaining = limit_frames - played_frames
                    if remaining <= 0:
                        truncated_by_limit = True
                        break
                    if block_frames > remaining:
                        block = block[: remaining * frame_bytes]
                        block_frames = remaining
                        truncated_by_limit = True

                if block:
                    stream.write(block)
                    played_frames += block_frames

                if limit_frames is not None and played_frames >= limit_frames:
                    truncated_by_limit = True
                    break

        if truncated_by_limit:
            proc.terminate()
            proc.wait()
            proc.stderr.read()
            print(
                f"play_smoke: sampled {played_frames / sample_rate:.2f}s from {song.name}",
                file=sys.stderr,
            )
            return 0

        rc = proc.wait()
        stderr = proc.stderr.read().decode("utf-8", "replace")
        if stderr:
            print(stderr, end="", file=sys.stderr)
        if rc != 0:
            print(f"play_smoke: decoder exited {rc} for {song}", file=sys.stderr)
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
        print(f"play_smoke: {song}: {exc}", file=sys.stderr)
        return 1


def main() -> int:
    parser = argparse.ArgumentParser(description="Play local song(s) through Qianqian SongCore")
    parser.add_argument("songs", nargs="+", type=Path)
    parser.add_argument("--decoder", type=Path, default=default_decoder())
    parser.add_argument("--device", default=None, help="sounddevice output device id/name")
    parser.add_argument(
        "--seconds",
        type=float,
        default=None,
        help="play only the first N seconds of each song (default: full song)",
    )
    args = parser.parse_args()

    if args.seconds is not None and args.seconds <= 0:
        parser.error("--seconds must be > 0")
    for song in args.songs:
        if not song.is_file():
            parser.error(f"song not found: {song}")
    if not args.decoder.is_file():
        parser.error(f"decoder not found: {args.decoder} (build with `xmake build qn_pcm_dump`)")

    try:
        import sounddevice as sd
    except ImportError:
        print(
            "play_smoke: missing test-only dependency `sounddevice`; install with:\n"
            "  python -m pip install sounddevice",
            file=sys.stderr,
        )
        return 2

    for song in args.songs:
        rc = play_one(sd, args.decoder, song, args.device, args.seconds)
        if rc != 0:
            return rc
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
