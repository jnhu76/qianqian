#!/bin/bash
# sem8: clean test — phony with NO description-scope deps; the dep set comes
# ONLY from on_config. Checks: on_config refire count, reload behavior,
# whether dynamically added deps are built, exit codes.
set -u
export PATH=/home/hoo/.local/bin:$PATH
R=/home/hoo/tmp/sem8
rm -rf "$R" && mkdir -p "$R/src"
printf 'int add_two(int x) { return x + 2; }\n' > "$R/src/a.c"
printf 'int sub_two(int x) { return x - 2; }\n' > "$R/src/b.c"

cat > "$R/xmake.lua" << 'EOF'
set_project("sem8")
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

target("tgate")
    set_kind("phony")
    set_default(false)
    before_build(function (target)
        raise("GATE-SENTINEL")
    end)

target("tphony")
    set_kind("phony")
    set_default(false)
    on_config(function (target)
        import("core.base.json")
        print("[onconfig tphony] firing; import=" .. tostring(import ~= nil))
        -- pretend recipe says static only
        target:add("deps", "tstatic")
    end)
EOF

cd "$R"
echo "===== f ====="
xmake f -p linux -a x86_64 -m release -y 2>&1 | grep -cE "onconfig tphony"
echo "F_EXIT=$?"
echo "===== build tphony (expect 0, libtstatic.a, NO libtshared.so, NO gate) ====="
xmake build tphony 2>&1 | grep -E "onconfig|compiling|archiving|linking|ok|error"
xmake build tphony > /dev/null 2>&1; echo "BUILD_EXIT=$?"
find build -name "*.a" -o -name "*.so" | sort
echo "===== build tgate explicit (expect 255) ====="
xmake build tgate > /dev/null 2>&1; echo "GATE_EXIT=$?"
echo "===== build tshared explicit (expect 255) ====="
xmake build tshared > /dev/null 2>&1; echo "SHARED_EXIT=$?"
echo "===== f AGAIN then build (config-cached path) ====="
xmake f -p linux -a x86_64 -m release -y > /dev/null 2>&1
rm -rf build/linux
xmake build tphony > /dev/null 2>&1; echo "REBUILD_EXIT=$?"
find build -name "*.a" | sort
echo SEM8_DONE
