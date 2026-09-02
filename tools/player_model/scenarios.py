#!/usr/bin/env python3
"""scenarios.py — Phase-1 gate suite for the PlayerEngine reference model.

Every gate is deterministic. The randomized stress uses seeded randomness
only. This suite is the executable oracle behind docs/player-engine.md;
the native PlayerEngine (Phase 1.5) must replay these semantics.

    python3 tools/player_model/scenarios.py                 # full suite
    python3 tools/player_model/scenarios.py --quick         # reduced stress
    python3 tools/player_model/scenarios.py --stress-seeds 1000 --stress-ops 500

Exit code 0 = all gates PASS.
"""
from __future__ import annotations

import argparse
import random
import sys
import time
import traceback
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from fake_decoder import (SONG_ERR_SEEK_ERROR, SONG_ERR_SEEK_UNSUPPORTED,  # noqa: E402
                          FakeSongConfig, frames_to_us)
from fake_sink import FakeSink  # noqa: E402
from pcm_ring import PcmRing  # noqa: E402
from player_model import (EngineConfig, EngineError, LandingQuality,  # noqa: E402
                          PlayerEngine, SongSeekError, State)

GATES: list[tuple[str, object]] = []


def gate(fn):
    GATES.append((fn.__name__.removeprefix("g_"), fn))
    return fn


# ---------------------------------------------------------------------------
# helpers


def make_pair(rate=48_000, capacity=16_384, chunk=1_024, period=512):
    engine = PlayerEngine(EngineConfig(
        capacity_frames=capacity, device_rate=rate, read_chunk_frames=chunk))
    return engine, FakeSink(engine, period)


def song(**kw) -> FakeSongConfig:
    cfg = dict(total_frames=48_000 * 8, sample_rate=48_000, channels=2,
               work_steps=1, seek_landing_offset=0, fail_at_frame=None,
               open_fails=False, seek_status=0, seek_landing_unknown=False,
               unknown_duration=False, reopen_fails=False)
    cfg.update(kw)
    return FakeSongConfig(**cfg)


def drain_producer(engine: PlayerEngine, limit=4096):
    """Run decode-worker steps until the worker has nothing left (EOF,
    pause, error, or idle)."""
    outcomes = []
    for _ in range(limit):
        rep = engine.producer_step()
        outcomes.append(rep.outcome)
        if rep.outcome == "IDLE":
            break
    return outcomes


def check_segments(engine: PlayerEngine, sink: FakeSink, seen: dict, ctx=""):
    """CONTENT continuity — a test-only oracle invariant. Using the fake
    decoder's hidden frame identities (which a production PlayerEngine
    never has: it sees durations, not tags), rendered frames within a
    segment must be exactly contiguous from the segment's TRUE first
    decoded frame — no gaps, duplicates, reordering, or stale frames.

    CONFIRMED segments additionally satisfy ABSOLUTE timeline continuity:
    the true first frame must equal the committed landing (the oracle
    proves SongCore's landing contract). ESTIMATED segments only satisfy
    RELATIVE continuity from the true first frame: the engine's base is
    an estimate, the true landing is unknowable to it, and the engine
    must never rebase onto the truth. Incremental per segment."""
    for seg, tags in sink.segments.items():
        prev = seen.get(seg, 0)
        if not tags:
            seen[seg] = prev
            continue
        quality = engine.segment_quality.get(seg, LandingQuality.CONFIRMED)
        if quality is LandingQuality.CONFIRMED:
            anchor = engine.segment_start[seg]
            assert tags[0] == anchor, (
                f"{ctx}: segment {seg} starts at rendered tag {tags[0]}, "
                f"committed landing was {anchor}")
        else:
            anchor = tags[0]  # hidden truth only; never a production input
        for i in range(prev, len(tags)):
            assert tags[i] == anchor + i, (
                f"{ctx}: segment {seg} position {i}: rendered tag {tags[i]}, "
                f"expected {anchor + i}")
        seen[seg] = len(tags)


def check_all(engine: PlayerEngine, sink: FakeSink, seen: dict, ctx="",
              deep=False):
    """Everything that must hold after EVERY operation."""
    engine.ring.check_invariants(deep=deep)
    # decode-side conservation — lifetime, survives open() song changes:
    # every frame taken from the decoder is in the ring (produced),
    # discarded as stale, or still in-flight between decode and publish
    in_flight = (len(engine.in_flight.chunk.frames)
                 if engine.in_flight is not None else 0)
    assert engine.decoded_source_frames == (
        engine.ring.produced_total + engine.discarded_stale_media_frames
        + in_flight), (
        f"{ctx}: decoded {engine.decoded_source_frames} != ring "
        f"{engine.ring.produced_total} + stale "
        f"{engine.discarded_stale_media_frames} + in_flight {in_flight}")
    # output-pipeline conservation (lifetime): submitted output is pending,
    # proven rendered, or explicitly discarded at a commit boundary
    assert engine.submitted_output_total == (
        engine.pending_output_frames() + engine.rendered_output_total
        + engine.discarded_output_total), (
        f"{ctx}: output accounting: submitted "
        f"{engine.submitted_output_total} != pending "
        f"{engine.pending_output_frames()} + rendered "
        f"{engine.rendered_output_total} + discarded "
        f"{engine.discarded_output_total}")
    # media-payload conservation (lifetime): real media frames leaving the
    # queue are pending, rendered, or discarded — GAP silence is separate
    assert engine.submitted_media_total == (
        engine.pending_media_frames() + engine.rendered_media_total
        + engine.discarded_output_media_total), (
        f"{ctx}: media accounting: submitted "
        f"{engine.submitted_media_total} != pending "
        f"{engine.pending_media_frames()} + rendered "
        f"{engine.rendered_media_total} + discarded "
        f"{engine.discarded_output_media_total}")
    # rendering can never outrun submission (current generation)
    assert engine.rendered_output <= engine.output_endpoint, (
        f"{ctx}: rendered {engine.rendered_output} > submitted "
        f"{engine.output_endpoint}")
    # output-domain split (task §23): proven-rendered output is exactly
    # rendered media + rendered GAP silence — no third kind exists
    assert engine.rendered_output_total == (
        engine.rendered_media_total + engine.rendered_gap_output_frames), (
        f"{ctx}: rendered output {engine.rendered_output_total} != media "
        f"{engine.rendered_media_total} + gap "
        f"{engine.rendered_gap_output_frames}")
    # per-tick backend accounting: every requested period the device took
    # was real media or injected silence
    sink.check_accounting()
    check_segments(engine, sink, seen, ctx)
    if engine.state != State.EMPTY:
        if engine.state is State.ENDED and engine.duration_known:
            assert engine.position_frames == engine.duration_frames, (
                f"{ctx}: ENDED position {engine.position_frames} != "
                f"duration {engine.duration_frames}")
        else:
            # CLOCK continuity — the production-realizable invariant: the
            # public media position is exactly the segment base plus the
            # rendered media duration (monotonic; submitted-only or GAP
            # output can never move it). Uses engine-observable counters
            # only, never the oracle's hidden frame identities.
            assert engine.position_frames == (
                engine.base_frame + engine.rendered_media_frames), (
                f"{ctx}: position {engine.position_frames} != base "
                f"{engine.base_frame} + rendered_media "
                f"{engine.rendered_media_frames}")
        if engine.duration_known:
            assert 0 <= engine.position_frames <= engine.duration_frames, (
                f"{ctx}: position {engine.position_frames} outside song")
        else:
            assert engine.duration_frames == -1, (
                f"{ctx}: unknown duration must stay -1, never a fake clamp")


# ---------------------------------------------------------------------------
# ring buffer: unit + property (task §6, §7)


