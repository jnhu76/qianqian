#!/usr/bin/env python3
"""probe-version-bytes.py — prove the static diff is confined to the
FFMPEG_VERSION string inside the four version.c.o members.

Usage: probe-version-bytes.py <new.a> <old.a> <out.txt>
"""
import hashlib
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from importlib import util
spec = util.spec_from_file_location("arcmp", Path(__file__).resolve().parent / "g4-ar-cmp.py")
arcmp = util.module_from_spec(spec)
spec.loader.exec_module(arcmp)

new_path, old_path, out_path = Path(sys.argv[1]), Path(sys.argv[2]), sys.argv[3]
a = arcmp.parse_ar(new_path)
b = arcmp.parse_ar(old_path)

lines = []
for i, (ma, mb) in enumerate(zip(a, b)):
    if ma["sha256"] == mb["sha256"]:
        continue
    ba, bb = ma["body"], mb["body"]
    assert len(ba) == len(bb), (i, len(ba), len(bb))
    diffs = [k for k in range(len(ba)) if ba[k] != bb[k]]
    # every occurrence of the version literal in each body (member 17 carries
    # it twice: once in the configure-args config string, once as
    # av_util_ffversion)
    def windows(body, needle=b"qianqian-v0.1.0-", width=32):
        out, start = [], 0
        while True:
            s = body.find(needle, start)
            if s < 0:
                return out
            out.append((s, s + width))
            start = s + 1
    wa, wb = windows(ba), windows(bb)
    in_window = all(any(lo <= k < hi for lo, hi in wa) for k in diffs)
    lines.append(f"member[{i}] name={ma['name']} size={len(ba)} "
                 f"differing_bytes={len(diffs)} "
                 f"version_occurrences_a={len(wa)} version_occurrences_b={len(wb)} "
                 f"all_diffs_inside_version_literals={in_window}")
    if diffs:
        lo, hi = min(diffs), max(diffs)
        lines.append(f"  diff_range={lo}..{hi}")
        lines.append(f"  bytes_a={ba[lo:hi+1]!r}")
        lines.append(f"  bytes_b={bb[lo:hi+1]!r}")
        for j, (s, _e) in enumerate(wa):
            lines.append(f"  literal_a[{j}]={ba[s:s+32]!r}")
            lines.append(f"  literal_b[{j}]={bb[s:s+32]!r}")
Path(out_path).write_text("\n".join(lines) + "\n")
print("\n".join(lines))
