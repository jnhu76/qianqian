set_project("qianqian")
set_version("0.1.0")
set_languages("c11")
set_config("builddir", "build/xmake")
add_rules("mode.debug", "mode.release")

-- E09 WASM sessions: every target in the session (including the FFmpeg
-- closure replay in qianqian_av) compiles with the guest toolchain. The
-- native default session stays untouched.
if get_config("wasm") == "wasi" then
    set_toolchains("wasi-sdk")
elseif get_config("wasm") == "emscripten" then
    set_toolchains("emcc")
end

-- Which frozen compile closure qianqian_av replays. Defaults to the
-- canonical import manifest; minimization experiments project filtered
-- manifests into build/minimize/<stage>/ and point this option at them.
option("av_manifest")
    set_default("build/ffmpeg-xmake/manifest.json")
    set_showmenu(true)
    set_description("FFmpeg compile-closure manifest to replay")

-- S4 experiment: garbage-collect unreferenced sections in the final binary.
option("gc_sections")
    set_default(false)
    set_showmenu(true)
    set_description("Link qn_pcm_dump with -Wl,--gc-sections")

-- S5 experiment: link-time optimization for the final binary.
option("lto")
    set_default(false)
    set_showmenu(true)
    set_description("Link qn_pcm_dump with -flto")

local artifact_dir = path.join(os.projectdir(), "build", "artifacts")
local ffmpeg_manifest = function ()
    return path.join(os.projectdir(), get_config("av_manifest"))
end

-- Import is intentionally separate from normal builds. It may invoke FFmpeg's
-- configure/Make once as an upstream oracle, then freezes the exact compile
-- closure in build/ffmpeg-xmake/manifest.json. Normal xmake builds never call
-- FFmpeg Makefiles.
task("ffmpeg-import")
    set_menu {
        usage = "xmake ffmpeg-import",
        description = "Resolve the minimal FFmpeg source closure for Xmake replay"
    }
    on_run(function ()
        import("lib.detect.find_tool")
        local python = find_tool("python3") or find_tool("python")
        assert(python, "python3/python is required for ffmpeg-import")
        os.execv(python.program, {path.join(os.projectdir(), "tools", "ffmpeg_import.py")})
    end)

target("qianqian_av")
    set_kind("static")
    set_default(false)
    set_targetdir(artifact_dir)
    on_load(function (target)
        import("core.base.json")
        -- Do not fail project loading here: `xmake ffmpeg-import` must be able
        -- to run before the manifest exists. before_build below owns the gate.
        local manifest_path = ffmpeg_manifest()
        if not os.isfile(manifest_path) then
            return
        end

        local m = json.loadfile(manifest_path)
        local srcroot = path.join(os.projectdir(), m.source_root)
        local buildroot = path.join(os.projectdir(), m.config_root)

        -- Generated config headers must win over the pristine source tree.
        target:add("includedirs", buildroot, srcroot, {public = true})

        -- E09 WASM sessions replay the closure at -Os (shipping shape, same
        -- caliber as the E08 -Os ladder floors). The manifest keeps the
        -- oracle's own flags untouched; codegen level is a build decision.
        local optimize_flags = function (flags)
            local out = {}
            for _, flag in ipairs(flags) do
                if flag:match("^%-O") then
                    flag = "-Os"
                end
                table.insert(out, flag)
            end
            return out
        end
        local wasm_session = (get_config("wasm") ~= nil and get_config("wasm") ~= false)
            or (get_config("e09") == true) -- native twin parity with the -Os WASM guests

        for _, unit in ipairs(m.units) do
            local root = unit.origin == "generated" and buildroot or srcroot
            local source = path.join(root, unit.path)
            local flags = {}
            for _, flag in ipairs(unit.flags or {}) do
                flag = flag:gsub("@SRC@", function () return srcroot end)
                flag = flag:gsub("@BUILD@", function () return buildroot end)
                table.insert(flags, flag)
            end
            if wasm_session then
                flags = optimize_flags(flags)
            end
            if #flags > 0 then
                local force = {}
                force[unit.flag_kind or "cflags"] = flags
                target:add("files", source, {force = force})
            else
                target:add("files", source)
            end
        end
    end)
    before_build(function ()
        if not os.isfile(ffmpeg_manifest()) then
            raise("FFmpeg source closure is missing. Run `xmake ffmpeg-import` first, then rerun xmake.")
        end
    end)