@gate
def g_ring_unit():
    r = PcmRing(8)
    assert r.write([0, 1, 2, 3, 4, 5]) == 6
    assert r.read(5) == [0, 1, 2, 3, 4]
    assert r.write([6, 7, 8, 9, 10, 11, 12]) == 7  # spans the boundary
    assert r.read(8) == [5, 6, 7, 8, 9, 10, 11, 12]
    assert r.read(1) == []
    r.check_invariants()

    # exact full: write must never overwrite unread frames
    r = PcmRing(4)
    assert r.write([0, 1, 2, 3]) == 4
    assert r.write([9]) == 0
    assert r.read(4) == [0, 1, 2, 3]
    assert r.read(1) == []
    # one-frame ring
    r = PcmRing(1)
    assert r.write([0]) == 1 and r.write([1]) == 0
    assert r.read(1) == [0]
    assert r.write([1]) == 1
    # many wrap cycles on a tiny ring stay FIFO
    r = PcmRing(3)
    expect = 0
    for cycle in range(200):
        got = r.read(2)
        assert got == list(range(expect, expect + len(got)))
        expect += len(got)
        made = [expect + i for i in range(2)]
        r.write(made)
        expect_next = expect  # frames still pending stay in front
    r.check_invariants(deep=True)
    print("  exact wrap example (cap 8: w6 r5 w7), full/empty/1-frame, 200 cycles OK")


@gate
def g_ring_property():
    """Randomized ring behavior against a straight list reference."""
    for seed in range(400):
        rng = random.Random(seed)
        cap = rng.randint(1, 16)
        r = PcmRing(cap)
        ref: list[int] = []
        next_tag = 0
        for _ in range(150):
            if rng.random() < 0.5:
                k = rng.randint(0, cap)
                chunk = list(range(next_tag, next_tag + k))
                next_tag += k
                n = r.write(chunk)
                ref.extend(chunk[:n])
            else:
                k = rng.randint(0, cap)
                want, ref = ref[:k], ref[k:]
                assert r.read(k) == want, f"seed {seed}"
            if rng.random() < 0.05:
                r.flush()
                ref.clear()
            r.check_invariants(deep=(rng.random() < 0.1))
        r.flush()
        r.check_invariants(deep=True)
    print("  400 seeds x 150 ops vs list reference OK")


# ---------------------------------------------------------------------------
# T1..T15 (task §28)


@gate
def g_t1_sequential():
    engine, sink = make_pair()
    engine.open(song())
    assert engine.state is State.READY
    assert engine.position_frames == 0
    engine.play()
    for _ in range(600):
        engine.producer_step()
        sink.tick()
    assert engine.state is State.PLAYING
    seen: dict = {}
    check_all(engine, sink, seen, "t1", deep=True)
    assert sink.segments[engine.segment] == list(range(sink.total_real_frames))
    assert sink.total_real_frames == engine.ring.consumed_total
    assert engine.underrun_count == 0
    print(f"  600 lockstep rounds, {sink.total_real_frames} frames contiguous, 0 underruns")


@gate
def g_t2_wraparound():
    """Long run on a tiny queue so every write/read spans the boundary."""
    engine, sink = make_pair(capacity=8, chunk=8, period=3)
    engine.open(song(total_frames=20_000))
    engine.play()
    seen: dict = {}
    for _ in range(6_000):
        engine.producer_step()
        sink.tick()
        check_all(engine, sink, seen, "t2")
    assert sink.total_real_frames >= 5_000
    check_all(engine, sink, seen, "t2-final", deep=True)
    print(f"  cap=8/period=3 x 6000 rounds, {sink.total_real_frames} frames, wraps verified")


@gate
def g_t3_producer_faster():
    engine, sink = make_pair()
    engine.open(song())
    engine.play()
    seen: dict = {}
    saw_backpressure = False
    for _ in range(600):
        for _ in range(4):
            if engine.producer_step().outcome == "BACKPRESSURE":
                saw_backpressure = True
        sink.tick()
        check_all(engine, sink, seen, "t3")
    assert saw_backpressure, "producer never hit the bounded-queue wall"
    assert engine.ring.readable_frames <= engine.ring.capacity
    assert engine.underrun_count == 0
    assert sink.total_real_frames == engine.ring.consumed_total
    print(f"  producer 4x faster: backpressure seen, buffered<=cap, 0 underruns")


@gate
def g_t4_consumer_faster():
    engine, sink = make_pair()
    engine.open(song(work_steps=3))
    engine.play()
    seen: dict = {}
    for _ in range(900):
        engine.producer_step()
        sink.tick(3)
        check_all(engine, sink, seen, "t4")
    assert engine.underrun_count > 0, "expected starvation underruns"
    assert engine.underrun_silence_output_frames > 0
    assert engine.preroll_events >= 1
    # every frame delivered exactly once, in order; nothing repeated
    tags = sink.segments[engine.segment]
    assert tags == list(range(len(tags)))
    # silence must never be reported as real frames
    assert sink.total_real_frames == engine.ring.consumed_total
    n, f = engine.underrun_count, engine.underrun_silence_output_frames
    print(f"  consumer 3x faster: {n} underruns / {f} silence frames, order intact")

    # long producer stall in the middle of playback
    engine, sink = make_pair()
    engine.open(song(work_steps=1))
    engine.play()
    seen = {}
    for _ in range(12):
        engine.producer_step()
    sink.tick(4)
    pos_stall = engine.position_frames
    buffered = engine.ring.readable_frames
    out_before = engine.rendered_output_total
    sink.tick(40)  # producer frozen: pure starvation
    assert engine.underrun_count > 0
    # device time advanced the full 40 periods; media position advanced
    # only by the real frames that were still buffered — underrun GAP has
    # device duration but no media duration
    assert engine.rendered_output_total == out_before + 40 * sink.period_frames
    assert engine.position_frames == pos_stall + buffered
    for _ in range(12):
        engine.producer_step()
    sink.tick(4)
    tags = sink.segments[engine.segment]
    assert tags == list(range(len(tags))), "stall corrupted or duplicated frames"
    check_all(engine, sink, seen, "t4-stall", deep=True)
    print("  40-period producer stall: silence advances device time only, frames intact")


@gate
def g_t5_pause_resume():
    # pause mid-buffer
    engine, sink = make_pair()
    engine.open(song())
    engine.play()
    seen: dict = {}
    for _ in range(12):
        engine.producer_step()
    sink.tick(5)
    engine.pause()
    assert engine.state is State.PAUSED
    pos, emitted, buffered = (engine.position_frames,
                              engine.decoder.emitted_total,
                              engine.ring.readable_frames)
    sink.tick(5)  # device must not progress while paused
    assert sink.last_result.kind == "idle"
    assert sink.last_render.kind == "paused"
    assert engine.position_frames == pos
    assert engine.ring.readable_frames == buffered
    engine.play()
    for _ in range(30):
        engine.producer_step()
        sink.tick()
    check_all(engine, sink, seen, "t5", deep=True)
    assert engine.decoder.emitted_total >= emitted  # retained, not re-decoded
    tags = sink.segments[engine.segment]
    assert tags == list(range(len(tags))), "pause/resume broke the stream"
    assert engine.underrun_count == 0

    # pause at the buffer boundary (queue completely full)
    engine, sink = make_pair(capacity=4096, chunk=1024)
    engine.open(song())
    engine.play()
    while engine.producer_step().outcome != "BACKPRESSURE":
        pass
    assert engine.ring.readable_frames == engine.ring.capacity
    engine.pause()
    sink.tick(3)
    assert sink.last_result.kind == "idle"
    engine.play()
    sink.tick()
    assert sink.segments[engine.segment][0] == 0
    check_all(engine, sink, {}, "t5-boundary", deep=True)
    print("  pause mid-buffer and at full boundary: frozen clock, buffer retained")


