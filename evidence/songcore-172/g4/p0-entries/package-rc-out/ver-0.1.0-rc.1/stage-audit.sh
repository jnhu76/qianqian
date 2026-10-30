set -euo pipefail
# stage-audit.sh -- WSL side of ops/package-rc.ps1. Emits KEY=VALUE lines on
# stdout; any identity failure exits nonzero (fail-closed) with FAIL on stderr.
ZIP="$QR_PINNED_ZIP"
V010="$QR_V010_TAR"
STAGE="$QR_STAGE_WSL"
WT="$QR_WORKTREE_WSL"
ARCHIVE="$QR_ARTIFACT_WSL"
AR=x86_64-w64-mingw32-ar
NM=x86_64-w64-mingw32-nm
OD=x86_64-w64-mingw32-objdump

for t in "$AR" "$NM" "$OD" sha256sum python3 tar; do
    command -v "$t" > /dev/null || { echo "FAIL: missing tool $t" >&2; exit 10; }
done

# O1 basis: the pinned FFmpeg source zip is re-verified against pin.json on
# every run before anything is extracted from it.
EXPECT_ZIP=$(python3 -c "import json;print(json.load(open('$WT/native/ffmpeg/pin.json'))['source_sha256'])")
GOT_ZIP=$(sha256sum "$ZIP" | awk '{print $1}')
if [ "$GOT_ZIP" != "$EXPECT_ZIP" ]; then
    echo "FAIL: pinned FFmpeg zip sha mismatch: $GOT_ZIP != $EXPECT_ZIP" >&2
    exit 21
fi
echo "ZIP_SHA256=$GOT_ZIP"

mkdir -p "$STAGE/LICENSES"
python3 - "$ZIP" "$STAGE/LICENSES" <<'PY'
import os, sys, zipfile
zip_path, out = sys.argv[1], sys.argv[2]
names = ["COPYING.GPLv2", "COPYING.GPLv3", "COPYING.LGPLv2.1", "COPYING.LGPLv3"]
z = zipfile.ZipFile(zip_path)
# GitHub source zips prefix every member with a top-level dir (FFmpeg-n9.0.1/);
# resolve each COPYING text by its unique suffix, fail-closed on ambiguity.
members = z.namelist()
resolved = {}
for n in names:
    hits = [m for m in members if m.split("/")[-1] == n]
    if len(hits) != 1:
        print("FAIL: pinned zip member resolution for %s: %d hits" % (n, len(hits)), file=sys.stderr)
        sys.exit(22)
    resolved[n] = hits[0]
for n in names:
    with z.open(resolved[n]) as src, open(os.path.join(out, n), "wb") as dst:
        dst.write(src.read())
print("COPYING_EXTRACTED=" + ",".join(names))
PY
for n in COPYING.GPLv2 COPYING.GPLv3 COPYING.LGPLv2.1 COPYING.LGPLv3; do
    echo "SHA_$n=$(sha256sum "$STAGE/LICENSES/$n" | awk '{print $1}')"
done

# THIRD_PARTY_NOTICES.txt origin: the published songcore-v0.1.0 asset,
# re-verified against the frozen S-V010 sha256 on every run (contract
# section 7 origin note + section 10 O2/B3).
GOT_V010=$(sha256sum "$V010" | awk '{print $1}')
if [ "$GOT_V010" != "$QR_V010_SHA" ]; then
    echo "FAIL: songcore-v0.1.0 tarball sha mismatch: $GOT_V010 != $QR_V010_SHA" >&2
    exit 23
fi
echo "V010_TAR_SHA256=$GOT_V010"
V010_MEMBER="songcore-v0.1.0-linux-x86_64/LICENSES/THIRD_PARTY_NOTICES.txt"
TMPD=$(mktemp -d)
tar -xzf "$V010" -C "$TMPD" "$V010_MEMBER"
mv "$TMPD/$V010_MEMBER" "$STAGE/LICENSES/THIRD_PARTY_NOTICES.txt"
rm -rf "$TMPD"

