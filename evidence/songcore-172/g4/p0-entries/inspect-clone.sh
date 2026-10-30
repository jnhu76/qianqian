#!/usr/bin/env bash
# Read-only: clone reuse preconditions + linux closure reusable preconditions.
set -u
DST=/home/hoo/qianqian-172-linux
say() { printf '%s\n' "$*"; }

cd "$DST" || { say "DST missing"; exit 0; }
say "head=$(git rev-parse HEAD)"
say "branch=$(git rev-parse --abbrev-ref HEAD)"
if git diff-index --quiet HEAD --; then say "diff_index_clean=YES"; else say "diff_index_clean=NO"; fi
say "status_porcelain_lines=$(git status --porcelain | wc -l)"
say "status_untracked_only=$(git status --porcelain | grep -vc '^??')"

say "== closure reusable preconditions (build-linux.sh stage2) =="
for f in native/ffmpeg/pin.json \
         native/build/ffmpeg-xmake/manifest.json \
         native/build/ffmpeg-src/.qianqian-verified \
         native/build/ffmpeg-xmake/oracle/ffbuild/config.mak \
         native/build/ffmpeg-xmake/oracle/config_components.h \
         native/ffmpeg/targets/linux-x86_64.json \
         native/ffmpeg/profiles/codec-base.json; do
  [ -f "$f" ] && say "file=$f PRESENT" || say "file=$f ABSENT"
done
[ -f native/build/ffmpeg-src/.qianqian-verified ] && say "verified_marker=$(cat native/build/ffmpeg-src/.qianqian-verified)"
PIN_COMMIT=$(python3 -c "import json;print(json.load(open('native/ffmpeg/pin.json'))['ffmpeg_commit_sha'])" 2>/dev/null) && say "pin_commit=$PIN_COMMIT"
PIN_SHA=$(python3 -c "import json;print(json.load(open('native/ffmpeg/pin.json'))['source_sha256'])" 2>/dev/null) && say "pin_sha=$PIN_SHA"
[ -f native/build/artifacts/libsongcore.a ] && say "linux_a_sha=$(sha256sum native/build/artifacts/libsongcore.a | awk '{print $1}') bytes=$(stat -c %s native/build/artifacts/libsongcore.a)" || say "linux_a=ABSENT"
[ -f native/build/artifacts/shared/libsongcore.so ] && say "linux_so_sha=$(sha256sum native/build/artifacts/shared/libsongcore.so | awk '{print $1}')" || say "linux_so=ABSENT"

say "== sizes =="
du -sh "$DST" 2>/dev/null
du -sh "$DST/native/build" 2>/dev/null
du -sh /home/hoo/q172rc 2>/dev/null
du -sh /home/hoo/q172d2-verify/closure 2>/dev/null

say "== is bf23516 an ancestor of main(69ba1ce)? (WSL repo) =="
git -C /home/hoo/Source/qianqian merge-base --is-ancestor bf23516b5851174dc63fb5058997fa5478751713 69ba1ce05a2772f5aa077c6ece3365c40f6e0500 \
  && say "pin_is_ancestor_of_HEAD=YES" || say "pin_is_ancestor_of_HEAD=NO"
say "commits_between=$(git -C /home/hoo/Source/qianqian rev-list --count bf23516b5851174dc63fb5058997fa5478751713..69ba1ce05a2772f5aa077c6ece3365c40f6e0500)"