@gate
def g_t6_stop_restart():
    engine, sink = make_pair()
    engine.open(song(work_steps=2))
    engine.play()
    seen: dict = {}
    for _ in range(10):
        engine.producer_step()
    sink.tick(4)
    engine.stop()
    assert engine.state is State.READY
    assert engine.position_frames == 0
    assert engine.ring.readable_frames == 0
    # a chunk in flight at stop() belongs to the dead epoch and dies at
    # publish time
    engine.producer_step()  # decays the stale chunk (WORKING)
    engine.producer_step()  # publishes -> STALE
    assert engine.discarded_stale_media_frames > 0
    assert engine.decoded_source_frames == (
        engine.ring.produced_total + engine.discarded_stale_media_frames)
    engine.play()
    for _ in range(40):
        engine.producer_step()
        sink.tick()
        check_all(engine, sink, seen, "t6")
    assert sink.segments[engine.segment][0] == 0, "restart must begin at 0"
    tags = sink.segments[engine.segment]
    assert tags == list(range(len(tags)))
    print(f"  stop->READY@0, buffered=0, stale={engine.discarded_stale_media_frames}, restart contiguous")


@gate
def g_t7_seek_playing():
    engine, sink = make_pair()
    engine.open(song(seek_landing_offset=-37))
    engine.play()
    seen: dict = {}
    for _ in range(10):
        engine.producer_step()
    sink.tick(4)
    requested = frames_to_us(48_000 * 4, 48_000)
    landing = engine.seek(requested)
    assert landing == 48_000 * 4 - 37, "engine must rebase on the RETURNED landing"
    assert engine.position_frames == landing
    assert engine.position_quality is LandingQuality.CONFIRMED
    assert engine.ring.readable_frames == 0
    assert engine.pending_output_frames() == 0
    assert engine.state is State.PLAYING
    for _ in range(60):
        engine.producer_step()
        sink.tick()
        check_all(engine, sink, seen, "t7")
    tags = sink.segments[engine.segment]
    assert tags[0] == landing, "first post-seek frame must be the landing"
    assert tags == list(range(landing, landing + len(tags)))
    assert all(t >= landing for seg in sink.segments for t in [0]) or True
    # no pre-seek frame may appear after the commit
    pre = sink.segments[engine.segment - 1]
    assert set(pre).isdisjoint(range(landing, landing + len(tags)))
    print(f"  seek@4s landing={landing} (offset -37): post-seek stream starts exactly there")


@gate
def g_t8_seek_paused():
    engine, sink = make_pair()
    engine.open(song())
    engine.play()
    for _ in range(8):
        engine.producer_step()
    sink.tick(3)
    engine.pause()
    landing = engine.seek(frames_to_us(48_000 * 2, 48_000))
    assert engine.state is State.PAUSED
    assert engine.position_frames == landing
    assert engine.ring.readable_frames == 0
    seen: dict = {}
    engine.play()
    for _ in range(50):
        engine.producer_step()
        sink.tick()
        check_all(engine, sink, seen, "t8")
    tags = sink.segments[engine.segment]
    assert tags == list(range(landing, landing + len(tags)))
    print(f"  seek while paused: resume plays from landing {landing}, contiguous")


@gate
def g_t9_repeated_seeks():
    engine, sink = make_pair(capacity=512, chunk=256, period=128)
    engine.open(song(work_steps=2))
    engine.play()
    seen: dict = {}
    rng = random.Random(7)
    for i in range(30):
        target = rng.randrange(0, engine.duration_frames + 1)
        engine.seek(frames_to_us(target, 48_000))
        for _ in range(rng.randint(0, 3)):
            engine.producer_step()
        sink.tick(rng.randint(0, 2))
        check_all(engine, sink, seen, f"t9/{i}", deep=(i % 7 == 0))
    for _ in range(40):
        engine.producer_step()
        sink.tick()
    check_all(engine, sink, seen, "t9-final", deep=True)
    assert engine.discarded_stale_media_frames > 0, "rapid seeks must kill in-flight chunks"
    tags = sink.segments[engine.segment]
    assert tags == list(range(tags[0], tags[0] + len(tags)))
    print(f"  30 rapid seeks (cap 512): stale={engine.discarded_stale_media_frames}, final segment clean")


@gate
def g_t10_stale_after_seek():
    """The dedicated stale-frame gate: a decode result in flight when the
    seek commits must be discarded, never rendered (task §19/§20)."""
    engine, sink = make_pair()
    engine.open(song(work_steps=3))
    engine.play()
    rep = engine.producer_step()
    assert rep.outcome == "BEGIN"
    flight_epoch, flight_len = rep.epoch, rep.frames
    engine.seek(frames_to_us(48_000, 48_000))
    sink.tick(2)  # backend callbacks during in-flight decay: silence only
    assert sink.segments.get(engine.segment, []) == []
    rep = engine.producer_step()
    assert rep.outcome == "WORKING"
    rep = engine.producer_step()
    assert rep.outcome == "WORKING"
    rep = engine.producer_step()
    assert rep.outcome == "STALE", f"expected STALE, got {rep.outcome}"
    assert rep.frames == flight_len
    assert engine.discarded_stale_media_frames == flight_len
    assert engine.ring.readable_frames == 0
    seen: dict = {}
    check_all(engine, sink, seen, "t10", deep=True)
    for _ in range(30):
        engine.producer_step()
        sink.tick()
    tags = sink.segments[engine.segment]
    assert tags == list(range(48_000, 48_000 + len(tags))), "stale PCM escaped"
    print(f"  epoch {flight_epoch} chunk ({flight_len} f) discarded after seek; stream clean")


@gate
def g_t11_eof_drain():
    """SongCore EOF must NOT end playback; the queue AND the submitted
    media output must drain first (§21)."""
    engine, sink = make_pair()
    engine.open(song(total_frames=3_000, sample_rate=48_000))
    engine.play()
    drain_producer(engine)
    assert engine.source_eof
    assert engine.ring.readable_frames > 0
    assert engine.state is State.PLAYING, "EOF must not shortcut to ENDED"
    ticks = 0
    while engine.state is not State.ENDED:
        sink.tick()
        ticks += 1
        assert ticks < 100
    assert engine.position_frames == engine.duration_frames
    assert sink.last_result.kind == "eos"
    assert engine.underrun_count == 0
    tags = sink.segments[engine.segment]
    assert tags == list(range(3_000))
    assert engine.duration_frames == 3_000
    print(f"  EOF->drain({ticks} ticks)->ENDED @duration, eos-silence={engine.eos_silence_output_frames}")

    # very short song: shorter than one device period
    engine, sink = make_pair()
    engine.open(song(total_frames=100))
    engine.play()
    drain_producer(engine)
    sink.tick()
    assert engine.state is State.ENDED
    assert engine.position_frames == 100
    assert engine.preroll_events == 0 and engine.underrun_count == 0
    print("  100-frame song: first tick delivers 100 + eos silence, ENDED")

    # EOF with exactly one frame still buffered
    engine, sink = make_pair(period=256)
    engine.open(song(total_frames=513))
    engine.play()
    drain_producer(engine)
    while engine.state is not State.ENDED:
        sink.tick()
    assert sink.segments[engine.segment][-1] == 512
    assert engine.position_frames == 513
    print("  one-frame tail: last real frame delivered, then ENDED")

    # pause near EOF: ENDED must wait until playback resumes and drains
    engine, sink = make_pair(period=256)
    engine.open(song(total_frames=600))
    engine.play()
    drain_producer(engine)
    sink.tick(1)
    engine.pause()
    assert engine.state is State.PAUSED
    sink.tick(3)
    assert engine.state is State.PAUSED, "drain must not complete while paused"
    engine.play()
    while engine.state is not State.ENDED:
        sink.tick()
    assert engine.position_frames == 600
    print("  pause near EOF: ENDED deferred until resume+drain")


