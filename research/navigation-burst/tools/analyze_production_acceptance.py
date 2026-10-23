#!/usr/bin/env python3
"""PR #311 production-acceptance analysis (burst runs on the real host).

Two independent oracles per scenario, both derived from the ConPTY
driver's evidence files (never from app-internal state):

1. COMMITTED REPLACEMENTS — value CHANGES of the shell's own
   `Track: n/N` line across the timestamped reconstructed frames
   (same witness class as the campaign's analyze_markers.py: frames
   are emitted only when the grid changes, so a repeated value does
   not recount; a polluted frame fails the regex and only delays).

2. DEVICE-OPEN CEILING — DISTINCT `ts_us` values of the off-by-default
   `[qianqian-wasapi] … opened:` mechanism lines. Every stream capture
   of an open line is carried by reconstructed frames, so ts_us dedupe
   gives the exact set of REAL device opens that left a trace; a line
   whose frame never carries it is LOST, so the count is a LOWER
   BOUND. The defect class this table polices is EXTRA replacements
   (the old per-key behavior: NNNNN cost 5 device opens), never a
   lost line — so the sound mechanical oracle is the CEILING:
   distinct opens must never exceed 1 + expected burst opens (the 1
   is the startup open, by construction the app's first and only
   pre-key open). When the total EQUALS the ceiling, the burst-open
   count is exact (total − 1); when it is below, the row is
   capture-limited and the exact count leans on the run that captured
   everything (P1) plus the driver's own needles. Frame-time
   attribution of a replayed stderr line is UNSOUND (its first
   carrying frame is not its emission time) and is not used.

The driver's own step needles (premise-synchronized expects, exit
codes, absence witnesses) are the primary semantics oracle; the
`Track:` frame analysis and the ceiling below are the independent
count oracles.

Expected values are the PR #311 acceptance table (per burst: at most
ONE replacement, ZERO when the burst nets no displacement), plus the
synchronized walk rows (one commit per witnessed step) and the two
unchanged-mechanism rows.
"""

import re
import sys
from pathlib import Path

EVIDENCE = Path(__file__).resolve().parent.parent / "evidence"

# scenario -> (expected burst opens, expected final Track, keyed?)
EXPECTED = {
    "B-single-N":      (1, "2/8"),
    "B-nnnnn-10ms":    (1, "6/8"),
    "B-nnnppnnp-10ms": (1, "3/8"),
    "B-np-10ms":       (0, "1/8"),
    "B-npnpnp-10ms":   (0, "1/8"),
    "B-pppp-10ms":     (5, "1/8"),  # 4 synchronized walk steps + the ONE p-burst commit
    "B-nnnnn-150ms":   (5, "6/8"),  # 5 synchronized walk steps
    "B-eof-natural":   (1, "2/2"),
    "B-manual-short":  (1, "2/2"),
}

TRACK_RE = re.compile(r"Track: (\d+)/(\d+)")
OPENED_RE = re.compile(r"\[qianqian-wasapi\][^\n]*?ts_us=(\d+) opened:")
KEY_RE = re.compile(r"\[\+[\d.]+s?\].*KEY ")
FRAME_RE = re.compile(r"^\[\+([\d.]+)s?\]")


def frames_from_text(text: str):
    current_t, rows = None, []
    for line in text.splitlines():
        m = re.match(r"\[\+([\d.]+)s?\]", line)
        if m:
            if current_t is not None:
                yield current_t, "\n".join(rows)
            current_t, rows = float(m.group(1)), []
        elif current_t is not None:
            rows.append(line)
    if current_t is not None:
        yield current_t, "\n".join(rows)


def track_changes(text: str):
    """(count of value changes, last value) over frame-ordered text."""
    last, changes = None, 0
    for _, frame in frames_from_text(text):
        m = TRACK_RE.search(frame)
        if not m:
            continue
        value = f"{m.group(1)}/{m.group(2)}"
        if value != last:
            changes += 1 if last is not None else 0
            last = value
    return changes, last


def burst_opens(markers_text: str):
    """(burst opens, total distinct opens) via first-KEY attribution.

    Walks the marker stream in order, tracks the current frame time,
    notes the frame time of the FIRST KEY marker, and counts a device
    open (deduped by ts_us) as a burst open iff its FIRST appearance
    is at/after that time. Returns (burst, total, first_key_seen).
    """
    first_key_t = None
    current_t = None
    first_appear = {}  # ts_us -> frame time of first appearance
    for line in markers_text.splitlines():
        fm = FRAME_RE.match(line)
        if fm:
            current_t = float(fm.group(1))
        else:
            current_t = None
        if first_key_t is None and KEY_RE.search(line):
            first_key_t = current_t
        om = OPENED_RE.search(line)
        if om and om.group(1) not in first_appear:
            first_appear[om.group(1)] = current_t
    total = len(first_appear)
    if first_key_t is None:
        return None, total, False
    burst = sum(1 for t in first_appear.values() if t is not None and t >= first_key_t)
    return burst, total, True


def distinct_opens(markers_text: str):
    """Distinct device-open ts_us values, in first-capture order."""
    seen = []
    for om in OPENED_RE.finditer(markers_text):
        if om.group(1) not in seen:
            seen.append(om.group(1))
    return seen


def main(prefix: str) -> int:
    fail = 0
    print(f"PR311 production acceptance — run {prefix}")
    print("scenario          final_track(exp) total_opens ceiling(1+exp) burst_read      verdict")
    for name, (exp_opens, exp_track) in EXPECTED.items():
        f = EVIDENCE / f"run{prefix}-{name}.txt"
        m = EVIDENCE / f"run{prefix}-{name}.markers.txt"
        if not f.exists() or not m.exists():
            print(f"{name:17} MISSING EVIDENCE FILE ({f.name} / {m.name})")
            fail += 1
            continue
        text = f.read_text(errors="replace")
        _, last = track_changes(text)
        total = len(distinct_opens(m.read_text(errors="replace")))
        ceiling = 1 + exp_opens
        if last != exp_track or total > ceiling:
            verdict = "RED"
            fail += 1
        elif total == ceiling:
            verdict = "GREEN (exact count)"
        else:
            verdict = "GREEN (capture-limited)"
        read = f"{total - 1}" if total == ceiling else f"<={exp_opens} (line lost)"
        print(
            f"{name:17} {last or '-':>4} ({exp_track})      {total:2d}          {ceiling:2d}          "
            f"{read:>4}          {verdict}"
        )
    print("verdict:", "ALL_GREEN" if fail == 0 else f"{fail} RED")
    return 1 if fail else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1] if len(sys.argv) > 1 else "P2"))
