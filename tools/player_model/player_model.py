"""player_model.py — PlayerEngine reference model (Phase-1 semantic oracle).

Executable definition of the semantics frozen in docs/player-engine.md:
state machine, bounded queue lifecycle, epoch-guarded seek, EOF drain, the
device/media timeline split, the submitted-vs-rendered output pipeline,
the seek-failure policy, and landing-quality tracking. Deterministic and
single-threaded; the decode worker, the backend submit callback, and the
hardware render progression are driven step-by-step by the scenario
(producer_step / submit / backend_render) — the same interleaving freedom
the native threads will have later.

Everything counts FRAMES (1 frame = one sample for every channel), and
every counter names its domain (docs/player-engine.md §Frame-domain
naming): decoded_source_*, queued_media_*, submitted_output_*,
rendered_output_*; *_media_* counts media-domain payload. Submitting PCM
to the backend does not make it audible: only backend render progression
(proven by the device clock) advances the media position.

This file is the authority the native implementation must match
behaviorally; docs/player-engine.md is the human-readable contract.
"""
from __future__ import annotations

from dataclasses import dataclass
from enum import Enum, auto

from fake_decoder import (DecodeError, DecodedChunk, FakeOpenError,
                          FakeSongConfig, FakeSongCore, us_to_frames,
                          us_to_frames_nearest)
from pcm_ring import PcmRing


class State(Enum):
    EMPTY = auto()
    READY = auto()
    PLAYING = auto()
    PAUSED = auto()
    ENDED = auto()
    ERROR = auto()


class LandingQuality(Enum):
    """Quality of the media clock base (docs §Seek landing quality):
    CONFIRMED = SongCore reported an actual landing; ESTIMATED = SONG_OK
    with a genuinely unknown landing (-1), the base is the requested
    clamped target. A FAILED seek is neither — it is an ERROR."""
    CONFIRMED = auto()
    ESTIMATED = auto()


class EngineError(Exception):
    """Illegal call for the current state. The native ABI maps this to a
    typed error code; the model raises so tests cannot miss it."""


class SongSeekError(Exception):
    """SongCore seek failed (typed status). Frozen fail-closed policy: the
    generation was already invalidated, so the engine lands in ERROR and
    must never silently resume the old timeline."""

    def __init__(self, status: int) -> None:
        super().__init__(f"seek failure, status={status}")
        self.status = status


class AudioEngine:
    """AudioEngine boundary stub. Phase 1 is BYPASS only; reset()/drain()
    are part of the seek/EOF lifecycle the engine must call. The frozen SRC
    rule (aresample only when source != required format) lives downstream
    of this boundary and is not modeled here."""

    def process(self, frames: list[int]) -> list[int]:
        return frames

    def reset(self) -> None:
        pass

    def drain(self) -> None:
        pass


@dataclass
class OutputSpan:
    """One submitted output interval and its device->media mapping.

    The reference representation of the frozen rule "a correct
    device->media mapping must exist" (docs §Timeline mapping). MEDIA
    spans carry real media frames (their tags are the absolute source
    frame identities — ORACLE-ONLY truth: a production engine tracks
    durations/anchors, never frame identities); GAP spans carry
    synthetic silence/padding with device duration and ZERO media
    duration. Output coordinates are per-generation (each commit
    restarts the output timeline at 0). The span representation itself
    is NOT frozen ABI — only the mapping semantics are.
    """

    generation: int
    output_begin: int
    output_end: int
    kind: str                 # "media" | "gap"
    tags: tuple[int, ...]     # media frame identities; () for gap

    @property
    def length(self) -> int:
        return self.output_end - self.output_begin


@dataclass
class SubmitResult:
    """Result of one backend callback (engine.submit): what was moved from
    the PCM queue into the backend buffer, plus any GAP padding. This PCM
    is SUBMITTED, not yet audible."""
    segment: int
    tags: tuple[int, ...]    # real frame identities submitted
    silence_frames: int      # GAP frames submitted (underrun/preroll/EOS)
    kind: str = "audio"      # "audio" | "underrun" | "preroll" | "eos" | "idle"


