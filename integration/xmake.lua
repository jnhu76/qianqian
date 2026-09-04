-- Integration ownership module: consumer-side proofs of the public layer
-- boundary. Integration references native exclusively through global
-- registries (target and task names) — never by include, never by shared
-- local state.

-- The linux/macos default session owns build/artifacts directly (historical
-- layout). A mingw/windows session redirects every artifact into a platform
-- subdir: sharing one dir would let a mingw libsongcore.a silently replace
-- the linux archive (and vice versa) across sessions.
local artifact_dir = path.join(os.projectdir(), "build", "artifacts")
if is_plat("mingw", "windows") then
    artifact_dir = path.join(artifact_dir, "windows-mingw-x86_64")
end

-- External FFI smoke (closure spec §32/§50): a pure-C consumer that knows
-- ONLY include/player_engine.h and the location of the runtime library.
-- No internal headers, no implementation archives — it loads the shared
-- library at run time (LoadLibraryA / dlopen) exactly like the future KMP
-- consumer will.
target("ffi_smoke")
    set_kind("binary")
    set_default(false)
    set_targetdir(artifact_dir)
    add_files("$(projectdir)/integration/ffi/ffi_smoke.c")
    add_includedirs("$(projectdir)/native/include")
    set_languages("c11")
    if is_plat("linux", "macosx") then
        add_syslinks("dl")
    end
    add_tests("default")
    on_test(function (target, opt)
        -- The smoke consumes the runtime as a pure external consumer would;
        -- make sure the artifact exists without linking it into this target.
        import("core.project.task")
        local root = os.projectdir()
        local runtime_dir = path.join(artifact_dir, "runtime")
        local lib = is_plat("mingw", "windows") and
                    path.join(runtime_dir, "qianqian.dll") or
                    path.join(runtime_dir, "libqianqian.so")
        if not os.isfile(lib) then
            task.run("build", {"qianqian_runtime"})
        end
        os.execv(target:targetfile(), {lib,
                 path.join(root, "corpus", "fixtures", "flac-16-44-stereo.flac")})
        return true
    end)