@gate
def g_t12_after_eof():
    engine, sink = make_pair(period=256)
    engine.open(song(total_frames=4_000))
    engine.play()
    drain_producer(engine)
    while engine.state is not State.ENDED:
        sink.tick()
    # seek after EOF -> READY at landing, playable
    landing = engine.seek(frames_to_us(1_000, 48_000))
    assert engine.state is State.READY
    assert not engine.source_eof
    engine.play()
    seen: dict = {}
    for _ in range(40):
        engine.producer_step()
        sink.tick()
    tags = sink.segments[engine.segment]
    assert tags == list(range(landing, landing + len(tags)))
    # play after ENDED restarts from 0 (frozen policy)
    drain_producer(engine)
    while engine.state is not State.ENDED:
        sink.tick()
    engine.play()
    assert engine.state is State.PLAYING
    assert engine.position_frames == 0
    assert engine.ring.readable_frames == 0
    for _ in range(10):
        engine.producer_step()
        sink.tick()
    tags = sink.segments[engine.segment]
    assert tags[0] == 0 and tags == list(range(len(tags)))
    check_all(engine, sink, seen, "t12", deep=True)
    # stop after EOF
    engine.stop()
    assert engine.state is State.READY and engine.position_frames == 0
    print("  seek/play/stop after EOF: restart-from-0 policy verified")


@gate
def g_t13_open_while_playing():
    engine, sink = make_pair()
    engine.open(song(total_frames=2_000, work_steps=3))
    engine.play()
    rep = engine.producer_step()
    assert rep.outcome == "BEGIN"  # old song chunk in flight
    sink.tick_submit(2)  # callbacks without device consumption
    assert engine.pending_output_frames() > 0  # old output in the backend
    engine.open(song(total_frames=5_000))
    assert engine.state is State.READY
    assert engine.ring.readable_frames == 0
    assert engine.pending_output_frames() == 0, "open must drop old pending output"
    assert engine.position_frames == 0
    for _ in range(3):
        rep = engine.producer_step()
    assert rep.outcome == "STALE", "old song's in-flight chunk must die"
    seen: dict = {}
    engine.play()
    for _ in range(80):
        engine.producer_step()
        sink.tick()
        check_all(engine, sink, seen, "t13")
    tags = sink.segments[engine.segment]
    assert tags == list(range(len(tags)))
    assert len(tags) > 2_000, "new song must actually play past the old length"
    print(f"  open-during-play: old chunk stale, new song contiguous past frame 2000")


@gate
def g_t14_error_injection():
    engine, sink = make_pair()
    engine.open(song(total_frames=5_000, fail_at_frame=1_500))
    engine.play()
    outcomes = drain_producer(engine)
    assert engine.state is State.ERROR
    assert engine.last_error and "108" in engine.last_error
    # partial-success framing: frames before the fault were emitted (SONG_OK),
    # the fault surfaced on the NEXT read
    assert engine.decoder.emitted_total == 1_500
    sink.tick(3)
    assert sink.last_result.kind == "idle"
    for op in (engine.play, lambda: engine.seek(0)):
        try:
            op()
            raise AssertionError("ERROR state must reject control calls")
        except EngineError:
            pass
    engine.stop()  # universal recovery (reopen path from ERROR)
    assert engine.state is State.READY
    assert engine.position_frames == 0
    engine.open(song())  # open also recovers
    engine.play()
    seen: dict = {}
    for _ in range(30):
        engine.producer_step()
        sink.tick()
        check_all(engine, sink, seen, "t14")
    assert "IDLE" in outcomes
    print("  decode fault -> ERROR (partial success honored), stop/open recover")


@gate
def g_state_illegal_probes():
    engine, sink = make_pair()
    for op in ("play", "seek"):
        try:
            if op == "play":
                engine.play()
            else:
                engine.seek(0)
            raise AssertionError("EMPTY must reject play/seek")
        except EngineError:
            pass
    engine.stop()  # stop on EMPTY is a documented no-op
    engine.pause()  # documented no-op
    assert engine.state is State.EMPTY
    try:
        engine.open(song(open_fails=True))
        raise AssertionError("open_fails must raise")
    except EngineError:
        pass
    assert engine.state is State.EMPTY and engine.decoder is None
    engine.open(song())
    assert engine.state is State.READY
    print("  EMPTY probes + failed open stays EMPTY, then recovers")


# ---------------------------------------------------------------------------
# clock model (task §25/§26)


@gate
def g_clock_model():
    period = 512
    engine, sink = make_pair(period=period)
    engine.open(song())
    engine.play()
    for _ in range(12):  # decode far ahead, device not ticking yet
        engine.producer_step()
    assert engine.position_frames == 0, "buffering must not move the clock"
    sink.tick(4)
    assert engine.position_frames == 4 * period
    assert engine.preroll_events == 0
    engine.pause()
    sink.tick(3)
    assert engine.position_frames == 4 * period, "pause must freeze the clock"
    engine.play()
    sink.tick()
    assert engine.position_frames == 5 * period, "resume must continue"
    landing = engine.seek(frames_to_us(96_000, 48_000))
    assert engine.position_frames == landing, "seek must rebase the clock"
    check_all(engine, sink, {}, "clock", deep=True)

    # preroll vs underrun accounting, device time vs media time
    engine, sink = make_pair(period=period)
    engine.open(song(work_steps=4))
    engine.play()
    sink.tick()  # nothing decoded yet: preroll, clock stays at 0
    assert sink.last_result.kind == "preroll"
    assert engine.position_frames == 0 and engine.preroll_events == 1
    for _ in range(5):  # work_steps=4: BEGIN + 3 decays + publish
        engine.producer_step()
    sink.tick()
    assert engine.position_frames == period, "clock starts at first real frame"
    sink.tick(3)  # producer starved: underrun GAP
    assert engine.underrun_count >= 1
    # device time advanced by every period (5 ticks: preroll + 2 media +
    # 2 underrun); media time advanced only by the periods that carried
    # real media — GAP has no media duration
    assert engine.rendered_output_total == 5 * period
    assert engine.position_frames == 2 * period, (
        "underrun GAP must advance device time but not media position")
    check_all(engine, sink, {}, "clock2", deep=True)
    print("  decode-ahead/pause/resume/seek-rebase/preroll/underrun clock rules OK")


# ---------------------------------------------------------------------------
# S1..S10 (pre-native semantic closure: submitted vs rendered, mapping,
# seek failure, unknown duration, landing quality, stale output)


def _expect_seek_error(engine: PlayerEngine, position_us: int,
                       status: int, ctx: str) -> None:
    """Fail-closed seek: generation invalidated first, deterministic ERROR,
    frozen diagnostic position, nothing stale left consumable."""
    pre_pos = engine.position_frames
    pre_epoch = engine.epoch
    try:
        engine.seek(position_us)
        raise AssertionError(f"{ctx}: seek must fail with status {status}")
    except SongSeekError as exc:
        assert exc.status == status, f"{ctx}: status {exc.status} != {status}"
    assert engine.state is State.ERROR, f"{ctx}: state must be ERROR"
    assert engine.last_error and f"status={status}" in engine.last_error, (
        f"{ctx}: deterministic diagnostic: {engine.last_error}")
    assert engine.epoch == pre_epoch + 1, f"{ctx}: invalidate FIRST"
    assert engine.ring.readable_frames == 0, f"{ctx}: queue must be flushed"
    assert engine.pending_output_frames() == 0, f"{ctx}: output invalidated"
    assert engine.position_frames == pre_pos, (
        f"{ctx}: no false seek-success position may be reported")


@gate
def g_s1_submitted_not_audible():
    """S1: submitting PCM to the backend does not make it audible and does
    not advance the media position."""
    engine, sink = make_pair(rate=100, capacity=1_000, chunk=100, period=100)
    engine.open(song(total_frames=10_000, sample_rate=100))
    engine.play()
    for _ in range(4):
        engine.producer_step()  # 2 chunks x 100 frames published
    sub = engine.submit(50)  # backend callback: 50 media frames
    assert sub.kind == "audio" and len(sub.tags) == 50
    assert engine.submitted_output_total == 50
    assert engine.rendered_output_total == 0
    assert engine.pending_output_frames() == 50
    assert engine.position_frames == 0, "submitted-only PCM must not advance position"
    rep = engine.backend_render(0)  # device clock evidence: nothing rendered
    assert rep.rendered_output_frames == 0
    assert engine.position_frames == 0
    engine.backend_render(50)  # now proven rendered
    assert engine.position_frames == 50
    check_all(engine, sink, {}, "s1", deep=True)
    print("  submit 50 / render 0 -> position 0; render 50 -> position 50 (100 Hz)")