@dataclass
class RenderReport:
    """Result of one hardware render progression (engine.backend_render):
    proof that the device actually consumed output frames."""
    segment: int
    kind: str                # "rendered" | "paused" | "stale"
    rendered_output_frames: int
    rendered_media_frames: int
    tags: tuple[int, ...]    # media identities proven rendered
    generation: int = 0


@dataclass
class StepReport:
    outcome: str   # IDLE | BEGIN | WORKING | WROTE | BACKPRESSURE | STALE
    frames: int = 0
    epoch: int = 0
    start_frame: int = 0


@dataclass
class _InFlight:
    """One decode result already produced, not yet published. Carries the
    epoch it was decoded under — the entire stale-frame defense."""

    epoch: int
    chunk: DecodedChunk
    steps_left: int


@dataclass
class EngineConfig:
    capacity_frames: int          # PCM queue capacity (the ONLY queue sizing)
    device_rate: int = 48_000     # Phase-1 model: device runs at source rate
    channels: int = 2
    read_chunk_frames: int = 1024 # song_read_pcm capacity per decode chunk


@dataclass
class EngineSnapshot:
    state: State
    media_position_frames: int    # the audible media timeline (never decode
                                  # progress, never submitted/device frames)
    position_quality: LandingQuality
    decoded_source_position: int  # decoder head on the media timeline
    duration_frames: int          # -1 = unknown (never a fake 0 clamp)
    duration_known: bool
    queued_media_frames: int
    capacity_frames: int
    epoch: int
    segment: int
    source_eof: bool
    underrun_count: int
    underrun_silence_output_frames: int
    preroll_events: int
    preroll_silence_output_frames: int
    eos_silence_output_frames: int
    decoded_source_frames: int
    submitted_output_frames: int
    rendered_output_frames: int
    pending_output_frames: int
    discarded_output_frames: int
    submitted_media_frames: int
    rendered_media_frames: int    # lifetime
    rendered_gap_output_frames: int  # lifetime silence proven rendered
    pending_media_frames: int
    discarded_output_media_frames: int
    discarded_stale_media_frames: int
    stale_render_events: int
    last_error: str | None