target("songcore")
    set_kind("static")
    set_default(false)
    set_targetdir(artifact_dir)
    add_files("src/songcore_ffmpeg.c")
    add_includedirs("include", {public = true})
    add_deps("qianqian_av")

target("qn_pcm_dump")
    set_kind("binary")
    set_default(false)
    set_targetdir(artifact_dir)
    add_files("tools/qn_pcm_dump.c")
    add_deps("songcore")
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

-- E10-C0: bench-only libavfilter capability probe. Links the FFmpeg closure
-- replayed by qianqian_av (whatever --av_manifest points at) and exercises
-- the filters that closure enables: registration presence/absence,
-- negotiated formats, correctness smokes, graph lifecycle. Bench-only: no
-- production semantics, no decode path.
target("qn_avfilter_cap_probe")
    set_kind("binary")
    set_default(false)
    set_targetdir(artifact_dir)
    add_files("bench/pcm/avfilter/qn_avfilter_cap_probe.c")
    add_deps("qianqian_av", "songcore")
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
        -- Optional link map for the C0 live-bytes ledger; opt-in via env so
        -- normal builds are untouched.
        local link_map = os.getenv("QN_LINK_MAP")
        if link_map and #link_map > 0 then
            target:add("ldflags", "-Wl,-Map=" .. link_map)
        end
        -- avf-c0 (codec-only closure) has no libavfilter to link: the probe
        -- compiles its fail-closed stub backend instead.
        if os.getenv("QN_PROBE_NO_AVFILTER") then
            target:add("defines", "QN_PROBE_NO_AVFILTER")
        end
    end)

-- ====================================================================
-- E09 — WASM total-cost experiment (independent sessions; the native
-- default session above is untouched).
--
--   WASI session:
--     xmake f -b build/xmake-wasi --wasm=wasi \
--         --av_manifest=build/ffmpeg-xmake-wasi/manifest.json
--     xmake build songcore_wasm qn_guest_wasm qn_pb_wasm
--
--   Emscripten session:
--     xmake f -b build/xmake-em --wasm=emscripten \
--         --av_manifest=build/ffmpeg-xmake-emscripten/manifest.json
--
--   Runner session (host binaries for the runtime ladder):
--     xmake f --e09
--     xmake build qn_native_runner qn_wamr_runner qn_wamr_aot_runner \
--         qn_wasm3_runner qn_wasmtime_runner qn_pb_native
-- ====================================================================

option("wasm")
    set_values(false, "wasi", "emscripten")
    set_default(false)
    set_showmenu(true)
    set_description("E09 WASM target for this build session")

option("wasmsdk")
    set_default("")
    set_showmenu(true)
    set_description("Override wasi-sdk/emscripten SDK root (else QN_WASI_SDK/QN_EMSDK or ~/toolchains/e09)")

option("e09")
    set_default(false)
    set_showmenu(true)
    set_description("Build E09 host-side runtime-ladder runners (native session)")

option("e09_home")
    set_default("")
    set_showmenu(true)
    set_description("E09 external toolchain home (else QN_E09_HOME or ~/toolchains/e09)")

local e09_home = function ()
    local h = get_config("e09_home")
    if h and #h > 0 then return h end
    return os.getenv("QN_E09_HOME") or path.join(os.getenv("HOME"), "toolchains", "e09")
end

