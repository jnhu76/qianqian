#!/usr/bin/env bash
set -u
R=/mnt/c/Users/fred1/source/qianqian
V010=/home/hoo/g4-172/laneD-license/v010/songcore-v0.1.0-linux-x86_64/LICENSES/THIRD_PARTY_NOTICES.txt
L=$R/evidence/songcore-172/g4/package/linux/extract/songcore-v0.0.0-preflight-linux-x86_64/LICENSES/THIRD_PARTY_NOTICES.txt
W=$R/evidence/songcore-172/g4/package/win/extract/songcore-v0.0.0-preflight-windows-x86_64/LICENSES/THIRD_PARTY_NOTICES.txt
O=/home/hoo/g4-172/laneD-license/render
mkdir -p "$O"

python3 - "$V010" "$O" <<'PY'
import sys
v010, out = sys.argv[1], sys.argv[2]
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
for version, target, name in (("0.0.0-preflight","linux-x86_64","linux"),
                              ("0.0.0-preflight","windows-x86_64","win")):
    lines = open(v010, encoding="utf-8").read().split("\n")
    q = [i for i, l in enumerate(lines) if l == "Qianqian"][0]
    title = "Third-party notices \u2014 SongCore %s (%s)" % (version, target)
    lines[0] = title
    lines[1] = "=" * len(title)
    open(out + "/" + name + ".txt", "w", encoding="utf-8", newline="\n").write(
        "\n".join(lines[:q] + paragraph) + "\n")
PY
echo "render_exit=$?"
echo "=== diff rendered(linux) vs shipped(linux) ==="
diff -u "$O/linux.txt" "$L"; echo "diff_linux_exit=$?"
echo "=== diff rendered(win) vs shipped(win) ==="
diff -u "$O/win.txt" "$W"; echo "diff_win_exit=$?"
echo "=== cat -A shipped linux tail ==="
tail -12 "$L" | cat -A
