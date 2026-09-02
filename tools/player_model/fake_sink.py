"""fake_sink.py — manual-tick audio device + exact-consumption collector.

Stands in for the future AudioBackend, split the way the frozen semantics
demand (docs/player-engine.md §Submitted != rendered):

  tick_submit()  runs the backend callback — engine.submit(): PCM queue ->
                 backend buffer, GAP padding classified as
                 preroll/underrun/EOS;
  tick_render()  advances hardware playout — engine.backend_render(): the
                 device/render clock proving what actually became audible.

tick() runs both in lockstep (the deterministic zero-latency device);
interleaving the two primitives is how scenarios prove that submitted
output is not rendered output.

Per-tick accounting (docs §Output accounting): every active callback
records requested_output_frames = media_frames_consumed +
silence_frames_injected = total_output_frames; the device/render counter
advances by total_output_frames while media progression is derived only
from the media part. Every real frame tag is collected per engine
segment — both submitted and rendered — so scenarios can assert exact
consumption on the audible stream (no gaps, no duplicates, no stale
frames). Realtime accuracy is explicitly not a goal.
"""
from __future__ import annotations

from dataclasses import dataclass

from player_model import PlayerEngine, RenderReport, SubmitResult


@dataclass
class TickAccounting:
    """One active backend callback, accounted across both domains."""
    requested_output_frames: int
    media_frames_consumed: int    # real frames moved queue -> backend
    silence_frames_injected: int  # GAP frames (preroll / underrun / EOS)
    total_output_frames: int      # == media + silence

    def check(self) -> None:
        assert self.total_output_frames == (
            self.media_frames_consumed + self.silence_frames_injected), (
            f"tick accounting: {self.total_output_frames} != "
            f"{self.media_frames_consumed} + {self.silence_frames_injected}")
        assert self.total_output_frames == self.requested_output_frames, (
            f"tick accounting: submitted {self.total_output_frames} != "
            f"requested {self.requested_output_frames}")


class FakeSink:
    def __init__(self, engine: PlayerEngine, period_frames: int) -> None:
        if period_frames <= 0:
            raise ValueError("period_frames must be > 0")
        self.engine = engine
        self.period_frames = period_frames
        self.ticks = 0
        self.segments: dict[int, list[int]] = {}          # rendered (audible)
        self.submitted_segments: dict[int, list[int]] = {}  # moved to backend
        self.total_real_frames = 0        # rendered real frames
        self.total_submitted_frames = 0   # submitted real frames
        self.total_silence_frames = 0     # submitted GAP frames
        self.last_result: SubmitResult | None = None
        self.last_render: RenderReport | None = None
        self.last_tick: TickAccounting | None = None
        self.total_requested_output_frames = 0
        self.total_media_frames_consumed = 0
        self.total_silence_frames_injected = 0
        self.total_output_frames = 0

    def _submit_once(self) -> None:
        result = self.engine.submit(self.period_frames)
        self.last_result = result
        self.total_silence_frames += result.silence_frames
        if result.tags:
            self.submitted_segments.setdefault(result.segment, []).extend(
                result.tags)
            self.total_submitted_frames += len(result.tags)
        if result.kind != "idle":
            # active callback: the requested period was filled with exactly
            # media frames + injected silence
            record = TickAccounting(
                requested_output_frames=self.period_frames,
                media_frames_consumed=len(result.tags),
                silence_frames_injected=result.silence_frames,
                total_output_frames=len(result.tags) + result.silence_frames)
            record.check()
            self.last_tick = record
            self.total_requested_output_frames += record.requested_output_frames
            self.total_media_frames_consumed += record.media_frames_consumed
            self.total_silence_frames_injected += record.silence_frames_injected
            self.total_output_frames += record.total_output_frames

    def check_accounting(self) -> None:
        """Cumulative per-tick invariant: everything the device requested
        and consumed was media or injected silence — nothing else exists."""
        assert self.total_output_frames == (
            self.total_media_frames_consumed
            + self.total_silence_frames_injected), (
            f"sink accounting: {self.total_output_frames} != "
            f"{self.total_media_frames_consumed} media + "
            f"{self.total_silence_frames_injected} silence")
        assert self.total_output_frames == self.total_requested_output_frames

    def tick_submit(self, periods: int = 1) -> None:
        """Backend callbacks only — no device consumption."""
        for _ in range(periods):
            self.ticks += 1
            self._submit_once()

    def tick_render(self, frames: int) -> None:
        """Device/render progression only — no callback refill."""
        report = self.engine.backend_render(frames)
        self.last_render = report
        if report.tags:
            self.segments.setdefault(report.segment, []).extend(report.tags)
            self.total_real_frames += len(report.tags)

    def tick(self, periods: int = 1) -> None:
        """Lockstep period: callback refills the period, then the device
        renders it (zero-latency fake device)."""
        for _ in range(periods):
            self.ticks += 1
            self._submit_once()
            self.tick_render(self.period_frames)