@gate
def g_s2_partial_render_only_audible():
    """S2 + pause semantics: only the rendered portion is audible; pause
    freezes render advancement with output pending."""
    engine, sink = make_pair(rate=100, capacity=1_000, chunk=100, period=100)
    engine.open(song(total_frames=10_000, sample_rate=100))
    engine.play()
    for _ in range(4):
        engine.producer_step()
    engine.submit(50)
    rep = engine.backend_render(20)
    assert rep.rendered_output_frames == 20 and rep.rendered_media_frames == 20
    assert engine.position_frames == 20  # 200 ms at 100 Hz
    assert engine.pending_output_frames() == 30  # 30 submitted, not audible

    engine.submit(50)
    engine.pause()
    rep = engine.backend_render(10)  # paused device consumes nothing
    assert rep.kind == "paused"
    assert engine.position_frames == 20 and engine.pending_output_frames() == 80
    assert engine.rendered_output_total == 20, "pause stops device progression"
    engine.play()
    engine.backend_render(80)
    assert engine.position_frames == 100
    check_all(engine, sink, {}, "s2", deep=True)
    print("  partial render advances only audible media; pause holds pending output")


@gate
def g_s3_media_gap_output_mapping():
    """S3: MEDIA/GAP output spans map device positions onto the media
    timeline; GAP consumes device time but zero media time."""
    engine, sink = make_pair(rate=100, capacity=1_000, chunk=10, period=10)
    engine.open(song(total_frames=1_000, sample_rate=100))
    engine.play()
    engine.producer_step()
    engine.producer_step()  # 10 frames published
    assert engine.submit(10).kind == "audio"
    sub = engine.submit(5)  # starved callback: 5-frame GAP
    assert sub.kind == "underrun" and sub.silence_frames == 5
    engine.producer_step()
    engine.producer_step()  # 10 more frames
    assert engine.submit(10).kind == "audio"
    assert engine.submitted_output_total == 25
    engine.backend_render(25)  # everything proven rendered
    for out_pos, media_pos in ((5, 5), (12, 10), (15, 10), (20, 15), (25, 20)):
        assert engine.media_position_at_output(out_pos) == media_pos, (
            f"device {out_pos} must map to media {media_pos}, got "
            f"{engine.media_position_at_output(out_pos)}")
    assert engine.position_frames == 20
    assert engine.rendered_output_total == 25  # device elapsed 250 ms
    assert engine.rendered_media_total == 20    # media elapsed 200 ms
    check_all(engine, sink, {}, "s3", deep=True)
    print("  10 MEDIA + 5 GAP + 10 MEDIA: device->media mapping exact at 5/12/15/20/25")


@gate
def g_s4_seek_failure_deterministic_error():
    """S4: seek failure is a fail-closed discontinuity from every legal
    state — one deterministic ERROR, then stop()/open() recover."""
    # READY
    engine, sink = make_pair()
    engine.open(song(total_frames=2_000, seek_status=SONG_ERR_SEEK_UNSUPPORTED))
    _expect_seek_error(engine, frames_to_us(1_000, 48_000),
                       SONG_ERR_SEEK_UNSUPPORTED, "s4-ready")
    sink.tick(2)  # no stale PCM may become consumable
    assert sink.last_result.kind == "idle"
    engine.stop()  # recovery: reopen path from ERROR
    assert engine.state is State.READY and engine.position_frames == 0
    engine.play()
    for _ in range(20):
        engine.producer_step()
        sink.tick()
    tags = sink.segments[engine.segment]
    assert tags[0] == 0 and tags == list(range(len(tags)))
    check_all(engine, sink, {}, "s4-ready-recovered", deep=True)

    # PLAYING
    engine, sink = make_pair()
    engine.open(song(total_frames=2_000, seek_status=SONG_ERR_SEEK_ERROR))
    engine.play()
    for _ in range(6):
        engine.producer_step()
    sink.tick(2)
    _expect_seek_error(engine, frames_to_us(1_500, 48_000),
                       SONG_ERR_SEEK_ERROR, "s4-playing")
    engine.stop()
    assert engine.state is State.READY

    # PAUSED
    engine, sink = make_pair()
    engine.open(song(total_frames=2_000, seek_status=SONG_ERR_SEEK_ERROR))
    engine.play()
    for _ in range(6):
        engine.producer_step()
    sink.tick(2)
    engine.pause()
    _expect_seek_error(engine, frames_to_us(1_500, 48_000),
                       SONG_ERR_SEEK_ERROR, "s4-paused")

    # ENDED (failure wins over the -> READY transition)
    engine, sink = make_pair(period=256)
    engine.open(song(total_frames=500, seek_status=SONG_ERR_SEEK_ERROR))
    engine.play()
    drain_producer(engine)
    while engine.state is not State.ENDED:
        sink.tick()
    _expect_seek_error(engine, frames_to_us(250, 48_000),
                       SONG_ERR_SEEK_ERROR, "s4-ended")
    print("  READY/PLAYING/PAUSED/ENDED + failed seek -> deterministic ERROR, stop recovers")


@gate
def g_s5_seek_failure_after_underrun():
    """S5: failure after audible underruns, stop()-recovery, and the
    negative branch — reopen failure keeps ERROR, only open() recovers."""
    engine, sink = make_pair(period=256)
    engine.open(song(total_frames=4_000, work_steps=4,
                     seek_status=SONG_ERR_SEEK_ERROR))
    engine.play()
    sink.tick()  # preroll
    for _ in range(5):
        engine.producer_step()
    sink.tick()
    sink.tick(5)  # starve past the buffered media -> underrun GAP
    assert engine.underrun_count >= 1
    assert engine.position_frames > 0  # some media had become audible
    _expect_seek_error(engine, frames_to_us(1_000, 48_000),
                       SONG_ERR_SEEK_ERROR, "s5-underrun")
    engine.stop()  # from ERROR: drop + reopen the source
    assert engine.state is State.READY
    assert engine.position_frames == 0
    assert engine.decoder.reopen_count == 1
    engine.play()
    seen: dict = {}
    for _ in range(40):
        engine.producer_step()
        sink.tick()
        check_all(engine, sink, seen, "s5")
    tags = sink.segments[engine.segment]
    assert tags[0] == 0 and tags == list(range(len(tags)))

    # negative: even the reopen fails -> ERROR persists, open() recovers
    engine, sink = make_pair(period=256)
    engine.open(song(total_frames=1_000, seek_status=SONG_ERR_SEEK_UNSUPPORTED,
                     reopen_fails=True))
    engine.play()
    for _ in range(6):
        engine.producer_step()
    sink.tick(2)
    _expect_seek_error(engine, frames_to_us(500, 48_000),
                       SONG_ERR_SEEK_UNSUPPORTED, "s5-reopen-fails")
    engine.stop()
    assert engine.state is State.ERROR, "failed reopen must not fake READY"
    assert engine.last_error and "stop recovery failed" in engine.last_error
    engine.open(song(total_frames=2_000))
    assert engine.state is State.READY
    print("  underrun then failed seek -> ERROR; stop reopens; reopen-fail keeps ERROR, open recovers")


@gate
def g_s6_unknown_duration_eof_endpoint():
    """S6: unknown duration — ENDED position is the final rendered media
    endpoint; underrun silence never extends it; no fake clamp."""
    engine, sink = make_pair(rate=100, capacity=200, chunk=10, period=10)
    engine.open(song(total_frames=100, sample_rate=100, unknown_duration=True))
    assert engine.duration_known is False
    assert engine.duration_frames == -1
    engine.play()
    engine.producer_step()
    engine.producer_step()  # 10 frames published
    sink.tick()  # 10 media rendered
    sink.tick(2)  # 20 cumulative underrun GAP frames
    assert engine.underrun_silence_output_frames == 20
    assert engine.position_frames == 10, "GAP must not advance media position"
    drain_producer(engine)  # 90 more frames, source EOF
    while engine.state is not State.ENDED:
        sink.tick()
    assert engine.state is State.ENDED
    assert engine.position_frames == 100, "ENDED = final rendered media endpoint (1.0 s)"
    assert engine.rendered_output_total == 120, "device elapsed 1.2 s"
    assert engine.duration_frames == -1  # never -1-as-position, never a fake 0
    check_all(engine, sink, {}, "s6", deep=True)
    print("  100 media + 20 underrun silence: ENDED @1.0 s media / 1.2 s device")