class PlayerEngine:
    """Control-plane + worker + backend model. One engine owns one song at
    a time.

    Thread ownership in the native implementation (modeled here by call
    discipline): open/play/pause/seek/stop arrive on the control thread;
    producer_step runs on the decode worker; submit runs on the audio
    callback; backend_render is the device/render clock reading. SongCore
    calls happen only inside producer_step/seek/stop paths and must stay
    serialized with respect to each other (the SongCore handle is not
    thread-safe).
    """

    def __init__(self, config: EngineConfig) -> None:
        if config.read_chunk_frames <= 0:
            raise ValueError("read_chunk_frames must be > 0")
        if config.read_chunk_frames > config.capacity_frames:
            raise ValueError("read_chunk_frames must fit the queue capacity")
        self.cfg = config
        self.ring = PcmRing(config.capacity_frames)
        self.audio_engine = AudioEngine()
        self.state = State.EMPTY
        self.decoder: FakeSongCore | None = None
        self.song_cfg: FakeSongConfig | None = None
        self.epoch = 0
        # Segment = one committed playback span (open / seek / stop /
        # restart each start one). Rendered frames must be exactly
        # contiguous from segment_start[segment] — the anti-stale assertion.
        self.segment = 0
        self.segment_start: dict[int, int] = {0: 0}
        self.segment_quality: dict[int, LandingQuality] = {0: LandingQuality.CONFIRMED}
        self.source_eof = False
        self.in_flight: _InFlight | None = None
        # Media clock (position authority): base at last commit plus the
        # media frames PROVEN rendered since. Frozen at commit boundaries;
        # survives _invalidate so ERROR diagnostics keep the last audible
        # position instead of fabricating a landing.
        self.base_frame = 0
        self.rendered_media_frames = 0   # per-segment (media domain)
        self.submitted_media_this_segment = False
        # Backend output model (per generation; generation == epoch — one
        # token guards both decode publication and output accounting,
        # docs §Stale output generations).
        self.spans: list[OutputSpan] = []
        self._span_idx = 0        # first span not fully rendered
        self._span_offset = 0     # rendered frames inside spans[_span_idx]
        self.output_endpoint = 0  # submitted output frames this generation
        self.rendered_output = 0  # rendered output frames this generation
        self.span_media_total = 0         # media submitted this generation
        self.span_media_rendered = 0      # media rendered this generation
        # Lifetime accounting (conservation laws, docs §Accounting):
        #   submitted_output == pending_output + rendered_output + discarded_output
        #   submitted_media  == pending_media  + rendered_media  + discarded_output_media
        #   rendered_output  == rendered_media + rendered_gap  (output domain
        #   splits exactly into proven-rendered media and GAP silence)
        self.submitted_output_total = 0
        self.rendered_output_total = 0
        self.discarded_output_total = 0
        self.submitted_media_total = 0
        self.rendered_media_total = 0
        self.rendered_gap_output_frames = 0
        self.discarded_output_media_total = 0
        self.stale_render_events = 0
        # Diagnostics (docs/player-engine.md).
        self.underrun_count = 0
        self.underrun_silence_output_frames = 0
        self.preroll_events = 0
        self.preroll_silence_output_frames = 0
        self.eos_silence_output_frames = 0
        self.discarded_stale_media_frames = 0
        self.decoded_source_frames = 0
        self.last_error: str | None = None

    # ------------------------------------------------------------------
    # diagnostics

    @property
    def duration_known(self) -> bool:
        return self.decoder is not None and self.decoder.duration_frames >= 0

    @property
    def duration_frames(self) -> int:
        return self.decoder.duration_frames if self.decoder else 0

    @property
    def decoded_source_position(self) -> int:
        """Decode position: the decoder head on the media timeline. May run
        ahead of the audible media position (normal pre-decode); must never
        drive player_get_position()."""
        return self.decoder.position if self.decoder else 0

    @property
    def position_quality(self) -> LandingQuality:
        return self.segment_quality[self.segment]

    @property
    def position_frames(self) -> int:
        """player_get_position(): the best estimate of where the content
        currently played by the output device sits on the media timeline —
        never decode progress, never a submitted endpoint, never device
        frames. Known duration clamps and ENDED snaps to duration; unknown
        duration never gets a fake clamp."""
        if self.state == State.ENDED and self.duration_known:
            return self.duration_frames
        pos = self.base_frame + self.rendered_media_frames
        if self.duration_known:
            pos = min(pos, self.duration_frames)
        return pos

    def pending_output_frames(self) -> int:
        return self.output_endpoint - self.rendered_output

    def pending_media_frames(self) -> int:
        return self.span_media_total - self.span_media_rendered

    def media_position_at_output(self, output_pos: int) -> int:
        """The device->media mapping: walk this generation's spans and map
        an output-domain position onto the media timeline. GAP spans
        occupy device time but contribute zero media duration; positions
        beyond the rendered endpoint clamp to the last mapped media
        endpoint."""
        media = 0
        for span in self.spans:
            if output_pos <= span.output_begin:
                break
            if span.kind == "media":
                media += min(output_pos, span.output_end) - span.output_begin
        return self.base_frame + media

    def snapshot(self) -> EngineSnapshot:
        return EngineSnapshot(
            state=self.state,
            media_position_frames=self.position_frames,
            position_quality=self.position_quality,
            decoded_source_position=self.decoded_source_position,
            duration_frames=self.duration_frames,
            duration_known=self.duration_known,
            queued_media_frames=self.ring.readable_frames,
            capacity_frames=self.ring.capacity,
            epoch=self.epoch,
            segment=self.segment,
            source_eof=self.source_eof,
            underrun_count=self.underrun_count,
            underrun_silence_output_frames=self.underrun_silence_output_frames,
            preroll_events=self.preroll_events,
            preroll_silence_output_frames=self.preroll_silence_output_frames,
            eos_silence_output_frames=self.eos_silence_output_frames,
            decoded_source_frames=self.decoded_source_frames,
            submitted_output_frames=self.submitted_output_total,
            rendered_output_frames=self.rendered_output_total,
            pending_output_frames=self.pending_output_frames(),
            discarded_output_frames=self.discarded_output_total,
            submitted_media_frames=self.submitted_media_total,
            rendered_media_frames=self.rendered_media_total,
            rendered_gap_output_frames=self.rendered_gap_output_frames,
            pending_media_frames=self.pending_media_frames(),
            discarded_output_media_frames=self.discarded_output_media_total,
            discarded_stale_media_frames=self.discarded_stale_media_frames,
            stale_render_events=self.stale_render_events,
            last_error=self.last_error,
        )

    # ------------------------------------------------------------------
    # commit machinery

    def _invalidate(self) -> None:
        """Kill the current generation: epoch bump (invalidates producer
        output FIRST), discard all pending output (submitted-but-not-
        rendered spans can never advance the next segment), flush the PCM
        queue, quiesce DSP. The media clock is intentionally NOT touched
        here: position stays at the last audible endpoint until a new
        landing commits (or freezes forever in ERROR)."""
        self.epoch += 1
        self.discarded_output_total += self.output_endpoint - self.rendered_output
        self.discarded_output_media_total += (self.span_media_total
                                              - self.span_media_rendered)
        self.spans = []
        self._span_idx = 0
        self._span_offset = 0
        self.output_endpoint = 0
        self.rendered_output = 0
        self.span_media_total = 0
        self.span_media_rendered = 0
        self.ring.flush()
        self.source_eof = False
        self.audio_engine.reset()

    def _commit_landing(self, landing_frames: int,
                        quality: LandingQuality) -> None:
        """Open a new segment rebased at a media landing."""
        self.base_frame = landing_frames
        self.rendered_media_frames = 0
        self.submitted_media_this_segment = False
        self.segment += 1
        self.segment_start[self.segment] = landing_frames
        self.segment_quality[self.segment] = quality

    def _landing(self, status: int, actual_us: int,
                 requested_us: int) -> tuple[int, LandingQuality] | None:
        """SongCore seek result -> (landing frame, quality). None = the
        seek failed (fail-closed; caller goes to ERROR). Success with an
        unknown landing (-1) is ESTIMATED on the requested clamped target —
        never described as an actual landing."""
        if status != 0:
            return None
        if actual_us >= 0:
            return (us_to_frames_nearest(actual_us, self.cfg.device_rate),
                    LandingQuality.CONFIRMED)
        target = max(us_to_frames(requested_us, self.cfg.device_rate), 0)
        if self.duration_known:
            target = min(target, self.duration_frames)
        return (target, LandingQuality.ESTIMATED)

    # ------------------------------------------------------------------
    # lifecycle: open / play / pause / stop / seek

    def open(self, song_cfg: FakeSongConfig) -> None:
        """open(song): stop everything, drop the previous song, start the
        new one at position 0 (a fresh handle — a CONFIRMED position),
        state READY. Never autoplays."""
        self._invalidate()  # kills old in-flight chunks AND old pending output
        try:
            decoder = FakeSongCore(song_cfg)
        except FakeOpenError as exc:
            self.state = State.EMPTY
            self.decoder = None
            self.song_cfg = None
            self.last_error = str(exc)
            raise EngineError(f"open failed: {exc}") from exc
        self.decoder = decoder
        self.song_cfg = song_cfg
        self._commit_landing(0, LandingQuality.CONFIRMED)
        self.state = State.READY
        self.last_error = None

    def play(self) -> None:
        if self.state == State.PLAYING:
            return  # idempotent
        if self.state in (State.READY, State.PAUSED):
            self.state = State.PLAYING
            return
        if self.state == State.ENDED:
            # Frozen restart policy: play after ENDED replays from 0.
            assert self.decoder is not None
            self._invalidate()
            status, actual_us = self.decoder.seek(0)
            landing = self._landing(status, actual_us, 0)
            if landing is None:
                self.state = State.ERROR
                self.last_error = f"restart seek failure, status={status}"
                raise SongSeekError(status)
            self._commit_landing(*landing)
            self.state = State.PLAYING
            return
        raise EngineError(f"play from {self.state.name} is illegal")

    def pause(self) -> None:
        if self.state == State.PLAYING:
            self.state = State.PAUSED
        # PAUSED / READY / ENDED: documented no-op. Buffer is retained.

    def stop(self) -> None:
        """stop != pause: deterministic rebuild to READY @0.

        Recovery decision (docs §ERROR recovery): from a healthy state the
        handle is trusted — rewind in place via seek(0). From ERROR (or
        when the rewind seek fails) the handle position is not trusted —
        drop and reopen the source. If even the reopen fails, ERROR
        persists and only open() recovers."""
        if self.state == State.EMPTY:
            return
        assert self.decoder is not None
        self._invalidate()
        landing: tuple[int, LandingQuality] | None = None
        if self.state is not State.ERROR:
            landing = self._landing(*self.decoder.seek(0), 0)
        if landing is None:
            try:
                self.decoder = self.decoder.reopen()
                landing = (0, LandingQuality.CONFIRMED)
            except FakeOpenError as exc:
                self.state = State.ERROR
                self.last_error = f"stop recovery failed: {exc}"
                return
        self._commit_landing(*landing)
        self.state = State.READY
        self.last_error = None

    def seek(self, position_us: int) -> int:
        """seek(T) from READY/PLAYING/PAUSED (and ENDED -> READY). Returns
        the actual landing frame (CONFIRMED) or the clamped requested
        target (ESTIMATED). Fail-closed commit order: invalidate the
        generation and flush FIRST, then attempt SongCore — a failed seek
        must never silently resume the old timeline."""
        if self.state not in (State.READY, State.PLAYING, State.PAUSED,
                              State.ENDED):
            raise EngineError(f"seek from {self.state.name} is illegal")
        assert self.decoder is not None
        was_ended = self.state == State.ENDED
        self._invalidate()
        status, actual_us = self.decoder.seek(position_us)
        landing = self._landing(status, actual_us, position_us)
        if landing is None:
            self.state = State.ERROR
            self.last_error = f"seek failure, status={status}"
            raise SongSeekError(status)
        self._commit_landing(*landing)
        if was_ended:
            self.state = State.READY
        return landing[0]

    # ------------------------------------------------------------------
    # decode worker side

    def producer_step(self) -> StepReport:
        """One decode-worker quantum. New work starts only while PLAYING;
        an in-flight chunk always completes (that is what makes the epoch
        guard exercisable), then publishes or dies by epoch."""
        if self.in_flight is not None:
            self.in_flight.steps_left -= 1
            if self.in_flight.steps_left > 0:
                return StepReport("WORKING", epoch=self.in_flight.epoch)
            flight, self.in_flight = self.in_flight, None
            if flight.epoch != self.epoch:
                self.discarded_stale_media_frames += len(flight.chunk.frames)
                return StepReport("STALE", frames=len(flight.chunk.frames),
                                  epoch=flight.epoch,
                                  start_frame=flight.chunk.start_frame)
            frames = self.audio_engine.process(list(flight.chunk.frames))
            assert self.ring.writable_frames >= len(frames), (
                "engine began a chunk that does not fit — producer bug")
            self.ring.write(frames)
            if flight.chunk.eof:
                self.source_eof = True
                self._maybe_end()
            return StepReport("WROTE", frames=len(frames), epoch=flight.epoch,
                              start_frame=flight.chunk.start_frame)

        if self.state in (State.EMPTY, State.ERROR):
            return StepReport("IDLE")
        if self.state != State.PLAYING:
            return StepReport("IDLE")  # PAUSED: no new decode, buffer retained
        assert self.decoder is not None
        if self.source_eof:
            return StepReport("IDLE")
        want = min(self.cfg.read_chunk_frames, self.decoder.remaining_frames)
        if self.ring.writable_frames < want:
            return StepReport("BACKPRESSURE")  # §10: yield, never overwrite
        try:
            chunk = self.decoder.read_pcm(want)
        except DecodeError as exc:
            self.state = State.ERROR
            self.last_error = f"decode failure, status={exc.status}"
            return StepReport("IDLE")
        self.decoded_source_frames += len(chunk.frames)
        self.in_flight = _InFlight(epoch=self.epoch, chunk=chunk,
                                   steps_left=self.song_cfg.work_steps)
        return StepReport("BEGIN", frames=len(chunk.frames), epoch=self.epoch,
                          start_frame=chunk.start_frame)

    # ------------------------------------------------------------------
    # backend submit (audio callback; must stay trivial)

    def _append_span(self, kind: str, length: int,
                     tags: tuple[int, ...]) -> None:
        self.spans.append(OutputSpan(
            generation=self.epoch,
            output_begin=self.output_endpoint,
            output_end=self.output_endpoint + length,
            kind=kind, tags=tags))
        self.output_endpoint += length
        self.span_media_total += len(tags)
        self.submitted_output_total += length
        self.submitted_media_total += len(tags)

    def submit(self, period_frames: int) -> SubmitResult:
        """One backend callback: move up to `period_frames` of real PCM
        from the queue into the backend as output spans, padding any
        shortfall with GAP silence per the frozen classification
        (preroll / underrun / EOS). Never blocks, never decodes.

        Submitting does NOT make PCM audible and does NOT advance the
        media position — only proven render progression does
        (backend_render; docs §Submitted != rendered)."""
        if self.state != State.PLAYING:
            return SubmitResult(self.segment, (), 0, "idle")
        tags = self.ring.read(min(self.ring.readable_frames, period_frames))
        m = len(tags)
        shortfall = period_frames - m
        drained = (self.source_eof and self.in_flight is None
                   and self.ring.readable_frames == 0)
        kind = "audio"
        gap_kind: str | None = None
        if drained:
            if shortfall > 0:
                gap_kind = kind = "eos"
        elif shortfall > 0:
            if not self.submitted_media_this_segment:
                # Startup preroll: the device runs before the first frames
                # arrive. Not an underrun (docs §Underrun).
                gap_kind = kind = "preroll"
            else:
                gap_kind = kind = "underrun"
        if m:
            self._append_span("media", m, tuple(tags))
            self.submitted_media_this_segment = True
        if gap_kind == "preroll":
            self.preroll_events += 1
            self.preroll_silence_output_frames += shortfall
            self._append_span("gap", shortfall, ())
        elif gap_kind == "underrun":
            self.underrun_count += 1
            self.underrun_silence_output_frames += shortfall
            self._append_span("gap", shortfall, ())
        elif gap_kind == "eos":
            self.eos_silence_output_frames += shortfall
            self._append_span("gap", shortfall, ())
        self._maybe_end()
        return SubmitResult(self.segment, tuple(tags), max(shortfall, 0), kind)

    # ------------------------------------------------------------------
    # backend render progression (device clock evidence)

    def backend_render(self, frames: int,
                       generation: int | None = None) -> RenderReport:
        """The device has consumed `frames` output frames (future
        authority: WASAPI IAudioClock / backend hardware clock reading).
        Generation-guarded: a late event from a dead generation is
        discarded — old output accounting can never advance the current
        media timeline. Only spans proven rendered here move the media
        position; PAUSE freezes render advancement (pending output stays
        pending)."""
        gen = self.epoch if generation is None else generation
        if gen != self.epoch:
            self.stale_render_events += 1
            return RenderReport(self.segment, "stale", 0, 0, (), gen)
        if self.state == State.PAUSED:
            return RenderReport(self.segment, "paused", 0, 0, (), gen)
        advance = min(frames, self.output_endpoint - self.rendered_output)
        remaining = advance
        media_adv = 0
        gap_adv = 0
        rendered_tags: list[int] = []
        while remaining > 0 and self._span_idx < len(self.spans):
            span = self.spans[self._span_idx]
            avail = span.length - self._span_offset
            take = min(avail, remaining)
            if span.kind == "media":
                media_adv += take
                rendered_tags.extend(
                    span.tags[self._span_offset:self._span_offset + take])
            else:
                gap_adv += take
            self._span_offset += take
            remaining -= take
            if self._span_offset == span.length:
                self._span_idx += 1
                self._span_offset = 0
        self.rendered_output += advance
        self.rendered_output_total += advance
        self.rendered_media_frames += media_adv
        self.rendered_media_total += media_adv
        self.rendered_gap_output_frames += gap_adv
        self.span_media_rendered += media_adv
        self._maybe_end()
        return RenderReport(self.segment, "rendered", advance, media_adv,
                            tuple(rendered_tags), gen)

    def _maybe_end(self) -> None:
        """ENDED requires the complete audible drain: source exhausted,
        queue empty, nothing in flight, and no pending MEDIA output.
        Trailing backend padding (EOS GAP) must not postpone it."""
        if (self.state == State.PLAYING and self.source_eof
                and self.in_flight is None
                and self.ring.readable_frames == 0
                and self.pending_media_frames() == 0):
            self.state = State.ENDED
            self.audio_engine.drain()
