#!/usr/bin/env python3
"""model_equivalence.py — Python ↔ native PlayerEngine semantic equivalence.

The PRIMARY Phase-1.5 acceptance gate (docs/player-engine.md, task §44–§50):
one deterministic, seeded trace generator drives BOTH engines —

    Python oracle   tools/player_model/player_model.py   (the authority)
    native engine   src/player/ + tests/player/trace_runner

— and the observable snapshot streams are compared field-by-field after
EVERY operation. Exit code 0 = all seeds equivalent.

  python3 tools/player_model/model_equivalence.py                 # 200 x 300
  python3 tools/player_model/model_equivalence.py --seeds 1000 --ops 500  # heavy
  python3 tools/player_model/model_equivalence.py --seeds 4 --ops 40      # debug

Harness-level normalizations (never oracle rewrites; task §45):
  * decoded_source_position — the frozen SongCore ABI has no position tell,
    so the native engine reports its ESTIMATE (segment landing + frames
    taken since the commit). Both sides emit that estimate; the fake's
    hidden truth is compared separately in the fake_pos field.
  * last_error — the real ABI exposes no open()/reopen() diagnostics (no
    handle exists), so native open-failure texts carry a status code where
    the oracle carries the fake's message. Compared by category.
  * remaining hint / EOF fold — applied inside the native trace runner so
    the native decision sequence matches the oracle 1:1 (see its header).

Generator constraint (documented oracle hole, NOT a semantic change): seek
targets are chosen so the fake's landing stays < total_frames. Landing
exactly at EOF leaves the oracle's producer_step calling read_pcm(0), which
the fake rejects — an unreachable-by-chance edge in the random stress
(~0.4%/200 seeds), verified separately. The native engine handles the real
shape (SONG_EOF poll) natively.
"""
from __future__ import annotations

import argparse
import random
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from fake_decoder import (FakeSongConfig, frames_to_us,  # noqa: E402
                          us_to_frames)
from fake_sink import FakeSink  # noqa: E402
from player_model import (EngineConfig, EngineError, LandingQuality,  # noqa: E402
                          PlayerEngine, SongSeekError, State)
from scenarios import check_all  # noqa: E402

# SNAP field names in emission order (must match tests/player/trace_runner.cpp).
FIELDS = [
    "idx", "op", "outcome", "state", "media_position_frames", "position_quality",
    "decoded_source_position", "fake_position", "duration_frames",
    "duration_known", "queued_media_frames", "capacity_frames", "epoch",
    "segment", "source_eof", "underrun_count",
    "underrun_silence_output_frames", "preroll_events",
    "preroll_silence_output_frames", "eos_silence_output_frames",
    "decoded_source_frames", "submitted_output_frames", "rendered_output_frames",
    "pending_output_frames", "discarded_output_frames",
    "submitted_media_frames", "rendered_media_frames",
    "rendered_gap_output_frames", "pending_media_frames",
    "discarded_output_media_frames", "discarded_stale_media_frames",
    "stale_render_events", "last_error",
]


def fake_landing(cfg: FakeSongConfig, position_us: int) -> int:
    """Mirror of FakeSongCore.seek's clamp math (generator-side check only)."""
    rate = cfg.sample_rate
    target = max(us_to_frames(position_us, rate), 0)
    if not cfg.unknown_duration:
        target = min(target, cfg.total_frames)
    landing = max(target + cfg.seek_landing_offset, 0)
    if not cfg.unknown_duration:
        landing = min(landing, cfg.total_frames)
    return landing


def error_category(sanitized: str) -> str:
    """Compare last_error by category: the real ABI has no open/reopen
    diagnostics, so native open failures carry a status where the oracle
    carries the fake's message text."""
    if not sanitized:
        return "none"
    if sanitized.startswith("open_failed"):
        return "open-failed"
    if sanitized == "simulated_song_open_failure":
        # The oracle stores the bare FakeOpenError text for open failures.
        return "open-failed"
    if sanitized.startswith("stop_recovery_failed"):
        return "stop-recovery-failed"
    if sanitized == "simulated_reopen_failure":
        return "stop-recovery-failed"
    for prefix in ("seek_failure__status=", "restart_seek_failure__status=",
                   "decode_failure__status="):
        if sanitized.startswith(prefix):
            return prefix + sanitized[len(prefix):]
    raise AssertionError(f"unclassified last_error: {sanitized!r}")


@dataclass
class GenResult:
    trace: list[str]
    snaps: list[str]
    op_lines: list[str]