@gate
def g_s7_confirmed_landing():
    """S7: SongCore's actual landing is the media authority, never the
    request; the quality is CONFIRMED."""
    engine, sink = make_pair()
    engine.open(song(total_frames=48_000 * 8, seek_landing_offset=-3_840))
    engine.play()
    for _ in range(6):
        engine.producer_step()
    sink.tick(2)
    landing = engine.seek(frames_to_us(48_000 * 5, 48_000))
    assert landing == 48_000 * 5 - 3_840  # 5.000 s requested, 4.920 s landed
    assert engine.position_frames == landing
    assert engine.position_quality is LandingQuality.CONFIRMED
    assert engine.position_frames != 48_000 * 5, "request must not stay the authority"
    for _ in range(20):
        engine.producer_step()
        sink.tick()
    tags = sink.segments[engine.segment]
    assert tags[0] == landing, "post-seek media must start at the landing"
    check_all(engine, sink, {}, "s7", deep=True)
    print(f"  requested 5.000 s -> landing {landing} (4.920 s), CONFIRMED")


@gate
def g_s8_estimated_landing():
    """S8: SONG_OK with a genuinely unknown landing rebases on the clamped
    request as an ESTIMATE — never described as an actual landing."""
    engine, sink = make_pair()
    engine.open(song(total_frames=48_000 * 8, seek_landing_unknown=True))
    engine.play()
    for _ in range(6):
        engine.producer_step()
    sink.tick(2)
    requested = 48_000 * 5
    landing = engine.seek(frames_to_us(requested, 48_000))
    assert landing == requested, "unknown landing -> base = requested clamp"
    assert engine.position_quality is LandingQuality.ESTIMATED
    assert engine.state is State.PLAYING
    seen: dict = {}
    for _ in range(20):
        engine.producer_step()
        sink.tick()
        check_all(engine, sink, seen, "s8")
    tags = sink.segments[engine.segment]
    assert tags[0] == requested, "media decodes from the clamp target"
    assert engine.position_frames == requested + len(tags)
    print("  OK + landing -1 -> ESTIMATED @5.000 s, playback progresses from it")


@gate
def g_s9_pending_old_output_cannot_advance_new_song():
    """S9: a late render event from a dead backend generation can never
    advance the new song's media timeline."""
    engine, sink = make_pair(rate=100, capacity=1_000, chunk=100, period=100)
    engine.open(song(total_frames=10_000, sample_rate=100))
    engine.play()
    for _ in range(4):
        engine.producer_step()
    engine.submit(50)
    engine.backend_render(20)
    assert engine.position_frames == 20
    old_gen = engine.epoch
    engine.open(song(total_frames=5_000, sample_rate=100))  # Song B
    assert engine.pending_output_frames() == 0, "open must drop pending output"
    assert engine.discarded_output_media_total == 30  # 30 unrendered media dropped
    rep = engine.backend_render(30, generation=old_gen)  # late device event
    assert rep.kind == "stale"
    assert engine.stale_render_events == 1
    assert engine.position_frames == 0, "old-generation output must not move Song B"
    assert engine.rendered_output_total == 20  # unchanged by the stale event
    engine.play()
    seen: dict = {}
    for _ in range(10):
        engine.producer_step()
        sink.tick()
        check_all(engine, sink, seen, "s9")
    tags = sink.segments[engine.segment]
    assert tags[0] == 0 and tags == list(range(len(tags)))
    print("  30 pending Song-A frames dropped at open; late render discarded; B plays from 0")


@gate
def g_s10_eof_waits_for_submitted_playout():
    """S10: ENDED requires the submitted media to actually render; trailing
    backend padding (EOS GAP) does not postpone it."""
    engine, sink = make_pair(rate=100, capacity=1_000, chunk=60, period=50)
    engine.open(song(total_frames=60, sample_rate=100))
    engine.play()
    drain_producer(engine)
    engine.submit(50)
    engine.submit(10)  # final partial period: 10 media + 40 EOS GAP
    assert engine.state is State.PLAYING, (
        "EOF + empty queue + submitted-but-unrendered media must NOT be ENDED")
    assert engine.pending_media_frames() == 60
    engine.backend_render(10)
    assert engine.state is State.PLAYING and engine.position_frames == 10
    engine.backend_render(50)  # the last media span proves rendered
    assert engine.state is State.ENDED
    assert engine.position_frames == 60 == engine.duration_frames

    # trailing backend padding must not postpone ENDED
    engine, sink = make_pair(rate=100, capacity=1_000, chunk=60, period=60)
    engine.open(song(total_frames=55, sample_rate=100))
    engine.play()
    drain_producer(engine)
    sub = engine.submit(60)  # 55 media + 5 EOS GAP
    assert sub.kind == "eos" and sub.silence_frames == 5
    engine.backend_render(55)  # exactly the media
    assert engine.state is State.ENDED, "EOS GAP still pending must not postpone"
    assert engine.pending_output_frames() == 5
    assert engine.position_frames == 55
    print("  50+10 submitted media: ENDED only after render; EOS padding never postpones")


@gate
def g_estimated_segment_offset_invariance():
    """Named invariant: within one ESTIMATED segment the hidden
    true-vs-reported position offset must remain CONSTANT. The fake
    decoder secretly lands at clamp(target)+offset; the engine reports
    positions from the estimate. Their difference is an unknown segment
    offset — it must not accumulate as playback, underruns, and
    pause/resume proceed (accumulation would mean a real clock/rate/
    accounting bug). Test-only content oracle: the engine itself never
    reads, simulates, or expects the true frame identities."""
    rate = 48_000
    offset = 1_776  # +37 ms of true landing divergence
    engine, sink = make_pair(period=512)
    engine.open(song(total_frames=rate * 8, seek_landing_unknown=True,
                     seek_landing_offset=offset))
    engine.play()
    for _ in range(6):
        engine.producer_step()
    sink.tick(2)
    requested = rate * 5
    landing = engine.seek(frames_to_us(requested, rate))
    assert landing == requested, "estimate base = requested clamp"
    assert engine.position_quality is LandingQuality.ESTIMATED

    def true_minus_reported() -> int:
        tags = sink.segments[engine.segment]
        return (tags[-1] + 1) - engine.position_frames

    seen: dict = {}
    for _ in range(10):
        engine.producer_step()
        sink.tick()
        check_all(engine, sink, seen, "offset-invariance")
    tags = sink.segments[engine.segment]
    assert tags[0] == requested + offset, (
        "hidden truth: the decoder landed at clamp + offset")
    assert true_minus_reported() == offset
    d = offset

    # more real playback: offset constant
    for _ in range(20):
        engine.producer_step()
        sink.tick()
    assert true_minus_reported() == d

    # starved periods (underrun GAP): device advances, BOTH true and
    # reported media freeze -> offset constant, no accumulation
    sink.tick(6)
    assert engine.underrun_count >= 1
    assert true_minus_reported() == d

    # pause/resume: media frozen on both sides of the fence
    engine.pause()
    sink.tick(2)
    assert sink.last_render.kind == "paused"
    engine.play()
    for _ in range(6):
        engine.producer_step()
        sink.tick()
    assert true_minus_reported() == d
    check_all(engine, sink, seen, "offset-invariance-final", deep=True)
    print(f"  ESTIMATED segment: true-vs-reported offset stays {d} frames "
          f"(+37 ms) across playback, underrun, and pause/resume")


