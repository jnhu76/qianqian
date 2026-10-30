#!/usr/bin/env bash
# probe-xmake-dotdir.sh — does `xmake --version` create ./.xmake ?
set -uo pipefail
export PATH="$HOME/.local/bin:$PATH"
export LC_ALL=C
T=$(mktemp -d /tmp/xmprobe.XXXXXX)
cd "$T"
echo "cwd=$(pwd)"
xmake --version >/dev/null 2>&1 || true
echo "after xmake --version:"; ls -a "$T"
echo "---- contents of the WSL repo .xmake ----"
find /home/hoo/Source/qianqian/.xmake -printf '%TY-%Tm-%Td %TH:%TM:%TS %y %p\n' | head -20
rm -rf "$T"
