#!/bin/bash
# sem7: probe what is available in on_load across xmake's load passes
# (config vs build phase) and whether on_config can add deps.
set -u
export PATH=/home/hoo/.local/bin:$PATH
R=/home/hoo/tmp/sem7
rm -rf "$R" && mkdir -p "$R/src" "$R/inc"
printf 'int add_two(int x) { return x + 2; }\n' > "$R/src/a.c"
printf 'int sub_two(int x) { return x - 2; }\n' > "$R/src/b.c"

cat > "$R/inc/other.lua" << 'EOF'
print("[desc] other.lua: import=" .. tostring(import) .. " io=" .. tostring(io) .. " json=" .. tostring(json))
EOF

cat > "$R/xmake.lua" << 'EOF'
set_project("sem7")
set_languages("c11")
add_rules("mode.release", "mode.debug")
includes("inc/other.lua")

target("tstatic")
    set_kind("static")
    add_files("src/a.c")
    on_load(function (target)
        print("[onload tstatic] import=" .. tostring(import)
            .. " io=" .. tostring(io)
            .. " os.files=" .. tostring(os.files)
            .. " plat=" .. tostring(target:plat())
            .. " arch=" .. tostring(target:arch())
            .. " is_plat(mingw)=" .. tostring(is_plat("mingw")))
    end)

target("tphony")
    set_kind("phony")
    set_default(false)
    add_deps("tstatic")
    on_load(function (target)
        print("[onload tphony] import=" .. tostring(import))
    end)

target("tonconfig")
    set_kind("phony")
    set_default(false)
    add_deps("tstatic")
    on_config(function (target)
        print("[onconfig tonconfig] import=" .. tostring(import)
            .. " plat=" .. tostring(target:plat()))
        target:add("deps", "tstatic")
        print("[onconfig tonconfig] added dep tstatic")
    end)
EOF

cd "$R"
echo "===== xmake f ====="
xmake f -p linux -a x86_64 -m release -y 2>&1 | grep -E "onload|onconfig|desc|error" 
echo "F_EXIT=$?"
echo "===== xmake build tonconfig (does on_config dep take effect?) ====="
rm -rf build/linux
xmake build tonconfig 2>&1 | grep -E "onload|onconfig|desc|error|compiling|archiving|ok"
echo "BUILD_EXIT=$?"
find build -name "*.a" 2>/dev/null
echo SEM7_DONE