toolchain("wasi-sdk")
    set_kind("standalone")
    on_load(function (toolchain)
        local sdk = get_config("wasmsdk")
        if not sdk or #sdk == 0 then sdk = os.getenv("QN_WASI_SDK") end
        if not sdk or #sdk == 0 then
            sdk = path.join(os.getenv("HOME"), "toolchains", "e09", "wasi-sdk-34.0")
        end
        local bin = path.join(sdk, "bin")
        local sysroot = path.join(sdk, "share", "wasi-sysroot")
        local triple = "--target=wasm32-wasip1"
        toolchain:set("toolset", "cc", path.join(bin, "clang"))
        toolchain:set("toolset", "cxx", path.join(bin, "clang++"))
        toolchain:set("toolset", "ld", path.join(bin, "clang"))
        toolchain:set("toolset", "sh", path.join(bin, "clang"))
        toolchain:set("toolset", "as", path.join(bin, "clang"))
        toolchain:set("toolset", "ar", path.join(bin, "llvm-ar"))
        toolchain:set("toolset", "strip", path.join(bin, "llvm-strip"))
        toolchain:set("toolset", "nm", path.join(bin, "llvm-nm"))
        toolchain:add("cxflags", triple, "--sysroot=" .. sysroot,
                     "-D_WASI_EMULATED_PROCESS_CLOCKS", {force = true})
        toolchain:add("asflags", triple, "--sysroot=" .. sysroot, {force = true})
        toolchain:add("ldflags", triple, "--sysroot=" .. sysroot,
                     "-D_WASI_EMULATED_PROCESS_CLOCKS",
                     "-lwasi-emulated-process-clocks", {force = true})
        toolchain:add("shflags", triple, "--sysroot=" .. sysroot, {force = true})
    end)

toolchain("emcc")
    set_kind("standalone")
    on_load(function (toolchain)
        local sdk = get_config("wasmsdk")
        if not sdk or #sdk == 0 then sdk = os.getenv("QN_EMSDK") end
        if not sdk or #sdk == 0 then
            sdk = path.join(os.getenv("HOME"), "toolchains", "e09", "emsdk", "upstream", "emscripten")
        end
        toolchain:set("toolset", "cc", path.join(sdk, "emcc"))
        toolchain:set("toolset", "cxx", path.join(sdk, "em++"))
        toolchain:set("toolset", "ld", path.join(sdk, "emcc"))
        toolchain:set("toolset", "sh", path.join(sdk, "emcc"))
        toolchain:set("toolset", "ar", path.join(sdk, "emar"))
        toolchain:set("toolset", "ranlib", path.join(sdk, "emranlib"))
        toolchain:set("toolset", "strip", path.join(sdk, "emstrip"))
    end)

-- WASM guest artifacts live in their own artifact subdir.
local wasm_artifact_dir = path.join(artifact_dir, "wasm")

-- Reactor link shape shared by every WASI guest module: no _start, callers
-- initialize via _initialize, linear memory exported for host-side PCM reads.
-- E09: wasi-sdk/LLVM 23 derives a tiny memory ceiling for reactor modules
-- (min 0 / max ~46 pages observed), which starves FFmpeg allocation. Pin
-- explicit bounds instead; actual usage is measured in the memory audit.
-- E09: exporting __heap_base is REQUIRED for WAMR embedding — without it
-- WAMR 2.4.5 places its app heap at __data_end and stomps guest .bss (the
-- wasi-libc __tls_init guard lives there), making _initialize trap
-- "unreachable". With the export, WAMR inserts its heap before __heap_base
-- and rewrites the global; guest .bss stays intact.
local wasi_reactor_ldflags = function ()
    return {
        "-mexec-model=reactor",
        "-Wl,--export-memory",
        "-Wl,--export-if-defined=__heap_base",
        "-Wl,-z,stack-size=1048576",
        "-Wl,--initial-memory=16777216",
        "-Wl,--max-memory=268435456",
    }
end