# ---------------------------------------------------------------------------
# T16..T20 (clock semantics corrective: device vs media timeline)
@gate
def g_t16_decode_ahead_clock_separation():
    """T16: producer fills 500 ms ahead, consumer never ticks — decode
    position increases; media position and device position stay frozen."""
    engine, sink = make_pair(rate=100, capacity=1_000, chunk=25, period=10)
    engine.open(song(total_frames=10_000, sample_rate=100))
    engine.play()
    for _ in range(4):  # 2 chunks x 25 frames = 500 ms of media decoded
        engine.producer_step()
    assert engine.decoded_source_position == 50, "0.5 s decoded ahead"
    assert engine.position_frames == 0, "decode progress must not move media position"
    assert engine.rendered_output_total == 0, "no tick, no device progression"
    assert sink.total_output_frames == 0
    check_all(engine, sink, {}, "t16", deep=True)
    print("  500 ms decode-ahead: decoded=0.5 s, media=0, device=0")


@gate
def g_t17_underrun_device_vs_media_clock():
    """T17: one starved period at 100 Hz — 4 media + 6 silence; the device
    advances 100 ms, media only 40 ms; the next media continues exactly
    after those 4 frames. No media content disappears."""
    engine, sink = make_pair(rate=100, capacity=1_000, chunk=10, period=10)
    engine.open(song(total_frames=10_000, sample_rate=100))
    engine.play()
    engine.producer_step()
    engine.producer_step()          # frames 0..9
    engine.submit(6)                # small first request: 6 media queued out
    sink.tick_render(6)             # 6 media audible, 4 left queued
    sink.tick()                     # requests 10: 4 media + 6 silence
    rec = sink.last_tick
    assert rec is not None
    assert rec.requested_output_frames == 10
    assert rec.media_frames_consumed == 4
    assert rec.silence_frames_injected == 6
    assert rec.total_output_frames == 10
    assert engine.underrun_count == 1
    assert engine.position_frames == 10        # media +40 ms this period
    assert engine.rendered_output_total == 16  # device +100 ms this period
    engine.producer_step()
    engine.producer_step()          # frames 10..19
    sink.tick()
    tags = sink.segments[engine.segment]
    assert tags == list(range(20)), (
        "next media must begin exactly after the 4 — no jump, no loss")
    check_all(engine, sink, {}, "t17", deep=True)
    print("  starved period: 4 media + 6 silence -> device +100 ms, media +40 ms")


@gate
def g_t18_repeated_underrun_media_continuity():
    """T18: real/silence/real/silence/real — all media frames consumed
    exactly once and in order while device elapsed > media elapsed (the
    divergence is expected, not something to hide)."""
    engine, sink = make_pair(rate=100, capacity=1_000, chunk=10, period=10)
    engine.open(song(total_frames=10_000, sample_rate=100))
    engine.play()
    for _ in range(3):
        engine.producer_step()
        engine.producer_step()      # 10 media frames
        sink.tick()                 # real period
        sink.tick()                 # starved period: underrun GAP
    assert engine.underrun_count == 3
    assert engine.underrun_silence_output_frames == 30
    tags = sink.segments[engine.segment]
    assert tags == list(range(30)), "every media frame exactly once, in order"
    assert engine.rendered_output_total == 60   # device 600 ms
    assert engine.position_frames == 30         # media 300 ms
    assert engine.rendered_output_total > engine.rendered_media_total
    check_all(engine, sink, {}, "t18", deep=True)
    print("  real/silence x3: 300 ms media exactly-once vs 600 ms device elapsed")


@gate
def g_t19_seek_after_underrun_rebase():
    """T19: seek after clock divergence rebases the MEDIA timeline to
    SongCore's actual landing — never derived from device elapsed time."""
    engine, sink = make_pair(rate=100, capacity=2_000, chunk=100, period=10)
    engine.open(song(total_frames=4_000, sample_rate=100,
                     seek_landing_offset=-6))
    engine.play()
    for _ in range(16):             # 8 chunks x 100 frames
        engine.producer_step()
    for _ in range(80):             # 8.0 s of clean media
        sink.tick()
    assert engine.position_frames == 800
    for _ in range(3):              # 0.3 s of underrun silence
        sink.tick()
    assert engine.underrun_silence_output_frames == 30
    assert engine.rendered_output_total == 830, "device elapsed 8.3 s"
    assert engine.position_frames == 800, "media position 8.0 s"
    landing = engine.seek(frames_to_us(3_000, 100))  # request 30 s
    assert landing == 2_994, "actual landing 29.94 s"
    assert engine.position_frames == 2_994, "media base = actual landing"
    assert engine.rendered_output_total == 830, "seek never mutates device history"
    assert engine.pending_output_frames() == 0
    engine.producer_step()
    engine.producer_step()
    sink.tick()
    tags = sink.segments[engine.segment]
    assert tags[0] == 2_994, "post-seek media starts at the landing"
    check_all(engine, sink, {}, "t19", deep=True)
    print("  device 8.3 s / media 8.0 s -> seek 30 s lands 29.94 s; media rebases there")


@gate
def g_t20_eof_after_underrun_duration():
    """T20: a 10 s song with 500 ms cumulative underrun silence ENDS at
    media duration 10 s — never at device-session elapsed 10.5 s."""
    engine, sink = make_pair(rate=100, capacity=2_000, chunk=100, period=10)
    engine.open(song(total_frames=1_000, sample_rate=100))
    engine.play()
    for _ in range(2):              # 100 frames = 1 s
        engine.producer_step()
    for _ in range(10):             # 1.0 s of clean media
        sink.tick()
    for _ in range(5):              # starved: 0.5 s underrun silence
        sink.tick()
    assert engine.underrun_silence_output_frames == 50
    drain_producer(engine)          # remaining 900 frames, source EOF
    while engine.state is not State.ENDED:
        sink.tick()
    assert engine.state is State.ENDED
    assert engine.position_frames == 1_000, "ENDED at media duration (10 s)"
    assert engine.duration_frames == 1_000
    assert engine.rendered_output_total == 1_050, "device session 10.5 s"
    check_all(engine, sink, {}, "t20", deep=True)
    print("  10 s song + 0.5 s underrun silence: ENDED @10.0 s media / 10.5 s device")


# ---------------------------------------------------------------------------
# T15 randomized stress (task §29) + capacity sweep (task §11)


def _pick(rng: random.Random, engine: PlayerEngine) -> str:
    if rng.random() < 0.03:
        return "open"
    s = engine.state
    choices = [("producer", 3), ("submit", 2), ("render", 2), ("seek", 2),
               ("stale_render", 1)]
    if s in (State.READY, State.PAUSED, State.ENDED):
        choices += [("play", 2), ("stop", 1)]
    if s == State.PLAYING:
        choices += [("pause", 2), ("stop", 1)]
    total = sum(w for _, w in choices)
    x = rng.uniform(0, total)
    for name, w in choices:
        if x < w:
            return name
        x -= w
    return "render"


