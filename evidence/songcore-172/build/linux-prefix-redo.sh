#!/bin/bash
# songcore-172 D2: redo the PRE-FIX linux baseline with the real pre-fix file
# (extracted on the Windows side; WSL git cannot resolve the worktree .git).
set -uo pipefail
export PATH=/home/hoo/.local/bin:$PATH
L=/home/hoo/q172d2/lin
OUT=/home/hoo/q172d2/linux-green
cd "$L/native"

cp /mnt/c/Users/fred1/source/qianqian-172/evidence/songcore-172/build/prefix-songcore.lua build/songcore.lua
sha256sum build/songcore.lua
xmake f -p linux -a x86_64 -m release -y > /dev/null 2>&1; echo "F_EXIT=$?"
rm -rf build/artifacts
xmake build -r songcore > "$OUT/linux-prefix-build.log" 2>&1
echo "PREFIX_BUILD_EXIT=$?"
tail -3 "$OUT/linux-prefix-build.log"
echo "--- pre-fix artifacts:"
find build/artifacts -type f | sort
find build/artifacts -type f -exec sha256sum {} \; | sort -k2 > "$OUT/prefix-artifacts.txt"
nm -D build/artifacts/shared/libsongcore.so 2>/dev/null | awk '$2=="T"||$2=="W"{print $3}' | grep -c "^song" > "$OUT/prefix-so-songcount.txt" || echo 0 > "$OUT/prefix-so-songcount.txt"
echo "pre-fix .so exported song* count: $(cat "$OUT/prefix-so-songcount.txt")"

echo "=== comparison vs post-fix ==="
diff "$OUT/prefix-artifacts.txt" "$OUT/postfix-artifacts.txt" && echo "ARTIFACT_SETS_AND_HASHES_IDENTICAL"
echo "post-fix .so exported song* count: $(cat "$OUT/postfix-so-songcount.txt")"
echo PREFIX_REDO_DONE
