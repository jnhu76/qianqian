#!/usr/bin/env python3
"""b329-arcmp.py — occurrence-indexed ar archive comparison for SongCore #329.

Unlike `ar x`/`ar t` workflows that collapse duplicate member names, this tool
enumerates EVERY member occurrence (name + occurrence index) and compares the
two archives occurrence by occurrence, as required by #329 B1 ("13 duplicate-name
members using occurrence index; do not collapse via ar x by basename").

Usage:
  b329-arcmp.py <archive1> <archive2> <out-prefix> <root1-label> <root2-label>

Outputs (next to <out-prefix>):
  <out>-1.tsv, <out>-2.tsv : name<TAB>occ<TAB>size<TAB>sha256<TAB>date<TAB>uid<TAB>gid<TAB>mode
  <out>-cmp.txt            : verdict + per-key table + duplicates + header audit + order check
  <out>-leak.txt           : absolute-path leak census over ALL members of both archives

Exit: 0 = fully identical (members, order, headers); 1 = differences; 2 = tool error.
"""
import hashlib
import sys
from pathlib import Path

GLOBAL_MAGIC = b"!<arch>\n"
HEAD = 60


class ARError(Exception):
    pass


def parse_archive(path: Path):
    data = path.read_bytes()
    if not data.startswith(GLOBAL_MAGIC):
        raise ARError(f"{path}: not a GNU ar archive (bad magic)")
    pos = len(GLOBAL_MAGIC)
    members = []
    longnames = b""
    while pos < len(data):
        if len(data) - pos < HEAD:
            raise ARError(f"{path}: truncated member header at offset {pos}")
        hdr = data[pos:pos + HEAD]
        name = hdr[0:16].decode("ascii", "replace").rstrip()
        date = hdr[16:28].decode("ascii", "replace").strip()
        uid = hdr[28:34].decode("ascii", "replace").strip()
        gid = hdr[34:40].decode("ascii", "replace").strip()
        mode = hdr[40:48].decode("ascii", "replace").strip()
        size_s = hdr[48:58].decode("ascii", "replace").strip()
        fmag = hdr[58:60]
        if fmag != b"`\n":
            raise ARError(f"{path}: bad member magic at offset {pos}: {fmag!r}")
        size = int(size_s)
        body = data[pos + HEAD:pos + HEAD + size]
        if len(body) != size:
            raise ARError(f"{path}: truncated member body at offset {pos}")
        pos += HEAD + size + (size % 2)  # members are 2-byte aligned
        if name == "//":
            longnames = body
            members.append({"name": "<longnames>", "raw": name, "date": date,
                            "uid": uid, "gid": gid, "mode": mode, "size": size,
                            "sha256": hashlib.sha256(body).hexdigest()})
            continue
        if name.startswith("/"):
            real = "<symbol-table>" if name == "/" else "<symbol-table-ext>"
            members.append({"name": real, "raw": name, "date": date, "uid": uid,
                            "gid": gid, "mode": mode, "size": size,
                            "sha256": hashlib.sha256(body).hexdigest()})
            continue
        if name.startswith("#1/"):  # BSD style, defensive
            nlen = int(name[3:])
            real = body[:nlen].rstrip(b"\x00").decode("utf-8", "replace")
            body = body[nlen:]
            members.append({"name": real, "raw": name, "date": date, "uid": uid,
                            "gid": gid, "mode": mode, "size": size,
                            "sha256": hashlib.sha256(body).hexdigest(),
                            "content": body})
            continue
        real = name[:-1] if name.endswith("/") else name
        if real.startswith("/"):  # GNU long name reference
            off = int(real[1:])
            end = longnames.find(b"/\n", off)
            if end < 0:
                raise ARError(f"{path}: long-name offset {off} out of range")
            real = longnames[off:end].decode("utf-8", "replace")
        members.append({"name": real, "raw": name, "date": date, "uid": uid,
                        "gid": gid, "mode": mode, "size": size,
                        "sha256": hashlib.sha256(body).hexdigest(),
                        "content": body})
    return members


def with_occurrences(members):
    seen = {}
    out = []
    for m in members:
        n = seen.get(m["name"], 0) + 1
        seen[m["name"]] = n
        out.append((m["name"], n, m))
    return out


def leak_scan(members, label):
    pats = [label.encode(), b"/home/", b"C:\\", b"/mnt/", b"/tmp/"]
    hits = {}
    total = {p.decode("latin1"): 0 for p in pats}
    for m in members:
        body = m.get("content", b"")
        if not body:
            continue
        for p in pats:
            c = body.count(p)
            if c:
                hits.setdefault(m["name"], {})[p.decode("latin1")] = \
                    hits.get(m["name"], {}).get(p.decode("latin1"), 0) + c
                total[p.decode("latin1")] += c
    return hits, total


