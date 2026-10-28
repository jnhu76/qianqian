#!/usr/bin/env python3
"""Render MATRIX.md from the executed receipts (no re-interpretation).

Reads raw/healthy.json, raw/counterfactuals.json and the per-cell logs, and
prints the Markdown table that MATRIX.md carries. The script only selects
already-recorded lines; it never decides a verdict.
"""
import json
import os
import re

EVIDENCE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
RAW = os.path.join(EVIDENCE, "raw")

PATTERNS = {
    "ffmpeg-C1": [r"SINK_HOLD_ENTERED.*", r"EXPORTER_ENTERED.*", r"QFB-C1-REGISTRATION.*", r"QFB-C1-SHUTDOWN.*"],
    "ffmpeg-C2": [r"QFB-C2-REGISTRATION.*", r"QFB-C2-SHUTDOWN.*"],
    "ffmpeg-C3": [r"QFB-CAPACITY.*", r"QFB-C3-TARGET.*", r"QFB-C3-SHUTDOWN.*"],
    "ffmpeg-C4": [r"BROKEN-STDERR-CONFIRMED.*", r"QFB-C4-COUNTERS.*", r"QFB-C4-SHUTDOWN.*"],
    "read-C1": [r"C1-PREMISE SINK_HOLD.*", r"C1-POOL.*", r"C1-SHUTDOWN.*"],
    "read-C2": [r"C2-POOL.*", r"C2-SHUTDOWN.*"],
    "read-C3": [r"C3-POOL.*", r"C3-SHUTDOWN.*"],
    "read-C4": [r"BROKEN-STDERR-CONFIRMED.*", r"C4-COUNTERS.*", r"C4-SHUTDOWN.*"],
    "seek-C1": [r"C1-PRODUCER .*", r"C1-ALPHA.*", r"C1-SHUTDOWN.*"],
    "seek-C2": [r"C2-ALPHA.*", r"C2-SHUTDOWN.*"],
    "seek-C3": [r"C3-ALPHA.*", r"C3-SHUTDOWN.*"],
    "seek-C4": [r"C4-BROKEN-STDERR-CONFIRMED.*", r"C4-FAILING.*", r"C4-SHUTDOWN.*"],
}


def read(path):
    with open(path, "r", encoding="utf-8-sig", errors="replace") as f:
        return f.read()


def main():
    healthy = {c["cell"]: c for c in json.load(open(os.path.join(RAW, "healthy.json"), encoding="utf-8"))}
    cf = {c["cell"]: c for c in json.load(open(os.path.join(RAW, "counterfactuals.json"), encoding="utf-8"))}
    order = [c for c in PATTERNS]
    print("| Arm | Cell | Healthy | Key evidence (healthy receipt) | Counterfactual | Exact RED | Restored |")
    print("|---|---|---|---|---|---|---|")
    for cell in order:
        h = healthy.get(cell)
        r = cf.get(cell)
        log = read(os.path.join(EVIDENCE, h["log"]))
        evidence = []
        for pat in PATTERNS[cell]:
            m = re.search(pat, log)
            if m:
                evidence.append(m.group(0).strip())
        arm, control = cell.rsplit("-", 1)
        arm_name = {"ffmpeg": "ffmpeg-bridge", "read": "decode-read", "seek": "decode-seek"}[arm]
        prints = " ".join("`%s`" % e.replace("|", "\\|") for e in evidence)
        red = r["red_checks"]
        red_txt = r.get("red_signature_matched") or r["red_signature"]
        print(
            "| %s | %s | %s | %s | exit %s, assertion %s, no PASS %s, timed_out %s | `%s` | %s |"
            % (
                arm_name,
                control,
                h["verdict"],
                prints,
                r["red_exit"],
                red["assertion_red"],
                red["no_pass_marker"],
                r["red_timed_out"],
                red_txt.replace("|", "\\|"),
                "GREEN" if r["restored_green"] else "FAIL",
            )
        )


if __name__ == "__main__":
    main()
