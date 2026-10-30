#!/usr/bin/env python3
"""g4-evidence-selfcheck.py — re-hash every evidence path recorded in stage.json.

Usage: g4-evidence-selfcheck.py <stage.json> <out.txt>
Exit 0 when every recorded sha256 still matches the file on disk.
"""
import hashlib
import json
import sys
from pathlib import Path


def sha256_file(p: Path) -> str:
    h = hashlib.sha256()
    with p.open("rb") as f:
        for b in iter(lambda: f.read(1 << 20), b""):
            h.update(b)
    return h.hexdigest()


stage = json.loads(Path(sys.argv[1]).read_text())
out = Path(sys.argv[2])
lines, bad = [], 0
for path, want in zip(stage["evidence_paths"], stage["evidence_sha256"]):
    p = Path(path)
    if not p.is_file():
        lines.append(f"MISSING {path}")
        bad += 1
        continue
    got = sha256_file(p)
    if got != want:
        lines.append(f"MISMATCH {path} recorded={want} actual={got}")
        bad += 1
lines.append(f"evidence_entries={len(stage['evidence_paths'])} mismatches={bad}")
out.write_text("\n".join(lines) + "\n")
print("\n".join(lines[-5:]))
sys.exit(1 if bad else 0)