def main():
    if len(sys.argv) != 6:
        print(__doc__)
        return 2
    a1, a2, out, lab1, lab2 = (Path(sys.argv[1]), Path(sys.argv[2]),
                               Path(sys.argv[3]), sys.argv[4], sys.argv[5])
    m1 = parse_archive(a1)
    m2 = parse_archive(a2)
    k1 = with_occurrences(m1)
    k2 = with_occurrences(m2)
    d1 = dict(((n, o), m) for n, o, m in k1)
    d2 = dict(((n, o), m) for n, o, m in k2)

    lines = []
    order1 = [n for n, _, _ in k1]
    order2 = [n for n, _, _ in k2]
    names1 = {k[0] for k in d1}
    names2 = {k[0] for k in d2}
    only1 = sorted(names1 - names2)
    only2 = sorted(names2 - names1)

    diff_keys, same_keys = [], []
    for key in sorted(set(d1) | set(d2)):
        x, y = d1.get(key), d2.get(key)
        if x is None or y is None:
            diff_keys.append((key, "ONE_SIDED"))
        elif x["sha256"] == y["sha256"] and x["size"] == y["size"]:
            same_keys.append(key)
        else:
            diff_keys.append((key, "BYTES_DIFFER"))

    dup_names = sorted({n for n in names1
                        if sum(1 for kk in d1 if kk[0] == n) > 1})
    hdr_anomalies = []
    for side, mm in (("r1", m1), ("r2", m2)):
        for m in mm:
            if m["name"] == "<symbol-table>":
                continue
            if m["date"] not in ("0", "") or m["uid"] not in ("0", "") \
               or m["gid"] not in ("0", ""):
                hdr_anomalies.append((side, m["name"], m["date"], m["uid"], m["gid"]))

    order_equal = order1 == order2
    identical = (not diff_keys and not only1 and not only2
                 and order_equal and len(m1) == len(m2))

    lines.append(f"ARCHIVE1={a1} members={len(m1)}")
    lines.append(f"ARCHIVE2={a2} members={len(m2)}")
    lines.append(f"ORDER_IDENTICAL={'yes' if order_equal else 'NO'}")
    lines.append(f"MEMBER_COUNT_EQUAL={'yes' if len(m1) == len(m2) else 'NO'}")
    lines.append(f"IDENTICAL_OCCURRENCES={len(same_keys)}")
    lines.append(f"DIFFERING_OCCURRENCES={len(diff_keys)}")
    lines.append(f"ONLY_IN_R1={only1}")
    lines.append(f"ONLY_IN_R2={only2}")
    lines.append(f"DUPLICATE_NAMES={len(dup_names)}")
    for n in dup_names:
        occ1 = sum(1 for kk in d1 if kk[0] == n)
        occ2 = sum(1 for kk in d2 if kk[0] == n)
        states = []
        for i in range(1, max(occ1, occ2) + 1):
            x, y = d1.get((n, i)), d2.get((n, i))
            if x is None or y is None:
                states.append(f"#{i}=ONE_SIDED")
            elif x["sha256"] == y["sha256"]:
                states.append(f"#{i}=SAME")
            else:
                states.append(f"#{i}=DIFFER")
        lines.append(f"DUP {n} occ_r1={occ1} occ_r2={occ2} {','.join(states)}")
    lines.append("--- differing occurrences ---")
    for key, why in diff_keys:
        x, y = d1.get(key), d2.get(key)
        xs = x["sha256"] if x else "-"
        ys = y["sha256"] if y else "-"
        lines.append(f"DIFF {key[0]}#{key[1]} {why} r1={xs} r2={ys}")
    lines.append("--- header-field audit (expect zeroed date/uid/gid) ---")
    lines.append("HEADER_ANOMALIES=%d" % len(hdr_anomalies))
    for row in hdr_anomalies[:20]:
        lines.append("HDR %s %s date=%s uid=%s gid=%s" % row)
    lines.append("--- leak census (all members, both archives) ---")
    h1, t1 = leak_scan(m1, lab1)
    h2, t2 = leak_scan(m2, lab2)
    lines.append(f"R1_LABEL={lab1} TOTAL_HITS={t1}")
    lines.append(f"R2_LABEL={lab2} TOTAL_HITS={t2}")
    for side, hits in (("r1", h1), ("r2", h2)):
        for name in sorted(hits):
            lines.append(f"LEAK {side} {name} {hits[name]}")

    cmp_path = Path(str(out) + "-cmp.txt")
    cmp_path.write_text("\n".join(lines) + "\n", encoding="utf-8")
    for side, kk in (("1", k1), ("2", k2)):
        tsv = Path(str(out) + "-" + side + ".tsv")
        with tsv.open("w", encoding="utf-8") as f:
            for n, o, m in kk:
                f.write(f"{n}\t{o}\t{m['size']}\t{m['sha256']}\t"
                        f"{m['date']}\t{m['uid']}\t{m['gid']}\t{m['mode']}\n")
    for line in lines[:8]:
        print(line)
    print(f"cmp={cmp_path}")
    return 0 if identical else 1


if __name__ == "__main__":
    try:
        sys.exit(main())
    except ARError as exc:
        print(f"TOOL_ERROR: {exc}", file=sys.stderr)
        sys.exit(2)
