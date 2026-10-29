#!/bin/bash
# Xmake semantics matrix — clean quoting channel (base64 transferred).
set -u
export PATH=/home/hoo/.local/bin:$PATH
R=/home/hoo/tmp/semres
rm -rf "$R" && mkdir -p "$R"

mk() { # $1=dir $2=lua content
  mkdir -p "$1/src"
  printf '%s' "$2" > "$1/xmake.lua"
  printf 'int add_two(int x) { return x + 2; }\n' > "$1/src/a.c"
  printf 'int sub_two(int x) { return x - 2; }\n' > "$1/src/b.c"
}

# --- p1: phony + dynamic dep add in on_load, plus a before_build raise on an unrelated target
mk "$R/p1" 'set_project("p1")
set_languages("c11")
add_rules("mode.release", "mode.debug")
target("tstatic")
    set_kind("static")
    add_files("src/a.c")
target("tshared")
    set_kind("shared")
    add_files("src/b.c")
    before_build(function (target)
        raise("TSHARED-BEFOREBUILD-SENTINEL")
    end)
target("tphony")
    set_kind("phony")
    set_default(false)
    before_build(function (target)
        print("[bb] tphony")
    end)
    on_load(function (target)
        print("[ol] tphony: add dep tstatic")
        target:add("deps", "tstatic")
    end)
'

# --- p2: on_load raise target + clean static
mk "$R/p2" 'set_project("p2")
set_languages("c11")
add_rules("mode.release", "mode.debug")
target("tstatic")
    set_kind("static")
    add_files("src/a.c")
target("tshared")
    set_kind("shared")
    add_files("src/b.c")
    on_load(function (target)
        print("[ol] tshared reached")
        raise("TSHARED-ONLOAD-SENTINEL")
    end)
'

run() { echo "--- $*"; "$@" > /dev/null 2>&1; echo "exit=$?"; }

echo "===== p1 ====="
cd "$R/p1"
run xmake f -p linux -a x86_64 -m release -y
run xmake build tphony            # A: dynamic dep add → tstatic built?
ls build/linux/x86_64/release/ 2>/dev/null
run xmake build tstatic           # B: unrelated before_build raise must not interfere
run xmake build tshared           # C: explicit build of before_build-raise target
echo "=== p1 C full output ==="
xmake build tshared 2>&1 | tail -5

echo "===== p2 ====="
cd "$R/p2"
run xmake f -p linux -a x86_64 -m release -y
run xmake build tstatic           # D: on_load raise elsewhere
run xmake build tshared           # E: explicit build of on_load-raise target
echo "=== p2 D full output ==="
xmake build tstatic 2>&1 | tail -6
echo "=== p2 F: xmake build nonexistent ==="
run xmake build no-such-target
echo "=== p2 F full output ==="
xmake build no-such-target 2>&1 | tail -3
echo "MATRIX_DONE"
