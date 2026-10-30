#!/usr/bin/env bash
# Reproduce the #330 P3 notices template against the v0.1.0 source and compare
# byte-for-byte with both shipped notices files. Also check line endings/BOM.
set -u
R=/mnt/c/Users/fred1/source/qianqian
V010=/home/hoo/g4-172/laneD-license/v010/songcore-v0.1.0-linux-x86_64/LICENSES/THIRD_PARTY_NOTICES.txt
L=$R/evidence/songcore-172/g4/package/linux/extract/songcore-v0.0.0-preflight-linux-x86_64/LICENSES/THIRD_PARTY_NOTICES.txt
W=$R/evidence/songcore-172/g4/package/win/extract/songcore-v0.0.0-preflight-windows-x86_64/LICENSES/THIRD_PARTY_NOTICES.txt

python3 - "$V010" "$L" "$W" <<'PY'
import sys, hashlib, io
v010, lpath, wpath = sys.argv[1:4]

paragraph = [
    "Qianqian",
    "  The Qianqian sources (songcore.h and the SongCore implementation) are",
    "  dual-licensed: MIT or Apache-2.0, at the recipient's option. The",
    "  complete license texts ship in this package as LICENSES/LICENSE-MIT",
    "  and LICENSES/LICENSE-APACHE and are sha256-bound in",
    "  metadata/checksums.txt. This is an engineering statement, not legal",
    "  advice; each release's license block is owner-reviewed before",
    "  publication (contract section 10).",
]

def render(version, target):
    lines = open(v010, encoding="utf-8").read().split("\n")
    assert lines[0].startswith("Third-party notices")
    assert lines[1] and set(lines[1]) == {"="}
    q = [i for i, l in enumerate(lines) if l == "Qianqian"]
    assert len(q) == 1
    title = "Third-party notices \u2014 SongCore %s (%s)" % (version, target)
    lines[0] = title
    lines[1] = "=" * len(title)
    return ("\n".join(lines[:q[0]] + paragraph) + "\n").encode("utf-8")

for path, version, target in ((lpath, "0.0.0-preflight", "linux-x86_64"),
                              (wpath, "0.0.0-preflight", "windows-x86_64")):
    want = render(version, target)
    got = open(path, "rb").read()
    print(path.rsplit("/",1)[-2] + "/" + path.rsplit("/",1)[-1])
    print("  template_sha256", hashlib.sha256(want).hexdigest())
    print("  shipped_sha256 ", hashlib.sha256(got).hexdigest())
    print("  byte_identical", want == got)
    print("  bom", got[:3] == b"\xef\xbb\xbf", "crlf", b"\r\n" in got, "cr_only", b"\r" in got)
    print("  bytes", len(got))
PY
echo "template_check_exit=$?"

echo "== FFmpeg section byte-identity (v0.1.0 vs shipped, up to Qianqian) =="
python3 - "$V010" "$L" "$W" <<'PY'
import sys, hashlib
v = open(sys.argv[1], encoding="utf-8").read().split("Qianqian")[0]
for p in sys.argv[2:]:
    s = open(p, encoding="utf-8").read().split("Qianqian")[0]
    # strip the identity header (first 2 lines) before comparing the FFmpeg body
    vb = "\n".join(v.split("\n")[2:])
    sb = "\n".join(s.split("\n")[2:])
    print(p.rsplit("/",1)[-2], "ffmpeg_body_identical", vb == sb,
          "v010_sha", hashlib.sha256(vb.encode()).hexdigest()[:16],
          "shipped_sha", hashlib.sha256(sb.encode()).hexdigest()[:16])
PY
echo "ffmpeg_section_exit=$?"
