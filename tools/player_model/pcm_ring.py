"""pcm_ring.py — bounded, frame-accurate PCM ring buffer (reference model).

Phase-1 oracle for the future native PlayerEngine queue. The accounting
unit is the FRAME everywhere: one frame = one sample for every channel.
Frames are stored as opaque integer tags, so loss / duplication /
reordering / stale frames are directly observable in tests. Byte sizes are
never accounting.

Single producer (decode worker) writes, single consumer (render callback)
reads, flush() drops everything unread (seek / stop / open). Ring-level
lifetime accounting always satisfies:

    produced_total == consumed_total + buffered + discarded_total

Frames that never enter the ring (stale-epoch decode results, killed
before publication) are tracked by the engine, not here.

Clarity over speed: modulo indexing, no bit tricks.
"""
from __future__ import annotations

from collections.abc import Iterable, Sequence


class PcmRing:
    """Bounded FIFO of frame tags. SPSC by convention; capacity is fixed."""

    def __init__(self, capacity_frames: int) -> None:
        if capacity_frames <= 0:
            raise ValueError("capacity_frames must be > 0")
        self.capacity = capacity_frames
        self._slots: list[int | None] = [None] * capacity_frames
        self._read = 0  # slot index of the oldest buffered frame
        self.buffered = 0
        self.produced_total = 0
        self.consumed_total = 0
        self.discarded_total = 0

    # -- geometry ----------------------------------------------------------

    @property
    def readable_frames(self) -> int:
        return self.buffered

    @property
    def writable_frames(self) -> int:
        return self.capacity - self.buffered

    # -- producer side ------------------------------------------------------

    def write(self, frames: Sequence[int]) -> int:
        """Store up to `writable_frames` tags in FIFO order.

        Never overwrites unread frames: the count actually stored is
        returned, and a well-behaved producer treats a short write as
        backpressure. Callers that need all-or-nothing check
        `writable_frames` first (the engine does).
        """
        n = min(len(frames), self.writable_frames)
        head = (self._read + self.buffered) % self.capacity
        for i in range(n):
            self._slots[(head + i) % self.capacity] = frames[i]
        self.buffered += n
        self.produced_total += n
        return n

    # -- consumer side ------------------------------------------------------

    def read(self, want: int) -> list[int]:
        """Consume up to `want` frames, oldest first. Never over-reads."""
        n = min(want, self.buffered)
        out = [self._slots[(self._read + i) % self.capacity] for i in range(n)]
        for i in range(n):
            self._slots[(self._read + i) % self.capacity] = None
        self._read = (self._read + n) % self.capacity
        self.buffered -= n
        self.consumed_total += n
        return out

    # -- flush --------------------------------------------------------------

    def flush(self) -> int:
        """Drop every unread frame (seek / stop / open). Returns the count."""
        dropped = self.buffered
        for i in range(dropped):
            self._slots[(self._read + i) % self.capacity] = None
        self._read = 0
        self.buffered = 0
        self.discarded_total += dropped
        return dropped

    # -- invariants ----------------------------------------------------------

    def check_invariants(self, deep: bool = True) -> None:
        assert 0 <= self.buffered <= self.capacity, (
            f"buffered {self.buffered} outside [0, {self.capacity}]")
        assert self.readable_frames + self.writable_frames == self.capacity
        assert self.produced_total == (
            self.consumed_total + self.buffered + self.discarded_total), (
            f"accounting: produced={self.produced_total} "
            f"consumed={self.consumed_total} buffered={self.buffered} "
            f"discarded={self.discarded_total}")
        if deep:
            occupied = sum(1 for s in self._slots if s is not None)
            assert occupied == self.buffered, (
                f"slot scan {occupied} != buffered {self.buffered}")