if get_config("wasm") then
    target("songcore_wasm_lib")
        set_kind("static")
        set_default(false)
        set_targetdir(wasm_artifact_dir)
        set_optimize("smallest")
        add_files("src/songcore_ffmpeg.c")
        add_includedirs("include", {public = true})
        add_deps("qianqian_av")

    target("songcore_wasm")
        set_kind("binary")
        set_default(false)
        set_targetdir(wasm_artifact_dir)
        set_basename("SongCore")
        set_extension(".wasm")
        if get_config("wasm") == "emscripten" then
            set_extension(".js") -- MODULARIZE glue + .wasm side by side
        end
        set_optimize("smallest")
        add_files("src/songcore_ffmpeg.c", "src/wasm/songcore_wasm_bridge.c")
        add_includedirs("include", "src/wasm")
        add_deps("qianqian_av")
        if get_config("wasm") == "wasi" then
            add_ldflags(wasi_reactor_ldflags(), {force = true})
        else
            -- Emscripten: keep the same exports addressable from JS glue.
            add_ldflags({
                "--no-entry", -- library-style module: init via emscripten runtime
                "-sERROR_ON_UNDEFINED_SYMBOLS=0", -- qianqian_host imports come from the embedder
                "-sALLOW_MEMORY_GROWTH=1",
                "-sMODULARIZE=1",
                "-sEXPORT_NAME=createSongCore",
                "-sINVOKE_RUN=0",
                "-sEXPORTED_RUNTIME_METHODS=HEAPU8,HEAPF32",
                "-sEXPORTED_FUNCTIONS=[\"_song_wasm_open\",\"_song_wasm_probe\",\"_song_wasm_read_pcm\",\"_song_wasm_seek\",\"_song_wasm_close\",\"_malloc\",\"_free\"]",
                "-g1", -- keep export symbol names for the JS glue (E09-emscripten-1:
                       -- the door IMPORTS are wired by type signature in
                       -- tools/wasm_em_node_harness.mjs because emscripten minifies
                       -- names and the linker reorders import slots)
                "-sENVIRONMENT=node,web",
            }, {force = true})
        if get_config("wasm") == "wasi" then
            -- E09: artifact stays PRISTINE toolchain output. The WAMR-only
            -- init-guard workaround (E09-WAMR-1) is applied to *copies* by
            -- tools/wasm_prepare_artifacts.py so Wasmtime/wasm3/Node always
            -- see unmodified bytes.
        end

        end

    target("qn_guest_wasm")
        set_kind("binary")
        set_default(false)
        set_targetdir(wasm_artifact_dir)
        set_basename("qn_guest_bench")
        set_extension(".wasm")
        if get_config("wasm") == "emscripten" then
            set_extension(".js")
        end
        set_optimize("smallest")
        add_files("bench/wasm/qn_guest_bench.c")
        add_includedirs("src/wasm")
        add_deps("songcore_wasm_lib")
        if get_config("wasm") == "wasi" then
            add_ldflags(wasi_reactor_ldflags(), {force = true})
        else
            add_ldflags({
                "--no-entry", -- library-style module: init via emscripten runtime
                "-sERROR_ON_UNDEFINED_SYMBOLS=0", -- qianqian_host imports come from the embedder
                "-sALLOW_MEMORY_GROWTH=1",
                "-sMODULARIZE=1",
                "-sEXPORT_NAME=createQnGuestBench",
                "-sINVOKE_RUN=0",
                "-sEXPORTED_RUNTIME_METHODS=HEAPU8,HEAPF32",
                "-sEXPORTED_FUNCTIONS=[\"_bench_bind\",\"_bench_correct\",\"_bench_bench\",\"_bench_lifecycle\",\"_bench_pcm_prepare\",\"_bench_pcm_len\",\"_bench_pcm_ptr\",\"_bench_pcm_channels\",\"_bench_pcm_rate\",\"_bench_pcm_pull\",\"_bench_pcm_reset\",\"_bench_stage_alloc\",\"_bench_mem_pages\",\"_malloc\",\"_free\"]",
                "-g1", -- keep export symbol names for the JS glue (E09-emscripten-1:
                       -- the door IMPORTS are wired by type signature in
                       -- tools/wasm_em_node_harness.mjs because emscripten minifies
                       -- names and the linker reorders import slots)
                "-sENVIRONMENT=node,web",
            }, {force = true})
        if get_config("wasm") == "wasi" then
            -- E09: artifact stays PRISTINE toolchain output. The WAMR-only
            -- init-guard workaround (E09-WAMR-1) is applied to *copies* by
            -- tools/wasm_prepare_artifacts.py so Wasmtime/wasm3/Node always
            -- see unmodified bytes.
        end

        end

    target("qn_guest_wasm_cmd")
        set_kind("binary")
        set_default(false)
        set_targetdir(wasm_artifact_dir)
        set_basename("qn_guest_bench_cmd")
        set_extension(".wasm")
        set_optimize("smallest")
        add_files("bench/wasm/qn_guest_bench.c")
        add_includedirs("src/wasm")
        add_deps("songcore_wasm_lib")
        add_defines("QN_GUEST_COMMAND")
        if get_config("wasm") == "wasi" then
            -- command model (_start instead of _initialize); same explicit
            -- memory bounds -- LLVM 23 derives starved ceilings otherwise
            add_ldflags({
                "-Wl,--export-memory",
                "-Wl,--export-if-defined=__heap_base",
                "-Wl,-z,stack-size=1048576",
                "-Wl,--initial-memory=16777216",
                "-Wl,--max-memory=268435456",
            }, {force = true})
        end
        -- pristine toolchain output; WAMR-only guard workaround lives in
        -- tools/wasm_prepare_artifacts.py (applied to copies, never here)

    target("qn_pb_wasm")
        set_kind("binary")
        set_default(false)
        set_targetdir(wasm_artifact_dir)
        set_basename("qn_pb_guest")
        set_extension(".wasm")
        if get_config("wasm") == "emscripten" then
            set_extension(".js")
        end
        set_optimize("smallest")
        add_files("bench/wasm/qn_pb_guest.c")
        add_includedirs("src/wasm")
        if get_config("wasm") == "wasi" then
            add_ldflags(wasi_reactor_ldflags(), {force = true})
        else
            add_ldflags({
                "--no-entry", -- library-style module: init via emscripten runtime
                "-sERROR_ON_UNDEFINED_SYMBOLS=0", -- qianqian_host imports come from the embedder
                "-sALLOW_MEMORY_GROWTH=1",
                "-sMODULARIZE=1",
                "-sEXPORT_NAME=createQnPbGuest",
                "-sINVOKE_RUN=0",
                "-sEXPORTED_RUNTIME_METHODS=HEAPU8,HEAPF32",
                "-sEXPORTED_FUNCTIONS=[\"_pb_fill\",\"_pb_ptr\",\"_pb_pull\",\"_pb_reset\",\"_pb_len\",\"_pb_stage_alloc\",\"_malloc\",\"_free\"]",
                "-g1", -- keep export symbol names for the JS glue (E09-emscripten-1:
                       -- the door IMPORTS are wired by type signature in
                       -- tools/wasm_em_node_harness.mjs because emscripten minifies
                       -- names and the linker reorders import slots)
                "-sENVIRONMENT=node,web",
            }, {force = true})
        if get_config("wasm") == "wasi" then
            -- E09: artifact stays PRISTINE toolchain output. The WAMR-only
            -- init-guard workaround (E09-WAMR-1) is applied to *copies* by
            -- tools/wasm_prepare_artifacts.py so Wasmtime/wasm3/Node always
            -- see unmodified bytes.
        end

        end