# P3 wording refresh (Issue #330; review-C F4 / P3-3): the v0.1.0-extracted
# notices carry two stale statements 鈥?the v0.1.0/linux identity header and
# the "carry no open-source license yet ... NOT granted by this package"
# Qianqian paragraph, which contradicts the LICENSE-MIT/LICENSE-APACHE texts
# shipped alongside in the same package (B4) and the repo-root dual license.
# The historical RC container keeps the stale wording on purpose: archived
# evidence is not rewritten (frozen zip sha256 7b4d0615d25af464739373a0a356
# 5f77f3208ce9c263f59e578d443801536415 unchanged). Every pack from this
# script forward gets the corrected identity header and component-license
# paragraph below; the extracted FFmpeg section stays byte-identical to the
# v0.1.0 asset. Fail-closed: if the v0.1.0-extracted shape ever drifts, the
# run aborts (exit 24) instead of shipping a silent partial rewrite.
python3 - "$STAGE/LICENSES/THIRD_PARTY_NOTICES.txt" <<'PY'
import os, sys
path = sys.argv[1]
lines = open(path, encoding="utf-8").read().split("\n")
if not (len(lines) >= 3 and lines[0].startswith("Third-party notices")
        and lines[1] and set(lines[1]) == {"="}):
    sys.stderr.write("FAIL: notices identity header shape drifted\n")
    sys.exit(24)
q = [i for i, l in enumerate(lines) if l == "Qianqian"]
if len(q) != 1:
    sys.stderr.write("FAIL: expected exactly one Qianqian section, got %d\n" % len(q))
    sys.exit(24)
title = "Third-party notices \u2014 SongCore %s (%s)" % (
    os.environ["QR_NOTICES_VERSION"], os.environ["QR_NOTICES_TARGET"])
lines[0] = title
lines[1] = "=" * len(title)
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
open(path, "w", encoding="utf-8", newline="\n").write(
    "\n".join(lines[:q[0]] + paragraph) + "\n")
PY
echo "NOTICES_SHA256=$(sha256sum "$STAGE/LICENSES/THIRD_PARTY_NOTICES.txt" | awk '{print $1}')"

# R3 archive target identity (method mirror of tools/check_release.py:117).
PE=$("$OD" -f "$ARCHIVE" 2>/dev/null | grep -c "file format pe-x86-64") || PE=0
ELF=$("$OD" -f "$ARCHIVE" 2>/dev/null | grep -c "file format elf") || ELF=0
if [ "$PE" -le 0 ] || [ "$ELF" -ne 0 ]; then
    echo "FAIL: R3 archive target identity: pe=$PE elf=$ELF (expected pe>0 elf==0)" >&2
    exit 14
fi
echo "PE_MEMBERS=$PE"
echo "ELF_MEMBERS=$ELF"

# V4 static symbol audit: all 15 ABI v1 entry points DEFINED in the archive.
SONGS=$("$NM" "$ARCHIVE" 2>/dev/null | grep " T song" | awk '{print $3}' | sort -u)
SONGN=$(echo "$SONGS" | grep -c .) || SONGN=0
if [ "$SONGN" -ne 15 ]; then
    echo "FAIL: static symbol audit: $SONGN song* T symbols (expected 15)" >&2
    exit 15
fi
echo "SONG_SYMBOLS=15"
printf "%s\n" "$SONGS" > "$STAGE/../audit-symbols.txt"
while IFS= read -r s; do
    if [ -n "$s" ]; then echo "SYMBOL=$s"; fi
done <<< "$SONGS"

MCOUNT=$("$AR" t "$ARCHIVE" | grep -c .)
if [ "$MCOUNT" -ne "$PE" ]; then
    echo "FAIL: member count $MCOUNT != PE member count $PE" >&2
    exit 24
fi
echo "MEMBER_COUNT=$MCOUNT"

# Contract section 9 runtime-dependency audit base: the undefined-import set.
"$NM" "$ARCHIVE" 2>/dev/null | grep " U " | awk '{print $2}' | sort -u > "$STAGE/../audit-undefined.txt"
BCRYPT=$(grep -E "^BCrypt" "$STAGE/../audit-undefined.txt" | paste -sd, -)
if [ -z "$BCRYPT" ]; then
    echo "FAIL: BCrypt* undefined imports absent (contract section 9 audit base changed)" >&2
    exit 25
fi
echo "UNDEFINED_BCRYPT=$BCRYPT"
echo "UNDEFINED_COUNT=$(grep -c . "$STAGE/../audit-undefined.txt")"
echo "AUDIT_OK"