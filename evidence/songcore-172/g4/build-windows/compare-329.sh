#!/usr/bin/env bash
# Compare our fresh RC artifact with the #329 retained windows static archive.
set -uo pipefail
Q329=/home/hoo/qq329/win-l1-route/src/native/build/artifacts/libsongcore.a
OUR=/home/hoo/q172rc/win/native/build/artifacts/libsongcore.a
W=/home/hoo/g4-cmp
rm -rf "$W"; mkdir -p "$W/a" "$W/b"

echo "=== identities"
sha256sum "$Q329" "$OUR"
stat -c '%n size=%s' "$Q329" "$OUR"

echo
echo "=== #329 closure manifest vs ours"
sha256sum /home/hoo/qq329/win-l1-route/src/native/build/ffmpeg-xmake/manifest.json 2>&1
sha256sum /home/hoo/q172rc/win/native/build/ffmpeg-xmake/manifest.json

echo
echo "=== member order / names"
(cd "$W/a" && x86_64-w64-mingw32-ar x "$Q329")
(cd "$W/b" && x86_64-w64-mingw32-ar x "$OUR")
ls "$W/a" | wc -l; ls "$W/b" | wc -l
diff <(ls "$W/a") <(ls "$W/b") > "$W/name-diff.txt" && echo "member_name_lists=identical" || { echo "member_name_lists=DIFFER"; head "$W/name-diff.txt"; }

echo
echo "=== per-member sha256 comparison"
DIFFS=0; SAME=0
: > "$W/member-diffs.txt"
for f in $(ls "$W/a"); do
  if [ -f "$W/b/$f" ]; then
    sa=$(sha256sum "$W/a/$f" | awk '{print $1}')
    sb=$(sha256sum "$W/b/$f" | awk '{print $1}')
    if [ "$sa" = "$sb" ]; then SAME=$((SAME+1)); else DIFFS=$((DIFFS+1)); echo "$f" >> "$W/member-diffs.txt"; fi
  fi
done
echo "identical_members=$SAME differing_members=$DIFFS"
echo "-- differing member names:"; cat "$W/member-diffs.txt" | head -20

echo
echo "=== build-root literal census"
echo "#329 abs-root hits: $(grep -rl '/home/hoo/qq329' "$W/a" 2>/dev/null | wc -l)"
echo "our  abs-root hits: $(grep -rl '/home/hoo/q172rc' "$W/b" 2>/dev/null | wc -l)"
echo "our  '=.'-mapped rel hits sample:"
grep -ahoo '"[^"]*libavcodec/[a-z0-9_/]*\.c"' "$W/b" 2>/dev/null | sort -u | head -5
echo "#329 mapped rel hits sample:"
grep -ahoo '"[^"]*libavcodec/[a-z0-9_/]*\.c"' "$W/a" 2>/dev/null | sort -u | head -5

echo
echo "=== first differing member: STT_FILE / FILE-symbol evidence"
FIRST=$(head -1 "$W/member-diffs.txt")
echo "member=$FIRST"
echo "-- #329 file symbols:"; x86_64-w64-mingw32-nm -a "$W/a/$FIRST" 2>/dev/null | grep -E ' (a|N) ' | head -5
echo "-- ours file symbols:"; x86_64-w64-mingw32-nm -a "$W/b/$FIRST" 2>/dev/null | grep -E ' (a|N) ' | head -5
echo "-- size delta of that member:"
stat -c '%n %s' "$W/a/$FIRST" "$W/b/$FIRST"
