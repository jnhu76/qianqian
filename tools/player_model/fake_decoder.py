"""fake_decoder.py — deterministic SongCore stand-in for the PlayerEngine model.

Mirrors exactly the frozen contract the engine is allowed to use
(include/songcore.h): read_pcm(chunk) -> DecodedChunk | empty-EOF,
seek(us) -> (status, actual_landing_us) where status mirrors song_status
and actual_landing_us is the measured landing (-1 = genuinely unknown,
never manufactured), and typed decode failure with SongCore's
partial-success framing (frames before the fault are returned as SONG_OK;
the fault surfaces on the NEXT call). duration may be reported unknown
(duration_us == -1 in song_info); the decoder still knows its real
content length internally.

Frame identity = absolute source frame index, so any stale, duplicated, or
reordered frame is visible to the tests. Deterministic: no threads, no
time — the engine's producer steps drive it.
"""
from __future__ import annotations

from dataclasses import dataclass

SONG_OK = 0                 # include/songcore.h
SONG_ERR_DECODE_ERROR = 108
SONG_ERR_IO = 103
SONG_ERR_SEEK_UNSUPPORTED = 109
SONG_ERR_SEEK_ERROR = 110


class FakeOpenError(Exception):
    """Simulated song_open failure (bad file)."""


class DecodeError(Exception):
    def __init__(self, status: int) -> None:
        super().__init__(f"decode failure, status={status}")
        self.status = status


def us_to_frames(us: int, sample_rate: int) -> int:
    """Floor conversion — SongCore's internal target clamp."""
    return (us * sample_rate) // 1_000_000


def us_to_frames_nearest(us: int, sample_rate: int) -> int:
    """Nearest-frame conversion — the ENGINE's clock rebase. The ABI
    reports landings in microseconds; converting back must not bias the
    clock a frame early every seek."""
    return (us * sample_rate + 500_000) // 1_000_000


def frames_to_us(frames: int, sample_rate: int) -> int:
    return frames * 1_000_000 // sample_rate


@dataclass(frozen=True)
class FakeSongConfig:
    """Description of one fake song file."""

    total_frames: int
    sample_rate: int = 48_000
    channels: int = 2
    # Producer steps from decode-start to publish (in-flight latency).
    work_steps: int = 1
    # Actual landing = clamp(target) + offset. <= 0 models SongCore landing
    # at/before the requested position; the engine must rebase on the
    # RETURNED position, never on the request.
    seek_landing_offset: int = 0
    # Decoder raises DecodeError when asked to produce this source frame.
    fail_at_frame: int | None = None
    open_fails: bool = False
    # song_seek failure injection: every seek returns this status.
    seek_status: int = SONG_OK
    # SONG_OK but out_actual_position_us == -1: the landing is genuinely
    # unknown — the decoder still positions itself at the clamped target.
    seek_landing_unknown: bool = False
    # song_info reports duration_us == -1 (content length still known to
    # the fake internally; the ENGINE must not rely on it).
    unknown_duration: bool = False
    # reopen() raises FakeOpenError — stop()-recovery failure injection.
    reopen_fails: bool = False


@dataclass(frozen=True)
class DecodedChunk:
    """One read_pcm result: contiguous source frames, possibly a tail."""

    start_frame: int
    frames: tuple[int, ...]
    eof: bool  # True once this chunk reaches total_frames


class FakeSongCore:
    """Contract mirror of song_open/probe/read_pcm/seek/close. Not
    thread-safe (like the real handle): calls must be serialized."""

    def __init__(self, config: FakeSongConfig) -> None:
        if config.open_fails:
            raise FakeOpenError("simulated song_open failure")
        if config.total_frames <= 0:
            raise FakeOpenError("song must contain at least one frame")
        if config.sample_rate <= 0 or config.channels <= 0:
            raise FakeOpenError("bad stream parameters")
        self.cfg = config
        self.position = 0  # next source frame read_pcm will emit
        self.exhausted = False
        self.emitted_total = 0  # frames handed out (global accounting)
        self.seek_count = 0
        self.reopen_count = 0

    @property
    def duration_frames(self) -> int:
        """song_info duration semantics: -1 = unknown."""
        return -1 if self.cfg.unknown_duration else self.cfg.total_frames

    @property
    def remaining_frames(self) -> int:
        return self.cfg.total_frames - self.position

    def reopen(self) -> "FakeSongCore":
        """Drop-and-reopen recovery: a fresh handle at position 0. Mirrors
        song_close + song_open of the same source (in place, so lifetime
        diagnostics survive). Fails like a real reopen can (file gone)."""
        if self.cfg.reopen_fails:
            raise FakeOpenError("simulated reopen failure")
        self.reopen_count += 1
        self.position = 0
        self.exhausted = False
        return self

    def read_pcm(self, frame_capacity: int) -> DecodedChunk:
        if frame_capacity <= 0:
            raise ValueError("frame_capacity must be > 0")
        if self.exhausted:
            return DecodedChunk(self.position, (), True)  # SONG_EOF, 0 frames
        start = self.position
        n = min(frame_capacity, self.remaining_frames)
        if self.cfg.fail_at_frame is not None and start >= self.cfg.fail_at_frame:
            raise DecodeError(SONG_ERR_DECODE_ERROR)
        if self.cfg.fail_at_frame is not None:
            # partial-success: emit up to the fault, fail on the NEXT call
            n = min(n, self.cfg.fail_at_frame - start)
        frames = tuple(range(start, start + n))
        self.position = start + n
        self.exhausted = self.position >= self.cfg.total_frames
        self.emitted_total += n
        return DecodedChunk(start, frames, self.exhausted)

    def seek(self, position_us: int) -> tuple[int, int]:
        """Clamp target, apply landing offset, flush decoder state; returns
        (song_status, actual landing in us). Failure statuses mirror the
        frozen ABI (SEEK_UNSUPPORTED / SEEK_ERROR / ...). Success with
        actual == -1 means genuinely unknown; the decoder positions itself
        at the clamped target anyway."""
        self.seek_count += 1
        if self.cfg.seek_status != SONG_OK:
            return (self.cfg.seek_status, -1)
        rate = self.cfg.sample_rate
        target = max(us_to_frames(position_us, rate), 0)
        if not self.cfg.unknown_duration:
            target = min(target, self.cfg.total_frames)
        landing = max(target + self.cfg.seek_landing_offset, 0)
        if not self.cfg.unknown_duration:
            landing = min(landing, self.cfg.total_frames)
        self.position = landing
        self.exhausted = landing >= self.cfg.total_frames
        actual = -1 if self.cfg.seek_landing_unknown else frames_to_us(landing, rate)
        return (SONG_OK, actual)
