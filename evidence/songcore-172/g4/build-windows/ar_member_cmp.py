#!/usr/bin/env python3
"""Compare two GNU ar archives member-by-member by OCCURRENCE INDEX.

The SongCore static archive contains duplicate member names (the q2-crossroot
comparator hit the same issue), so extraction-by-name is not a valid census.
"""
import hashlib
import sys


def members(path):
    data = open(path, "rb").read()
    if data[:8] != b"!<arch>\n":
        raise SystemExit(f"not an ar archive: {path}")
    off = 8
    longnames = b""
    out = []
    while off + 60 <= len(data):
        hdr = data[off:off + 60]
        if hdr[58:60] != b"`\n":
            raise SystemExit(f"bad member header at {off} in {path}")
        name = hdr[0:16].decode("ascii", "replace").rstrip()
        size = int(hdr[48:58].decode("ascii").strip())
        body = data[off + 60: off + 60 + size]
        if name == "//":
            longnames = body
        elif name.startswith("/") and name[1:].isdigit():
            idx = int(name[1:])
            end = longnames.find(b"/\n", idx)
            real = longnames[idx:end].decode("ascii", "replace")
            out.append((real, size, hashlib.sha256(body).hexdigest()))
        else:
            real = name[:-1] if name.endswith("/") else name
            if real not in ("/", "/SYM64/"):
                out.append((real, size, hashlib.sha256(body).hexdigest()))
        off += 60 + size + (size & 1)
    return out


a = members(sys.argv[1])
b = members(sys.argv[2])
print(f"occurrences_a={len(a)} occurrences_b={len(b)}")
same = diff = 0
diff_idx = []
for i in range(min(len(a), len(b))):
    if a[i][0] == b[i][0] and a[i][2] == b[i][2]:
        same += 1
    else:
        diff += 1
        diff_idx.append(i)
print(f"identical_occurrences={same} differing_occurrences={diff}")
for i in diff_idx[:20]:
    print(f"  idx={i} a={a[i][0]} size={a[i][1]} sha={a[i][2][:16]}")
    print(f"          b={b[i][0]} size={b[i][1]} sha={b[i][2][:16]}")
if len(a) != len(b):
    print("LENGTH MISMATCH")
# order check: name sequence equality
print("name_sequence_equal =", [m[0] for m in a] == [m[0] for m in b])
