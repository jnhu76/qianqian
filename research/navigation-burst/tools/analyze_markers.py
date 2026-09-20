#!/usr/bin/env python3
"""Navigation-burst evidence analysis (research/navigation-burst-boundary-0).

Committed-replacement witness: the TUI's own `Track: n/N` line in the
timestamped reconstructed frames (scanned per frame — its row index
shifts when the diagnostics pane gains/loses the Terminal line). Frames
are emitted only when the grid changes, and re-emitted content repeats
the same value, so counting VALUE CHANGES counts committed D14.6
replacements; a mid-write polluted frame fails the regex and is
skipped (delays, never invents). The per-commit timestamps give the
serialized replacement periods.
"""

import re
import sys
from pathlib import Path

EVIDENCE = Path(__file__).resolve().parent.parent / "evidence"

# scenario -> (expected replacements, final Track value, burst key count)
CASES = {
    "B-single-N":      (1, "Track: 2/8", 1),
    "B-nnnnn-10ms":    (5, "Track: 6/8", 5),
    "B-nnnnn-50ms":    (5, "Track: 6/8", 5),
    "B-nnnnn-150ms":   (5, "Track: 6/8", 5),
    "B-nnnnn-250ms":   (5, "Track: 6/8", 5),
    "B-pppp-10ms":     (8, "Track: 1/8", 4),
    "B-nnnppnnp-10ms": (8, "Track: 3/8", 8),
    "B-eof-natural":   (1, "Track: 2/2", 0),
    "B-manual-short":  (1, "Track: 2/2", 1),
}

TRACK_RE = re.compile(r"Track: (\d+)/(\d+)")


def frames(path: Path):
    current_t, rows = None, []
    for line in path.read_text(errors="replace").splitlines():
        m = re.match(r"\[\+([\d.]+)s?\]", line)
        if m:
            if current_t is not None:
                yield current_t, rows
            current_t, rows = float(m.group(1)), []
            continue
        rows.append(line)
    if current_t is not None:
        yield current_t, rows


def analyze(name: str, run: str):
    exp_repls, exp_final, burst_keys = CASES[name]
    path = EVIDENCE / f"{run}-{name}.txt"
    markers = EVIDENCE / f"{run}-{name}.markers.txt"
    key_times = [float(m) for m in re.findall(r"\[\+([\d.]+)\] KEY", markers.read_text(errors="replace"))]
    # Every scenario ends with the quit key; it is not part of the burst.
    key_times = key_times[:-1] if key_times else []
    if burst_keys and len(key_times) >= burst_keys:
        t_burst = key_times[-burst_keys]
    elif burst_keys == 0:
        t_burst = 0.0
    else:
        t_burst = float("nan")

    commits = []  # (time, "Track: n/N") at every value change
    prev = None
    t_final = float("nan")
    for t, rows in frames(path):
        m = None
        for r in rows:
            m = TRACK_RE.search(r)
            if m:
                break
        if not m:
            continue
        value = m.group(0)
        if value == prev:
            continue
        commits.append((t, value))
        prev = value
        if value == exp_final and t_final != t_final and t >= t_burst:
            t_final = t
    # The first commit entry is the startup episode's first render.
    replacements = max(len(commits) - 1, 0)
    periods = [
        (b - a) * 1000 for (a, _), (b, _) in zip(commits[2:], commits[3:])
    ]  # replacement-to-replacement periods after the startup frame
    return replacements, t_burst, t_final, commits, periods


def main() -> int:
    run = sys.argv[1] if len(sys.argv) > 1 else "run2"
    print(f"== {run} ==")
    print(f"{'scenario':<18} {'repls':>5} {'exp':>4} {'T_burst0':>9} {'T_final':>8} {'total_ms':>9}  commit_periods_ms")
    failures = []
    for name in CASES:
        exp_repls, _f, burst_keys = CASES[name]
        repls, t_burst, t_final, commits, periods = analyze(name, run)
        total = (t_final - t_burst) * 1000 if t_final == t_final and t_burst == t_burst else float("nan")
        ok = repls == exp_repls and (t_final == t_final or burst_keys == 0)
        if not ok:
            failures.append(name)
        tb = f"{t_burst:9.3f}" if t_burst == t_burst else "      nan"
        tf = f"{t_final:8.3f}" if t_final == t_final else "     nan"
        tm = f"{total:9.1f}" if total == total else "      nan"
        ps = " ".join(f"{p:.0f}" for p in periods)
        print(f"{name:<18} {repls:>5} {exp_repls:>4} {tb} {tf} {tm}  [{ps}]"
              + ("" if ok else "  <-- MISMATCH"))
    if failures:
        print(f"MISMATCHED: {failures}")
        return 1
    print("ALL REPLACEMENT-COUNT EXPECTATIONS MET")
    return 0


if __name__ == "__main__":
    sys.exit(main())