end

-- Host-side runtime ladder (native session only).
if get_config("e09") then
    local wamr_root = path.join(e09_home(), "wasm-micro-runtime-WAMR-2.4.5")
    local wamr_libdir = path.join(wamr_root, "product-mini", "platforms", "linux", "build-e09")
    local wasm3_root = path.join(e09_home(), "wasm3-2b4a5fa4c35def3552a9283c4f7aab39a93f33e9")
    local wasmtime_root = path.join(e09_home(), "wasmtime")

    target("qn_native_runner")
        set_kind("binary")
        set_default(false)
        set_targetdir(artifact_dir)
        set_optimize("smallest") -- guest harness parity with the WASM guests
        set_symbols("debug")
        set_strip("none")
        add_files("bench/wasm/qn_guest_bench.c", "tools/wasm/qn_host_file.c",
                  "tools/wasm/qn_runner_common.c") -- host-side sha/timing helpers
        add_includedirs("src/wasm", "tools/wasm")
        add_defines("QN_GUEST_NATIVE")
        add_deps("songcore")
        if is_plat("linux") or is_plat("macosx") then
            add_syslinks("m", "pthread")
        end

    target("qn_wamr_runner")
        set_kind("binary")
        set_default(false)
        set_targetdir(artifact_dir)
        set_optimize("faster") -- host-side runner code; guest speed is runtime-owned
        set_symbols("debug")
        set_strip("none") -- profilation needs iwasm symbols (E09 perf attribution)
        add_files("tools/wasm/qn_wamr_runner.c", "tools/wasm/qn_runner_common.c")
        add_includedirs("tools/wasm", path.join(wamr_root, "core", "iwasm", "include"))
        add_linkdirs(wamr_libdir)
        add_links("iwasm")
        if is_plat("linux") then
            add_syslinks("pthread", "dl", "m")
        end

    target("qn_wamr_aot_runner")
        set_kind("binary")
        set_default(false)
        set_targetdir(artifact_dir)
        set_optimize("faster")
        set_symbols("debug")
        set_strip("none")        add_files("tools/wasm/qn_wamr_runner.c", "tools/wasm/qn_runner_common.c")
        add_includedirs("tools/wasm", path.join(wamr_root, "core", "iwasm", "include"))
        add_linkdirs(wamr_libdir)
        add_links("iwasm")
        add_defines("QN_WAMR_AOT_RUNNER=1")
        if is_plat("linux") then
            add_syslinks("pthread", "dl", "m")
        end

    target("qn_wasm3_runner")
        set_kind("binary")
        set_default(false)
        set_targetdir(artifact_dir)
        set_optimize("faster")
        set_symbols("debug")
        set_strip("none")        add_files("tools/wasm/qn_wasm3_runner.c", "tools/wasm/qn_runner_common.c")
        add_files(path.join(wasm3_root, "source", "*.c"))
        remove_files(path.join(wasm3_root, "source", "m3_api_uvwasi.c"),
                     path.join(wasm3_root, "source", "m3_api_wasi.c"),
                     path.join(wasm3_root, "source", "m3_api_meta_wasi.c"),
                     path.join(wasm3_root, "source", "m3_api_libc.c"),
                     path.join(wasm3_root, "source", "m3_api_tracer.c"))
        add_includedirs("tools/wasm", path.join(wasm3_root, "source"))
        add_defines("d_m3HasWASI=0")
        if is_plat("linux") then
            add_syslinks("pthread", "dl", "m")
        end

    target("qn_wasmtime_runner")
        set_kind("binary")
        set_default(false)
        set_targetdir(artifact_dir)
        set_optimize("faster")
        set_symbols("debug")
        set_strip("none")        add_files("tools/wasm/qn_wasmtime_runner.c", "tools/wasm/qn_runner_common.c")
        add_includedirs("tools/wasm", wasmtime_root .. "/include")
        add_ldflags(wasmtime_root .. "/lib/libwasmtime.a", {force = true})
        if is_plat("linux") then
            add_syslinks("pthread", "dl", "m")
        end

    -- §14 PCM-copy microbenchmark, native side of the same protocol.
    target("qn_pb_native")
        set_kind("binary")
        set_default(false)
        set_targetdir(artifact_dir)
        set_optimize("faster")
        set_symbols("debug")
        set_strip("none")        add_files("tools/wasm/qn_pb_native.c", "tools/wasm/qn_runner_common.c")
        add_includedirs("tools/wasm")
        if is_plat("linux") then
            add_syslinks("m")
        end
end
