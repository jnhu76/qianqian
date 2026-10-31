#!/usr/bin/env python3
"""Render LICENSES/THIRD_PARTY_NOTICES.txt for a SongCore package.

One renderer for both targets: the notice wording is release content, not a
per-platform rule, and it is derived from the verified pin plus the closure
profile hash — never hand-maintained and never copied from a historical asset.
FFmpeg COPYING texts themselves are NOT rendered here (O1 requires them
extracted from the pinned source zip).
"""
from __future__ import annotations
import argparse, json, sys

def main() -> int:
    p = argparse.ArgumentParser()
    p.add_argument("--out", required=True)
    p.add_argument("--version", required=True)
    p.add_argument("--target", required=True)
    p.add_argument("--pin", required=True)
    p.add_argument("--profile-sha256", required=True)
    a = p.parse_args()
    pin = json.load(open(a.pin, encoding="utf-8"))
    title = "Third-party notices \u2014 SongCore v%s (%s)" % (a.version, a.target)
    # Only the shared form's targets ship a libsongcore.so; saying otherwise
    # would describe a file the recipient does not have.
    shipped = ("libsongcore.a / libsongcore.so" if a.target.startswith("linux")
               else "libsongcore.a")
    lines = [
        title,
        "=" * len(title),
        "",
        "FFmpeg",
        "  version : %s (tag %s)" % (pin["ffmpeg_version"], pin["ffmpeg_tag"]),
        "  commit  : %s" % pin["ffmpeg_commit_sha"],
        "  source  : %s" % pin["source_url"],
        "  sha256  : %s" % pin["source_sha256"],
        "  profile : codec-base (machine-derived minimal closure; sha256",
        "            %s)" % a.profile_sha256,
        "  effective license: LGPL-2.1-or-later (FFmpeg default, all copylibs disabled)",
        "  external libraries that alter licensing: none",
        "",
        "  The FFmpeg closure is statically linked inside %s." % shipped,
        "  Under LGPL-2.1-or-later static distribution, the object",
        "  files of the application may be provided together with the means",
        "  (source or relink instructions) to re-link a modified FFmpeg. This",
        "  package contains the complete FFmpeg license texts (LICENSES/COPYING.*)",
        "  and the closure identity above; the exact source tree is reconstructible",
        "  from the pinned URL + sha256.",
        "",
        "Qianqian",
        "  The Qianqian sources (songcore.h and the SongCore implementation) are",
        "  dual-licensed: MIT or Apache-2.0, at the recipient's option. The",
        "  complete license texts ship in this package as LICENSES/LICENSE-MIT",
        "  and LICENSES/LICENSE-APACHE and are sha256-bound in",
        "  metadata/checksums.txt. This is an engineering statement, not legal",
        "  advice; each release's license block is owner-reviewed before",
        "  publication (contract section 10).",
        "",
    ]
    with open(a.out, "w", encoding="utf-8", newline="\n") as f:
        f.write("\n".join(lines))
    print("NOTICES_SHA256=%s" % __import__("hashlib").sha256(
        open(a.out, "rb").read()).hexdigest())
    return 0

if __name__ == "__main__":
    sys.exit(main())
