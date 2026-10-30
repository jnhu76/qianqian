#!/usr/bin/env bash
# Confirm the ffversion.h values and the four differing version.c.obj members.
set -uo pipefail
echo "=== ffversion.h values"
echo "-- #329:"; cat /home/hoo/qq329/win-l1-route/src/native/build/ffmpeg-xmake/oracle/libavutil/ffversion.h
echo "-- ours:"; cat /home/hoo/q172rc/win/native/build/ffmpeg-xmake/oracle/libavutil/ffversion.h
echo
echo "=== four differing members: version strings"
Q329=/home/hoo/qq329/win-l1-route/src/native/build/artifacts/libsongcore.a
OUR=/home/hoo/q172rc/win/native/build/artifacts/libsongcore.a
python3 - "$Q329" "$OUR" <<'PY'
import hashlib, subprocess, sys
sys.path.insert(0, "/mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/g4/build-windows")
exec(open("/mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/g4/build-windows/ar_member_cmp.py").read().split("a = members(")[0])
a = members(sys.argv[1]); b = members(sys.argv[2])
for i in range(len(a)):
    if a[i][2] != b[i][2]:
        print(f"idx={i} name={a[i][0] or '<symbol-table>'} sizes={a[i][1]}/{b[i][1]}")
        for tag, path in (("#329", sys.argv[1]), ("ours", sys.argv[2])):
            # extract the i-th member body via ar p is ambiguous with duplicate
            # names; re-parse the archive here instead.
            data = open(path, "rb").read()
            off = 8; longnames = b""; occ = -1; body = None
            while off + 60 <= len(data):
                hdr = data[off:off+60]
                name = hdr[0:16].decode("ascii", "replace").rstrip()
                size = int(hdr[48:58].decode("ascii").strip())
                cur = data[off+60:off+60+size]
                if name == "//":
                    longnames = cur
                elif name.startswith("/") and name[1:].isdigit():
                    idx0 = int(name[1:]); end = longnames.find(b"/\n", idx0)
                    real = longnames[idx0:end].decode("ascii", "replace")
                    occ += 1
                    if occ == i: body = cur; break
                else:
                    real = name[:-1] if name.endswith("/") else name
                    occ += 1
                    if occ == i: body = cur; break
                off += 60 + size + (size & 1)
            if body:
                strings = subprocess.run(["strings", "-a", "/dev/stdin"], input=body,
                                         capture_output=True).stdout.decode("latin1").splitlines()
                hits = [s for s in strings if "FFmpeg version" in s or s.startswith("qianqian-")]
                print(f"   {tag}: {hits}")
PY
