#!/bin/bash
# sem9: no-recipe gate path of the REAL songcore.lua, end to end.
# Sandbox mirrors the native tree layout; ffmpeg/targets/ holds ONLY the
# windows-x86_64 recipe while the session is linux x86_64 -> no recipe can
# match -> the songcore phony must dep the gate and fail closed with the
# "no target recipe matches" message. Then a positive control: adding the
# linux recipe must flip the session back to building static+shared stubs.
set -u
export PATH=/home/hoo/.local/bin:$PATH
WT=/mnt/c/Users/fred1/source/qianqian-172
R=/home/hoo/tmp/sem9
rm -rf "$R"
mkdir -p "$R/native/build" "$R/native/src" "$R/native/include" "$R/native/ffmpeg/targets" "$R/native/scripts" "$R/native/ffmpeg/profiles" "$R/native/tools"

cp "$WT/native/xmake.lua"                "$R/native/xmake.lua"
cp "$WT/native/build/ffmpeg.lua"         "$R/native/build/ffmpeg.lua"
cp "$WT/native/build/songcore.lua"       "$R/native/build/songcore.lua"
cp "$WT/native/ffmpeg/targets/windows-x86_64.json" "$R/native/ffmpeg/targets/"
cp "$WT/native/ffmpeg/pin.json"          "$R/native/ffmpeg/pin.json" 2>/dev/null || true
printf '/* stub for sandbox gate test */\nint songcore_unused_probe(void) { return 0; }\n' > "$R/native/src/songcore_ffmpeg.c"
printf '#pragma once\n' > "$R/native/include/songcore.h"

cd "$R/native"
echo "=== A: no matching recipe (targets: windows only; session: linux) ==="
xmake f -p linux -a x86_64 -m release -y > /dev/null 2>&1; echo "F_EXIT=$?"
xmake build songcore > /home/hoo/tmp/sem9-a.log 2>&1; echo "BUILD_EXIT=$? (expect 255)"
tail -3 /home/hoo/tmp/sem9-a.log

echo "=== B: positive control — add linux-x86_64 recipe, same session ==="
cp "$WT/native/ffmpeg/targets/linux-x86_64.json" "$R/native/ffmpeg/targets/"
xmake f -p linux -a x86_64 -m release -y > /dev/null 2>&1; echo "F_EXIT=$?"
xmake build songcore > /home/hoo/tmp/sem9-b.log 2>&1; echo "BUILD_EXIT=$? (expect 255: qianqian_av closure gate — sandbox has no manifest)"
tail -3 /home/hoo/tmp/sem9-b.log
xmake build songcore_static > /home/hoo/tmp/sem9-c.log 2>&1; echo "STATIC_EXIT=$? (expect 255 too: closure gate)"
tail -2 /home/hoo/tmp/sem9-c.log
echo "=== C: recipe-policy gate still enforces shared in sandbox? build songcore_shared ==="
xmake build songcore_shared > /home/hoo/tmp/sem9-d.log 2>&1; echo "SHARED_EXIT=$?"
tail -2 /home/hoo/tmp/sem9-d.log
echo SEM9_DONE
