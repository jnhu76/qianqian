#!/usr/bin/env python3
"""g4-ar-cmp.py — ordered, occurrence-indexed ar comparison.

Usage: g4-ar-cmp.py <archive-a> <archive-b> <out-prefix>

Parses the GNU ar container directly (never `ar p`, which collapses duplicate
member names), then reports:
  - member count and order identity
  - per-occurrence sha256 equality (data only)
  - header field equality (mtime/uid/gid/mode)
  - for each differing occurrence: name, both sizes, both header fields, a
    short printable-string diff hint
Writes <out-prefix>.json and <out-prefix>.txt. Exit 0 always (measurement).
"""
from __future__ import annotations

import hashlib
import json
import string
import sys
from pathlib import Path

PRINTABLE = set(bytes(string.printable, "ascii"))


def parse_ar(path: Path):
    data = path.read_bytes()
    assert data[:8] == b"!<arch>\n", "not a GNU ar archive"
    off = 8
    members = []
    longnames = b""
    while off + 60 <= len(data):
        hdr = data[off:off + 60]
        raw_name = hdr[0:16].decode("latin1").rstrip()
        mtime = hdr[16:28].decode("latin1").strip()
        uid = hdr[28:34].decode("latin1").strip()
        gid = hdr[34:40].decode("latin1").strip()
        mode = hdr[40:48].decode("latin1").strip()
        size = int(hdr[48:58].decode("latin1").strip() or "0")
        assert hdr[58:60] == b"`\n", f"bad fmag at {off}"
        body = data[off + 60:off + 60 + size]
        if raw_name == "//":
            longnames = body
            name = "//"
        elif raw_name == "/":
            name = "/"
        elif raw_name.startswith("/") and raw_name[1:].isdigit():
            start = int(raw_name[1:])
            end = longnames.find(b"/\n", start)
            name = longnames[start:end].decode("latin1")
        else:
            name = raw_name.rstrip("/")
        members.append({
            "name": name,
            "header": {"mtime": mtime, "uid": uid, "gid": gid, "mode": mode},
            "size": size,
            "sha256": hashlib.sha256(body).hexdigest(),
            "body": body,
        })
        off += 60 + size + (size % 2)
    return members


def printable_runs(body: bytes, minlen: int = 6):
    out, cur = [], bytearray()
    for b in body:
        if b in PRINTABLE and b not in (10, 13):
            cur.append(b)
        else:
            if len(cur) >= minlen:
                out.append(cur.decode("latin1"))
            cur = bytearray()
    if len(cur) >= minlen:
        out.append(cur.decode("latin1"))
    return out


def main() -> int:
    a_path, b_path, prefix = Path(sys.argv[1]), Path(sys.argv[2]), sys.argv[3]
    a, b = parse_ar(a_path), parse_ar(b_path)

    names_a = [m["name"] for m in a]
    names_b = [m["name"] for m in b]
    order_identical = names_a == names_b

    diffs = []
    header_diffs = []
    for i, (ma, mb) in enumerate(zip(a, b)):
        if ma["header"] != mb["header"]:
            header_diffs.append({"index": i, "name_a": ma["name"],
                                 "name_b": mb["name"],
                                 "header_a": ma["header"], "header_b": mb["header"]})
        if ma["name"] != mb["name"] or ma["sha256"] != mb["sha256"]:
            strings_a = printable_runs(ma["body"])
            strings_b = printable_runs(mb["body"])
            only_a = [s for s in strings_a if s not in strings_b][:6]
            only_b = [s for s in strings_b if s not in strings_a][:6]
            diffs.append({
                "index": i, "name_a": ma["name"], "name_b": mb["name"],
                "size_a": ma["size"], "size_b": mb["size"],
                "sha_a": ma["sha256"], "sha_b": mb["sha256"],
                "strings_only_in_a": only_a,
                "strings_only_in_b": only_b,
            })

    result = {
        "archive_a": str(a_path),
        "archive_b": str(b_path),
        "members_a": len(a),
        "members_b": len(b),
        "member_order_identical": order_identical,
        "member_occurrences_identical": sum(
            1 for ma, mb in zip(a, b) if ma["name"] == mb["name"] and ma["sha256"] == mb["sha256"]),
        "differing_occurrences": len(diffs),
        "header_differing_occurrences": len(header_diffs),
        "header_diff_sample": header_diffs[:3],
        "diffs": diffs,
    }
    Path(prefix + ".json").write_text(json.dumps(result, indent=2))

    lines = []
    lines.append(f"archive_a={a_path}")
    lines.append(f"archive_b={b_path}")
    lines.append(f"members={len(a)}/{len(b)}")
    lines.append(f"member_order_identical={order_identical}")
    lines.append(f"occurrences_identical={result['member_occurrences_identical']}")
    lines.append(f"differing_occurrences={len(diffs)}")
    lines.append(f"header_differing_occurrences={len(header_diffs)}")
    for d in diffs[:40]:
        lines.append(f"DIFF[{d['index']}] {d['name_a']} size={d['size_a']}/{d['size_b']} "
                     f"sha_a={d['sha_a'][:16]} sha_b={d['sha_b'][:16]}")
        if d["strings_only_in_a"]:
            lines.append("  only_in_a=" + " | ".join(s[:160] for s in d["strings_only_in_a"]))
        if d["strings_only_in_b"]:
            lines.append("  only_in_b=" + " | ".join(s[:160] for s in d["strings_only_in_b"]))
    Path(prefix + ".txt").write_text("\n".join(lines) + "\n")
    print("\n".join(lines))
    return 0


if __name__ == "__main__":
    sys.exit(main())