def generate_and_run_oracle(seed: int, n_ops: int) -> GenResult:
    """Fork of scenarios.stress_trace that EMITS the trace while running the
    oracle, then formats one oracle SNAP line per op."""
    rng = random.Random(seed)
    rate = rng.choice((44_100, 48_000, 96_000))
    ms = rng.choice((20, 50, 100, 250, 500))
    capacity = max(rate * ms // 1000, 64)
    chunk = min(1024, capacity)
    period = min(256, capacity)
    engine = PlayerEngine(EngineConfig(capacity_frames=capacity, device_rate=rate,
                                       read_chunk_frames=chunk))
    sink = FakeSink(engine, period)

    next_song_id = [0]
    trace: list[str] = [f"TRACE 1",
                        f"CFG {capacity} {chunk} 4096 {period}"]
    snaps: list[str] = []
    op_lines: list[str] = []
    seen: dict = {}
    # decoded-position estimate bookkeeping (mirror of the native formula).
    est_sum = 0
    last_segment = engine.segment

    def rand_song() -> tuple[int, FakeSongConfig]:
        cfg = FakeSongConfig(
            total_frames=rng.choice((2_000, 8_000, 48_000)),
            sample_rate=rate, channels=2,
            work_steps=rng.choice((1, 2, 3, 5)),
            seek_landing_offset=rng.choice((0, 0, 0, -17)),
            unknown_duration=rng.random() < 0.15,
            seek_landing_unknown=rng.random() < 0.15,
            seek_status=rng.choice((0, 0, 0, 0, 0, 109, 110)),
            open_fails=rng.random() < 0.02,
            fail_at_frame=(rng.randrange(1, 2_000) if rng.random() < 0.10
                           else None),
        )
        next_song_id[0] += 1
        return next_song_id[0], cfg

    def song_line(sid: int, cfg: FakeSongConfig) -> str:
        return (f"SONG {sid} {cfg.total_frames} {cfg.sample_rate} "
                f"{cfg.channels} {cfg.work_steps} {cfg.seek_landing_offset} "
                f"{cfg.fail_at_frame if cfg.fail_at_frame is not None else -1} "
                f"{1 if cfg.open_fails else 0} {cfg.seek_status} "
                f"{1 if cfg.seek_landing_unknown else 0} "
                f"{1 if cfg.unknown_duration else 0} "
                f"{1 if cfg.reopen_fails else 0}")

    def emit(idx: int, op: str, outcome: str) -> None:
        nonlocal est_sum, last_segment
        # Normalize to the trace vocabulary (task §45: normalize at the
        # harness, never rewrite the oracle).
        op = {"producer": "decode", "stale_render": "render"}.get(op, op)
        if engine.segment != last_segment:
            last_segment = engine.segment
            est_sum = 0
        s = engine.snapshot()
        fake_pos = engine.decoder.position if engine.decoder else -1
        est = engine.segment_start[engine.segment] + est_sum \
            if engine.decoder is not None else 0
        err = (s.last_error or "").replace(" ", "_").replace(",", "_")
        vals = [idx, op, outcome, s.state.name, s.media_position_frames,
                s.position_quality.name, est, fake_pos, s.duration_frames,
                int(s.duration_known), s.queued_media_frames,
                s.capacity_frames, s.epoch, s.segment, int(s.source_eof),
                s.underrun_count, s.underrun_silence_output_frames,
                s.preroll_events, s.preroll_silence_output_frames,
                s.eos_silence_output_frames, s.decoded_source_frames,
                s.submitted_output_frames, s.rendered_output_frames,
                s.pending_output_frames, s.discarded_output_frames,
                s.submitted_media_frames, s.rendered_media_frames,
                s.rendered_gap_output_frames, s.pending_media_frames,
                s.discarded_output_media_frames,
                s.discarded_stale_media_frames, s.stale_render_events, err]
        snaps.append(" ".join(str(v) for v in vals))

    def pick() -> str:
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

    sid, cfg = rand_song()
    trace.append(song_line(sid, cfg))
    trace.append(f"OP open {sid}")
    op_lines.append(f"OP open {sid}")
    outcome0 = "ok"
    try:
        engine.open(cfg)
    except EngineError:
        outcome0 = "openfail"
    emit(0, "open", outcome0)
    check_all(engine, sink, seen, f"seed{seed} open")

    for i in range(1, n_ops + 1):
        op = pick()
        outcome = "ok"
        if op == "producer":
            n = rng.randint(1, 6)
            trace.append(f"OP decode {n}")
            op_lines.append(f"OP decode {n}")
            for _ in range(n):
                rep = engine.producer_step()
                if rep.outcome == "BEGIN":
                    est_sum += rep.frames
        elif op == "submit":
            n = rng.randint(1, 6)
            trace.append(f"OP submit {n}")
            op_lines.append(f"OP submit {n}")
            sink.tick_submit(n)
        elif op == "render":
            k = rng.randint(1, 6)
            trace.append(f"OP render {k} 0")
            op_lines.append(f"OP render {k} 0")
            sink.tick_render(k * period)
        elif op == "stale_render":
            k = rng.randint(1, 3)
            back = rng.randint(1, 3)
            trace.append(f"OP render {k} {back}")
            op_lines.append(f"OP render {k} {back}")
            rep = engine.backend_render(k * period, generation=engine.epoch - back)
            assert rep.kind == "stale"
        elif op == "seek":
            if engine.state in (State.ERROR, State.EMPTY):
                # Stress rule: seek is skipped in ERROR/EMPTY. Substitute a
                # documented no-op (pause) so both sides stay op-aligned.
                op = "pause"
                trace.append("OP pause")
                op_lines.append("OP pause")
                engine.pause()
            else:
                # Generator constraint: keep the fake's landing < total (the
                # read_pcm(0) oracle hole; see module docstring).
                while True:
                    t = rng.randrange(0, cfg.total_frames + 1)
                    if fake_landing(cfg, frames_to_us(t, rate)) < cfg.total_frames:
                        break
                us = frames_to_us(t, rate)
                trace.append(f"OP seek {us}")
                op_lines.append(f"OP seek {us}")
                try:
                    engine.seek(us)
                except SongSeekError:
                    outcome = "seekfail"
        elif op == "play":
            trace.append("OP play")
            op_lines.append("OP play")
            try:
                engine.play()
            except SongSeekError:
                outcome = "seekfail"
            except EngineError:
                outcome = "illegal"
        elif op == "pause":
            trace.append("OP pause")
            op_lines.append("OP pause")
            engine.pause()
        elif op == "stop":
            trace.append("OP stop")
            op_lines.append("OP stop")
            engine.stop()
        elif op == "open":
            sid, cfg = rand_song()
            trace.append(song_line(sid, cfg))
            line = f"OP open {sid}"
            trace.append(line)
            op_lines.append(line)
            try:
                engine.open(cfg)
            except EngineError:
                outcome = "openfail"
        emit(i, op, outcome)
        check_all(engine, sink, seen, f"seed{seed} op{i} {op}")

    return GenResult(trace=trace, snaps=snaps, op_lines=op_lines)


def run_native(runner: str, trace_lines: list[str]) -> list[str]:
    text = "\n".join(trace_lines) + "\n"
    proc = subprocess.run([runner], input=text, capture_output=True, text=True,
                          timeout=120)
    if proc.returncode != 0:
        raise RuntimeError(f"native runner exit {proc.returncode}\n"
                           f"stderr tail: {proc.stderr[-2000:]}")
    snaps = [ln for ln in proc.stdout.splitlines() if ln.startswith("SNAP ")]
    return [ln[len("SNAP "):] for ln in snaps]


def compare(py: str, native: str) -> str | None:
    """First differing field, or None. last_error compares by category."""
    a = py.split()
    b = native.split()
    if len(a) != len(b):
        return f"field-count {len(a)} != {len(b)}"
    for name, va, vb in zip(FIELDS, a, b):
        if name == "last_error":
            ca, cb = error_category(va), error_category(vb)
            if ca != cb:
                return f"last_error: {ca!r} != {cb!r} ({va!r} vs {vb!r})"
            continue
        if va != vb:
            return f"{name}: python={va} native={vb}"
    return None


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--seeds", type=int, default=200)
    ap.add_argument("--ops", type=int, default=300)
    ap.add_argument("--runner", default="build/artifacts/player_trace_runner",
                    help="path to the native trace runner binary")
    args = ap.parse_args()

    runner = str(Path(args.runner).resolve())
    if not Path(runner).exists():
        print(f"native runner not found: {runner} (build it: xmake build player_trace_runner)")
        return 2

    for seed in range(args.seeds):
        gen = generate_and_run_oracle(seed, args.ops)
        try:
            native = run_native(runner, gen.trace)
        except RuntimeError as exc:
            print(f"FAIL seed={seed}: {exc}")
            _dump_trace(gen.trace)
            return 1
        if len(native) != len(gen.snaps):
            print(f"FAIL seed={seed}: snapshot count python={len(gen.snaps)} "
                  f"native={len(native)}")
            _dump_trace(gen.trace)
            return 1
        for i, (p, n) in enumerate(zip(gen.snaps, native)):
            diff = compare(p, n)
            if diff is not None:
                print(f"FAIL seed={seed} op={i} first-diff: {diff}")
                print(f"  op line   : {gen.op_lines[i] if i < len(gen.op_lines) else '?'}")
                print(f"  python    : SNAP {p}")
                print(f"  native    : SNAP {n}")
                _dump_trace(gen.trace)
                return 1
        if seed % 25 == 0 or seed == args.seeds - 1:
            print(f"  seed {seed}: {args.ops} ops equivalent")
    print(f"EQUIVALENCE PASS: {args.seeds} seeds x {args.ops} ops, "
          f"all observables equal after every op")
    return 0


def _dump_trace(trace: list[str]) -> None:
    with tempfile.NamedTemporaryFile("w", suffix=".trace", prefix="qn-player-",
                                      delete=False, dir="/tmp") as f:
        f.write("\n".join(trace) + "\n")
    print(f"  trace saved: {f.name}")


if __name__ == "__main__":
    raise SystemExit(main())
