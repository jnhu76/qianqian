#!/bin/bash
# sem6: multi-dep dynamic add in phony on_load + zero-form gate target path.
set -u
export PATH=/home/hoo/.local/bin:$PATH
R=/home/hoo/tmp/sem6
rm -rf "$R" && mkdir -p "$R/src"
printf 'int add_two(int x) { return x + 2; }\n' > "$R/src/a.c"
printf 'int sub_two(int x) { return x - 2; }\n' > "$R/src/b.c"
cat > "$R/xmake.lua" << 'EOF'
set_project("sem6")
set_languages("c11")
add_rules("mode.release", "mode.debug")

target("tstatic")
    set_kind("static")
    add_files("src/a.c")

target("tshared")
    set_kind("shared")
    add_files("src/b.c")

target("tgate")
    set_kind("phony")
    set_default(false)
    before_build(function (target)
        raise("GATE-SENTINEL: no supported artifact form")
    end)

target("tphony")
    set_kind("phony")
    set_default(false)
    on_load(function (target)
        local multi = os.getenv("SEM6_MODE") or "multi"
        if multi == "multi" then
            print("[ol] adding tstatic + tshared")
            target:add("deps", "tstatic", "tshared")
        else
            print("[ol] zero forms -> gate")
            target:add("deps", "tgate")
        end
    end)
EOF
cd "$R"
xmake f -p linux -a x86_64 -m release -y > /dev/null 2>&1
echo "--- SEM6_MODE=multi: build tphony (expect exit 0, both artifacts)"
SEM6_MODE=multi xmake build tphony > /dev/null 2>&1; echo "multi_exit=$?"
ls build/linux/x86_64/release/ 2>/dev/null
echo "--- SEM6_MODE=zero: build tphony (expect exit 255 via gate)"
xmake f -p linux -a x86_64 -m release -y > /dev/null 2>&1
SEM6_MODE=zero xmake build tphony 2>&1 | tail -2
SEM6_MODE=zero xmake build tphony > /dev/null 2>&1; echo "zero_exit=$?"
echo SEM6_DONE
