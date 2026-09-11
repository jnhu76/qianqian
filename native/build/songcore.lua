-- SongCore ownership module: SongCore build (static/shared/phony), plus the
-- SongCore-scoped build/test instruments (qn_pcm_dump, songcore_probe,
-- dsp_cap_probe) and the size options only those instruments consume.

-- The linux/macos default session owns build/artifacts directly (historical
-- layout). A mingw/windows session redirects every artifact into a platform
-- subdir: sharing one dir would let a mingw libsongcore.a silently replace
-- the linux archive (and vice versa) across sessions — the same trap the
-- wasm session avoids with artifacts/wasm below.
local artifact_dir = path.join(os.projectdir(), "build", "artifacts")
if is_plat("mingw", "windows") then
    artifact_dir = path.join(artifact_dir, "windows-mingw-x86_64")
end
-- WASM sessions archive their guest closure here as well; sharing the
-- native artifact path would let a wasm-format libsongcore.a silently
-- overwrite the native archive (and vice versa) across sessions.
local wasm_artifact_dir = path.join(artifact_dir, "wasm")
-- Same resolution as native/build/ffmpeg.lua (single source of truth: the
-- av_manifest option default); recomputed here for songcore_probe's
-- test-closure identity defines.
local ffmpeg_manifest = function ()
    return path.join(os.projectdir(), get_config("av_manifest"))
end

-- Product build controls: section GC and link-time optimization for the
-- final artifact size.
option("gc_sections")
    set_default(false)
    set_showmenu(true)
    set_description("Link with -Wl,--gc-sections (section garbage collection)")

option("lto")
    set_default(false)
    set_showmenu(true)
    set_description("Link with -flto (link-time optimization)")

-- One conceptual native library (songcore), two artifact kinds. The public
-- ABI is include/songcore.h (ABI v1); the target names below are build
-- internals and never appear in the ABI. Shared build logic lives in
-- songcore_common() — the Xmake-recommended way to avoid duplicating the
-- definition across static/shared targets.
local songcore_common = function (dep_av)
    add_files("$(projectdir)/native/src/songcore_ffmpeg.c")
    add_includedirs("$(projectdir)/native/include", {public = true})
    if dep_av then
        add_deps("qianqian_av")
    end
    if is_plat("linux", "macosx", "android", "iphoneos") then
        add_syslinks("m", "pthread")
    end
    if is_plat("mingw", "windows") then
        -- FFmpeg's av_random_bytes uses BCryptGenRandom on Windows
        add_syslinks("bcrypt")
    end
end

target("songcore_static")
    set_kind("static")
    set_basename("songcore") -- → libsongcore.a / songcore.lib
    set_default(false)
    set_targetdir(get_config("wasm") and wasm_artifact_dir or artifact_dir)
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
    -- to a dynamic dependency (same subdir convention as artifacts/wasm/).
    set_targetdir(path.join(artifact_dir, "shared"))
    add_defines("SONGCORE_BUILD_SHARED")
    -- Hide by default; only SONGCORE_API declarations stay exported. The
    -- statically-linked FFmpeg closure is forced local at link time on ELF
    -- (no Mach-O equivalent yet — macos-arm64 recipe notes this).
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

target("qn_pcm_dump")
    set_kind("binary")
    set_default(false)
    set_targetdir(artifact_dir)
    add_files("$(projectdir)/tools/qn_pcm_dump.c")
    add_deps("songcore_static")
    if is_plat("linux") or is_plat("macosx") then
        add_syslinks("m", "pthread")
    end
    if is_plat("mingw") then
        -- FFmpeg's av_random_bytes uses BCryptGenRandom on Windows
        add_syslinks("bcrypt")
    end
    on_load(function (target)
        if get_config("gc_sections") then
            target:add("ldflags", "-Wl,--gc-sections")
        end
        if get_config("lto") then
            target:add("ldflags", "-flto=auto")
        end
    end)

