#!/usr/bin/env bash
set -u
echo "== closure manifest sha256 =="
sha256sum /home/hoo/qianqian-172-linux/native/build/ffmpeg-xmake/manifest.json
sha256sum /home/hoo/q172d2-verify/closure/ffmpeg-xmake/manifest.json
sha256sum /home/hoo/q172rc/closure/ffmpeg-xmake/manifest.json
echo "closure_sha_exit=$?"
echo "== closure manifest compact summary =="
python3 - <<'PY'
import json
for p in ("/home/hoo/qianqian-172-linux/native/build/ffmpeg-xmake/manifest.json",
          "/home/hoo/q172d2-verify/closure/ffmpeg-xmake/manifest.json"):
    m = json.load(open(p, encoding="utf-8"))
    tgt = m.get("target") or {}
    units = m.get("units")
    n_units = len(units) if isinstance(units, list) else units
    print(p)
    print("  target.id             ", tgt.get("id"))
    print("  target.recipe_sha256  ", tgt.get("recipe_sha256"))
    print("  profile               ", m.get("profile"), m.get("profile_sha256"))
    print("  ffmpeg_commit_sha     ", m.get("ffmpeg_commit_sha"))
    print("  ffmpeg_source_sha256  ", m.get("ffmpeg_source_sha256"))
    print("  configure_args        ", len(m.get("configure_args") or []))
    print("  units (truncated)     ", (n_units[:3] if isinstance(n_units, dict) else
                                       (len(n_units) if isinstance(n_units, list) else n_units)))
PY
echo "closure_parse_exit=$?"
echo "== ffmpeg-src trees present =="
ls -d /home/hoo/q172d2-verify/closure/ffmpeg-src /home/hoo/q172rc/closure/ffmpeg-src /home/hoo/qianqian-172-linux/native/build/ffmpeg-src 2>&1
echo "== COPYING files inside extracted ffmpeg-src trees =="
sha256sum /home/hoo/q172rc/closure/ffmpeg-src/COPYING.* /home/hoo/qianqian-172-linux/native/build/ffmpeg-src/COPYING.* 2>&1
echo "src_copying_exit=$?"
