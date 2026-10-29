set_project("semtest")
set_languages("c11")
set_config("builddir", "build/xmake")
add_rules("mode.release", "mode.debug")

-- Test 1: does an on_load raise on a NON-dependency target fire when the
-- phony (or another target) is built?
-- Test 2: does target:add("deps", ...) inside the phony's on_load actually
-- build the dynamically added dep?
-- Test 3: does `xmake f` / project load trigger on_load?
-- Test 4: does a raise in on_load of an explicitly built target exit nonzero
-- with the message?
-- Test 5: can an included lua file import("core.base.json") and read JSON at
-- description scope (file scope)?
includes("build/other.lua")

target("tstatic")
    set_kind("static")
    add_files("src/a.c")
    on_load(function (target)
        print("[onload] tstatic plat=" .. target:plat() .. " arch=" .. target:arch())
    end)

target("tshared")
    set_kind("shared")
    add_files("src/b.c")
    on_load(function (target)
        print("[onload] tshared reached")
        raise("TSHARED-UNSUPPORTED-SENTINEL: shared form not enabled by fake recipe")
    end)

target("tphony")
    set_kind("phony")
    set_default(false)
    on_load(function (target)
        print("[onload] tphony reached; dynamically adding dep tstatic")
        target:add("deps", "tstatic")
    end)