-- SongCore ABI v1 machine-test instrument (test-only, one JSON per run).
target("songcore_probe")
    set_kind("binary")
    set_default(false)
    set_targetdir(artifact_dir)
    add_files("$(projectdir)/native/tests/songcore/songcore_probe.c")
    add_deps("songcore_static")
    if is_plat("linux") or is_plat("macosx") then
        add_syslinks("m", "pthread")
    end
    if is_plat("mingw") then
        add_syslinks("bcrypt")
    end
    -- Test-closure provenance: embed the canonical identity fields of the
    -- replayed FFmpeg manifest (written by tools/ffmpeg_profile_import.py)
    -- so native/tests/songcore/regression.py can verify this binary was
    -- built from
    -- the closure it expects — a codec-base-built probe must fail closed
    -- before any corpus case. No manifest here defers to qianqian_av's
    -- before_build gate; the probe then reports "unknown" and the
    -- regression preflight rejects it.
    on_load(function (target)
        import("core.base.json")
        local manifest_path = ffmpeg_manifest()
        if not os.isfile(manifest_path) then
            return
        end
        local m = json.loadfile(manifest_path)
        local define = function (name, value)
            target:add("defines", format("%s=\"%s\"", name, value or "unknown"))
        end
        define("QN_TEST_AV_PROFILE", m.profile)
        define("QN_TEST_AV_PROFILE_SHA256", m.profile_sha256)
        define("QN_TEST_AV_TARGET", (m.target or {}).id)
        define("QN_TEST_AV_FFMPEG_SOURCE_SHA256", m.ffmpeg_source_sha256)
    end)
    -- `xmake test`: full SongCore regression + fail-closed --check, then
    -- the external-consumer gates (shared export audit, ctypes decode
    -- consumer, static archive consumer). os.execv raises on nonzero
    -- exit, so a gate failure fails the test.
    add_tests("default")
    on_test(function (target, opt)
        import("lib.detect.find_tool")
        local python = find_tool("python3") or find_tool("python")
        if not python then
            return false, "python3 is required for the SongCore regression"
        end
        local root = os.projectdir()
        local out = path.join(root, "bench", "results", "songcore-v1")
        local script = path.join(root, "native", "tests", "songcore", "regression.py")
        local consumers = path.join(root, "native", "tests", "songcore", "consumers.py")
        os.execv(python.program, {script, "--out", out})
        os.execv(python.program, {script, "--check", "--out", out})
        os.execv(python.program, {consumers, "--out", "--core"})
        os.execv(python.program, {consumers, "--check", "--core"})
        return true
    end)

-- DSP capability probe (test-only): links the FFmpeg closure replayed by
-- qianqian_av and exercises the filters that closure enables — registration
-- presence/absence, negotiated formats, correctness smokes, graph lifecycle.
-- Used by native/tests/songcore/dsp_src.py and by tools/dsp_closure.py for the
-- capability-driven libavfilter closure ladder.
target("dsp_cap_probe")
    set_kind("binary")
    set_default(false)
    set_targetdir(artifact_dir)
    add_files("$(projectdir)/native/tests/songcore/dsp_cap_probe.c")
    -- build.merge_archive: the closure (qianqian_av) lives inside the merged
    -- songcore archive; depending on qianqian_av directly would be silently
    -- dropped from the link line.
    add_deps("songcore_static")
    if is_plat("linux") or is_plat("macosx") then
        add_syslinks("m", "pthread")
    end
    on_load(function (target)
        if get_config("gc_sections") then
            target:add("ldflags", "-Wl,--gc-sections")
        end
        if get_config("lto") then
            target:add("cflags", "-flto")
            target:add("ldflags", "-flto=auto")
        end
        -- Optional link map for the DSP live-bytes ledger; opt-in via env so
        -- normal builds are untouched. (Restored: the productize/rename
        -- commit 1b4d539 dropped this block from the old xmake.lua probe
        -- target while keeping its QN_PROBE_NO_AVFILTER sibling, which left
        -- tools/dsp_closure.py's QN_LINK_MAP producer without a consumer and
        -- the live-bytes ledger unreproducible.)
        local link_map = os.getenv("QN_LINK_MAP")
        if link_map and #link_map > 0 then
            target:add("ldflags", "-Wl,-Map=" .. link_map)
        end
        -- A codec-only closure (no libavfilter) has nothing to link: the
        -- probe compiles its fail-closed stub backend instead.
        if os.getenv("QN_PROBE_NO_AVFILTER") then
            target:add("defines", "QN_PROBE_NO_AVFILTER")
        end
    end)
