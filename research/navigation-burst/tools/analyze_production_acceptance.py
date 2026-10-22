#!/usr/bin/env python3
"""PR #311 production-acceptance analysis (burst runs on the real host).

Two independent oracles per scenario, both derived from the ConPTY
driver's evidence files (never from app-internal state):

1. COMMITTED REPLACEMENTS — value CHANGES of the shell's own
   `Track: n/N` line across the timestamped reconstructed frames
   (same witness class as the campaign's analyze_markers.py: frames
   are emitted only when the grid changes, so a repeated value does
   not recount; a polluted frame fails the regex and only delays).

2. DEVICE OPENS — DISTINCT `ts_us` values of the off-by-default
   `[qianqian-wasapi] … opened:` mechanism lines. The same line is
   carried by several reconstructed frames, so occurrences would
   double-count; the device-open timestamp is per real open.

Expected values are the PR #311 acceptance table (per burst: at most
ONE replacement, ZERO when the burst nets no displacement), plus the
synchronized walk rows (one commit per spaced step) and the two
unchanged-mechanism rows.
"""

import re
import sys
from pathlib import Path

EVIDENCE = Path(__file__).resolve().parent.parent / "evidence"

# scenario -> (expected burst opens, expected final Track, keyed?)
# burst opens counts opens AFTER the startup open (the first open is
# the argv-seeded episode, before any key).
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


def main(prefix: str) -> int:
    fail = 0
    print(f"PR311 production acceptance — run {prefix}")
    print("scenario          burst_opens(exp) final_track(exp) track_changes device_opens verdict")
    for name, (exp_opens, exp_track) in EXPECTED.items():
        f = EVIDENCE / f"run{prefix}-{name}.txt"
        m = EVIDENCE / f"run{prefix}-{name}.markers.txt"
        if not f.exists() or not m.exists():
            print(f"{name:17} MISSING EVIDENCE FILE ({f.name} / {m.name})")
            fail += 1
            continue
        text = f.read_text(errors="replace")
        changes, last = track_changes(text)
        # Device opens from the MARKER stream (one line per event; the
        # frame transcripts interleave VT pollution that splits the
        # line). The same marker is carried by several reconstructed
        # frames, so dedupe by the per-open ts_us.
        seen = []
        for om in OPENED_RE.finditer(m.read_text(errors="replace")):
            if om.group(1) not in seen:
                seen.append(om.group(1))
        burst_opens = max(0, len(seen) - 1)  # minus the startup open
        ok = burst_opens == exp_opens and last == exp_track
        fail += 0 if ok else 1
        print(
            f"{name:17} {burst_opens} ({exp_opens})            "
            f"{last or '-':>4} ({exp_track})      {changes:2d}            {len(seen):2d}          "
            f"{'GREEN' if ok else 'RED'}"
        )
    print("verdict:", "ALL_GREEN" if fail == 0 else f"{fail} RED")
    return 1 if fail else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1] if len(sys.argv) > 1 else "P2"))
