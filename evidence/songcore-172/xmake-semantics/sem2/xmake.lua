set_project("sem2")
set_languages("c11")
add_rules("mode.release", "mode.debug")

-- Baseline: does a raise in on_load of an explicitly built target exit
-- nonzero? Does the on_load of an unrelated target fire at all during a
-- build of another target?

target("tstatic")
    set_kind("static")
    add_files("src/a.c")

target("tshared")
    set_kind("shared")
    add_files("src/b.c")
    on_load(function (target)
        print("[onload] tshared reached")
        raise("TSHARED-UNSUPPORTED-SENTINEL")
    end)

target("tphony")
    set_kind("phony")
    set_default(false)
    add_deps("tstatic")
    on_load(function (target)
        print("[onload] tphony reached")
    end)