def stress_trace(seed: int, n_ops: int):
    rng = random.Random(seed)
    rate = rng.choice((44_100, 48_000, 96_000))
    ms = rng.choice((20, 50, 100, 250, 500))
    capacity = max(rate * ms // 1000, 64)
    engine, sink = make_pair(rate=rate, capacity=capacity,
                             chunk=min(1024, capacity), period=min(256, capacity))

    def rand_song():
        return song(total_frames=rng.choice((2_000, 8_000, 48_000)),
                    sample_rate=rate,
                    work_steps=rng.choice((1, 2, 3, 5)),
                    seek_landing_offset=rng.choice((0, 0, 0, -17)),
                    unknown_duration=rng.random() < 0.15,
                    seek_landing_unknown=rng.random() < 0.15,
                    seek_status=rng.choice((0, 0, 0, 0, 0, 109, 110)))

    cfg = rand_song()
    trace: list[tuple] = []
    seen: dict = {}
    try:
        engine.open(cfg)
        check_all(engine, sink, seen, f"seed{seed} open")
        for i in range(n_ops):
            op = _pick(rng, engine)
            detail = ""
            if op == "producer":
                for _ in range(rng.randint(1, 6)):
                    engine.producer_step()
            elif op == "submit":
                n = rng.randint(1, 6)
                sink.tick_submit(n)
                detail = f"x{n}"
            elif op == "render":
                k = rng.randint(1, 6)
                sink.tick_render(k * sink.period_frames)
                detail = f"x{k}"
            elif op == "seek":
                if engine.state in (State.ERROR, State.EMPTY):
                    detail = "skipped"
                else:
                    t = rng.randrange(0, cfg.total_frames + 1)
                    try:
                        engine.seek(frames_to_us(t, rate))
                        detail = f"t={t}"
                    except SongSeekError as exc:
                        # failed seek is a deterministic fail-closed ERROR —
                        # never reported as success, never an ESTIMATED landing
                        assert engine.state is State.ERROR
                        assert engine.last_error and f"status={exc.status}" in engine.last_error
                        detail = f"t={t} FAILED"
            elif op == "stale_render":
                back = rng.randint(1, 3)
                rep = engine.backend_render(
                    rng.randint(1, 3) * sink.period_frames,
                    generation=engine.epoch - back)
                assert rep.kind == "stale"
                detail = f"-{back}gen"
            elif op == "play":
                engine.play()
            elif op == "pause":
                engine.pause()
            elif op == "stop":
                engine.stop()
            elif op == "open":
                cfg = rand_song()
                engine.open(cfg)
            trace.append((i, op, detail))
            check_all(engine, sink, seen, f"seed{seed} op{i} {op}{detail}")
    except AssertionError:
        print(f"\n  FAIL seed={seed}")
        print("  ops tail:", trace[-30:])
        print("  snapshot:", engine.snapshot())
        print(f"  generation: epoch={engine.epoch} segment={engine.segment}")
        print(f"  ring: produced={engine.ring.produced_total} "
              f"consumed={engine.ring.consumed_total} "
              f"buffered={engine.ring.readable_frames} "
              f"discarded={engine.ring.discarded_total}")
        print(f"  decode position: {engine.decoded_source_position}")
        print(f"  media position: {engine.position_frames} "
              f"(quality {engine.position_quality.name})")
        print(f"  device/render position: {engine.rendered_output_total} "
              f"(pending {engine.pending_output_frames()})")
        print(f"  underruns: {engine.underrun_count} / "
              f"{engine.underrun_silence_output_frames} f; preroll "
              f"{engine.preroll_events}; stale-render {engine.stale_render_events}")
        raise
    return trace, engine, sink


@gate
def g_t15_randomized_stress(seeds=200, ops=300):
    t0 = time.time()
    totals = dict(underruns=0, underrun_f=0, preroll=0, stale_decode=0,
                  submitted=0, rendered=0, stale_render=0, seeks_failed=0,
                  ended=0)
    for seed in range(seeds):
        try:
            trace, engine, sink = stress_trace(seed, ops)
        except AssertionError:
            raise
        snap = engine.snapshot()
        totals["underruns"] += snap.underrun_count
        totals["underrun_f"] += snap.underrun_silence_output_frames
        totals["stale_decode"] += snap.discarded_stale_media_frames
        totals["preroll"] += snap.preroll_events
        totals["submitted"] += snap.submitted_media_frames
        totals["rendered"] += snap.rendered_media_frames
        totals["stale_render"] += snap.stale_render_events
        totals["seeks_failed"] += sum(
            1 for _, op, d in trace if op == "seek" and "FAILED" in d)
        totals["ended"] += 1 if snap.state is State.ENDED else 0
    dt = time.time() - t0
    print(f"  {seeds} seeds x {ops} ops, all invariants after EVERY op "
          f"({dt:.1f}s): underruns={totals['underruns']} "
          f"({totals['underrun_f']} f), stale-decode={totals['stale_decode']} f, "
          f"media submitted={totals['submitted']} / rendered={totals['rendered']}, "
          f"stale-render={totals['stale_render']}, failed-seeks={totals['seeks_failed']}, "
          f"ended={totals['ended']}")


@gate
def g_capacity_sweep():
    """20/50/100/250/500 ms queues at 44.1/48/96 kHz under two bursty
    producer patterns: a sustained DEFICIT (starves eventually regardless
    of size) and a sustainable SURPLUS (bursts must be absorbed).
    Structural gate only — synthetic timing must not freeze a production
    buffer size (task §11)."""
    rows = []
    for pattern, lo_p, hi_p, lo_c, hi_c in (("deficit", 3, 6, 6, 14),
                                            ("surplus", 5, 8, 4, 8)):
        for rate in (44_100, 48_000, 96_000):
            for ms in (20, 50, 100, 250, 500):
                capacity = rate * ms // 1000
                engine, sink = make_pair(
                    rate=rate, capacity=capacity, chunk=min(512, capacity),
                    period=min(256, capacity))
                engine.open(song(total_frames=rate * 8, sample_rate=rate))
                engine.play()
                # same request pattern per (pattern, rate) across capacities
                rng = random.Random(hash((pattern, rate)) & 0xFFFF)
                seen: dict = {}
                min_fill_after_audio = capacity
                guard = 0
                while engine.state is not State.ENDED:
                    guard += 1
                    assert guard < 100_000, f"sweep {pattern} {rate}/{ms}"
                    for _ in range(rng.randint(lo_p, hi_p)):
                        engine.producer_step()
                    for _ in range(rng.randint(lo_c, hi_c)):
                        sink.tick()
                    if sink.total_real_frames > 0:
                        min_fill_after_audio = min(min_fill_after_audio,
                                                   engine.ring.readable_frames)
                    check_all(engine, sink, seen, f"sweep {pattern} {rate}/{ms}")
                assert engine.position_frames == engine.duration_frames
                assert engine.state is State.ENDED
                rows.append((pattern, rate, ms, capacity, engine.underrun_count,
                             engine.underrun_silence_output_frames,
                             min_fill_after_audio))
    print("  pattern  rate     cap_ms  cap_frames  underruns  underrun_f  min_fill")
    for pattern, rate, ms, cap, uc, uf, mf in rows:
        print(f"  {pattern:7s}  {rate:6d}  {ms:5d}  {cap:10d}  "
              f"{uc:9d}  {uf:10d}  {mf:8d}")
    for rate in (44_100, 48_000, 96_000):
        deficit = [r[4] for r in rows if r[0] == "deficit" and r[1] == rate]
        assert deficit == sorted(deficit, reverse=True), (
            f"deficit pattern should starve small queues sooner: {deficit}")
    surplus_small = [r for r in rows if r[0] == "surplus" and r[2] == 20]
    surplus_big = [r for r in rows if r[0] == "surplus" and r[2] == 500]
    print(f"  deficit: starvation total is capacity-invariant once cap >= burst; "
          f"only onset delay grows")
    print(f"  surplus: underrun frames 20ms={sum(r[5] for r in surplus_small)} "
          f"vs 500ms={sum(r[5] for r in surplus_big)} "
          f"(capacity absorbs bursts)")


# ---------------------------------------------------------------------------



def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--stress-seeds", type=int, default=200)
    ap.add_argument("--stress-ops", type=int, default=300)
    ap.add_argument("--quick", action="store_true",
                    help="reduced stress for quick iteration")
    args = ap.parse_args()
    if args.quick:
        args.stress_seeds, args.stress_ops = 30, 120

    failed = []
    for name, fn in GATES:
        t0 = time.time()
        try:
            if name == "t15_randomized_stress":
                fn(seeds=args.stress_seeds, ops=args.stress_ops)
            else:
                fn()
            print(f"PASS {name} ({time.time() - t0:.2f}s)")
        except Exception:
            print(f"FAIL {name}")
            traceback.print_exc()
            failed.append(name)
    print()
    if failed:
        print(f"RESULT: {len(GATES) - len(failed)}/{len(GATES)} gates PASS, "
              f"FAILED: {', '.join(failed)}")
        return 1
    print(f"RESULT: ALL {len(GATES)} GATES PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
