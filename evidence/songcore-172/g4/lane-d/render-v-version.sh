set -u
V010=/home/hoo/g4-172/laneD-license/v010/songcore-v0.1.0-linux-x86_64/LICENSES/THIRD_PARTY_NOTICES.txt
L=/mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/g4/package/linux/extract/songcore-v0.0.0-preflight-linux-x86_64/LICENSES/THIRD_PARTY_NOTICES.txt
W=/mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/g4/package/win/extract/songcore-v0.0.0-preflight-windows-x86_64/LICENSES/THIRD_PARTY_NOTICES.txt
python3 - "$V010" "$L" "$W" <<'PY'
import sys, hashlib
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
    q = [i for i, l in enumerate(lines) if l == "Qianqian"][0]
    title = "Third-party notices \u2014 SongCore %s (%s)" % (version, target)
    lines[0] = title
    lines[1] = "=" * len(title)
    return ("\n".join(lines[:q] + paragraph) + "\n").encode()
for path, target in ((lpath, "linux-x86_64"), (wpath, "windows-x86_64")):
    want = render("v0.0.0-preflight", target)
    got = open(path, "rb").read()
    print(target, "byte_identical_with_v_prefix", want == got,
          "template", hashlib.sha256(want).hexdigest()[:16],
          "shipped", hashlib.sha256(got).hexdigest()[:16])
PY
echo "render_v_exit=$?"
