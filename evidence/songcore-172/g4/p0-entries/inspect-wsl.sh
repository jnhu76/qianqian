#!/usr/bin/env bash
# Read-only inventory of every path the ops/* entries pin, WSL side.
set -u
export PATH="$HOME/.local/bin:$PATH"
say() { printf '%s\n' "$*"; }

say "== repos =="
for r in /home/hoo/Source/qianqian /home/hoo/qianqian-172-linux; do
  if [ -d "$r/.git" ]; then
    say "repo=$r head=$(git -C "$r" rev-parse HEAD) branch=$(git -C "$r" rev-parse --abbrev-ref HEAD)"
  else
    say "repo=$r ABSENT"
  fi
done
say "== branches matching songcore-172 (WSL repo) =="
git -C /home/hoo/Source/qianqian branch -a --list '*songcore-172*' 2>&1 || true

say "== build-linux.sh pinned branch/commit exists in WSL repo? =="
git -C /home/hoo/Source/qianqian cat-file -e bf23516b5851174dc63fb5058997fa5478751713^{commit} 2>/dev/null \
  && say "pin_commit=PRESENT" || say "pin_commit=ABSENT"
git -C /home/hoo/Source/qianqian rev-parse --verify --quiet refs/remotes/origin/songcore-172/pr-a-build-fixes \
  && say "pin_branch_origin=PRESENT" || say "pin_branch_origin=ABSENT"

say "== package-rc.ps1 pinned inputs =="
for f in /home/hoo/q172d2-verify/win/native/build/downloads/ffmpeg-n9.0.1.zip \
         /home/hoo/s2pkg-v010/songcore-v0.1.0-linux-x86_64.tar.gz \
         /tmp/s3-v010/songcore-v0.1.0-linux-x86_64.tar.gz; do
  if [ -f "$f" ]; then
    say "path=$f exists sha256=$(sha256sum "$f" | awk '{print $1}') bytes=$(stat -c %s "$f")"
  else
    say "path=$f ABSENT"
  fi
done

say "== closures / caches =="
for d in /home/hoo/qianqian-172-linux/native/build/ffmpeg-xmake \
         /home/hoo/q172rc /home/hoo/q172rc/closure \
         /home/hoo/q172d2-verify/closure /home/hoo/qianqian-172-linux/native/build/ffmpeg-src; do
  if [ -e "$d" ]; then say "path=$d PRESENT"; else say "path=$d ABSENT"; fi
done
if [ -f /home/hoo/qianqian-172-linux/native/build/ffmpeg-xmake/manifest.json ]; then
  python3 - <<'PY'
import json
m=json.load(open('/home/hoo/qianqian-172-linux/native/build/ffmpeg-xmake/manifest.json'))
print("linux_closure_manifest target=%s recipe_sha256=%s schema=%s" % (
    json.dumps(m.get('target'),sort_keys=True), m.get('target',{}).get('recipe_sha256'), m.get('schema')))
PY
fi

say "== tools =="
for t in xmake python3 gcc x86_64-w64-mingw32-gcc ar objdump readelf nm git wslpath; do
  p=$(command -v "$t" 2>/dev/null) && say "tool=$t path=$p" || say "tool=$t MISSING"
done
say "xmake_version=$(xmake --version 2>/dev/null | head -1 | sed 's/\x1b\[[0-9;]*m//g')"
say "gcc_version=$(gcc --version 2>/dev/null | head -1)"
say "HOME=$HOME"
