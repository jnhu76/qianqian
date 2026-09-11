-- SongCore ownership module: SongCore build (static/shared/phony).

local artifact_dir = path.join(os.projectdir(), "build", "artifacts")

-- One conceptual native library (songcore), two artifact kinds. The public
-- ABI is include/songcore.h (ABI v1); the target names below are build
-- internals and never appear in the ABI. Shared build logic lives in
-- songcore_common() — the Xmake-recommended way to avoid duplicating the
-- definition across static/shared targets.
local songcore_common = function (dep_av)
    add_files("$(projectdir)/src/songcore_ffmpeg.c")
    add_includedirs("$(projectdir)/include", {public = true})
    if dep_av then
        add_deps("qianqian_av")
    end
    if is_plat("linux", "macosx", "android", "iphoneos") then
        add_syslinks("m", "pthread")
    end
end

target("songcore_static")
    set_kind("static")
    set_basename("songcore") -- → libsongcore.a / songcore.lib
    set_default(false)
    set_targetdir(artifact_dir)
    -- One self-contained static consumer artifact: the FFmpeg closure
    -- (qianqian_av) is merged into libsongcore.a by Xmake's supported
    -- merge policy (ar / lib.exe, cross-toolchain safe), so external
    -- static consumers link exactly one Qianqian archive plus system
    -- libraries — no separate internal archive in the documented link
    -- line. Linkers still drop unreferenced FFmpeg members from the
    -- merged archive member-wise, so test binaries keep their size.
    set_policy("build.merge_archive", true)
    songcore_common(true)

target("songcore_shared")
    set_kind("shared")
    set_basename("songcore") -- → libsongcore.so / songcore.dll
    set_default(false)
    -- Own subdir: a co-located libsongcore.so would win the linker's -l
    -- search over the static archive and silently flip every test binary
    -- to a dynamic dependency.
    set_targetdir(path.join(artifact_dir, "shared"))
    add_defines("SONGCORE_BUILD_SHARED")
    -- Hide by default; only SONGCORE_API declarations stay exported. The
    -- statically-linked FFmpeg closure is forced local at link time on ELF.
    set_symbols("hidden")
    if is_plat("linux", "android") then
        add_ldflags("-Wl,--exclude-libs,ALL", {force = true})
    end
    -- The shared artifact links the SAME merged archive the static
    -- consumers get. Depending on qianqian_av directly is wrong under
    -- build.merge_archive: xmake then treats the closure as "merged into
    -- songcore_static" and drops it from this target's link line, which
    -- ELF silently accepts as undefined references (hollow artifact) and
    -- PE rejects at DLL link time.
    songcore_common(false)
    add_deps("songcore_static")

-- Build convenience: both artifact kinds under the historical name.
target("songcore")
    set_kind("phony")
    set_default(false)
    add_deps("songcore_static", "songcore_shared")
