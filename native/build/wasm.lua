-- ====================================================================
-- WASM target (independent session; the native default session above is
-- untouched).
--
--   WASI session:
--     xmake f -b build/xmake-wasi --wasm=wasi \
--         --av_manifest=build/ffmpeg-xmake-wasi/manifest.json
--     xmake build songcore_wasm
--
--   Emscripten session:
--     xmake f -b build/xmake-em --wasm=emscripten \
--         --av_manifest=build/ffmpeg-xmake-emscripten/manifest.json
--     xmake build songcore_wasm
--
-- WASM viability was verified against machine evidence (see docs/research/wasm.md
-- and bench/results/wasm-summary.json); native remains the shipping default.
-- ====================================================================

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

option("wasm")
    set_values(false, "wasi", "emscripten")
    set_default(false)
    set_showmenu(true)
    set_description("WASM target for this build session")

option("wasmsdk")
    set_default("")
    set_showmenu(true)
    set_description("Override wasi-sdk/emscripten SDK root (else QN_WASI_SDK/QN_EMSDK or ~/toolchains/wasm)")

local wasm_sdk_home = function ()
    return path.join(os.getenv("HOME"), "toolchains", "wasm")
end

toolchain("wasi-sdk")
    set_kind("standalone")
    on_load(function (toolchain)
        local sdk = get_config("wasmsdk")
        if not sdk or #sdk == 0 then sdk = os.getenv("QN_WASI_SDK") end
        if not sdk or #sdk == 0 then
            sdk = path.join(wasm_sdk_home(), "wasi-sdk-34.0")
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
            sdk = path.join(wasm_sdk_home(), "emsdk", "upstream", "emscripten")
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

-- Reactor link shape shared by every WASI guest module: no _start, callers
-- initialize via _initialize, linear memory exported for host-side PCM reads.
-- Pin explicit memory bounds (LLVM 23 derives starved ceilings for reactor
-- modules, which starves FFmpeg allocation); actual usage is measured.
-- Exporting __heap_base is REQUIRED for WAMR embedding — without it WAMR
-- places its app heap at __data_end and stomps guest .bss.
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
        add_files("$(projectdir)/native/src/songcore_ffmpeg.c")
        add_includedirs("$(projectdir)/native/include", {public = true})
        add_deps("songcore_static")

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
        add_files("$(projectdir)/native/src/songcore_ffmpeg.c", "$(projectdir)/native/src/wasm/songcore_wasm_bridge.c")
        add_includedirs("$(projectdir)/native/include", "$(projectdir)/native/src/wasm")
        add_deps("songcore_static")
        if get_config("wasm") == "wasi" then
            add_ldflags(wasi_reactor_ldflags(), {force = true})
        else
            -- Emscripten: keep the same exports addressable from JS glue.
            add_ldflags({
                "--no-entry", -- library-style module: init via emscripten runtime
                "-sERROR_ON_UNDEFINED_SYMBOLS=0", -- host imports come from the embedder
                "-sALLOW_MEMORY_GROWTH=1",
                "-sMODULARIZE=1",
                "-sEXPORT_NAME=createSongCore",
                "-sINVOKE_RUN=0",
                "-sEXPORTED_RUNTIME_METHODS=HEAPU8,HEAPF32",
                "-sEXPORTED_FUNCTIONS=[\"_song_wasm_abi_version\",\"_song_wasm_open\",\"_song_wasm_probe\",\"_song_wasm_audio_stream_count\",\"_song_wasm_audio_stream_info\",\"_song_wasm_select_stream\",\"_song_wasm_get_metadata\",\"_song_wasm_get_metadata_count\",\"_song_wasm_get_metadata_entry\",\"_song_wasm_get_artwork_count\",\"_song_wasm_get_artwork_item\",\"_song_wasm_read_pcm\",\"_song_wasm_seek\",\"_song_wasm_last_error\",\"_song_wasm_close\",\"_song_wasm_alloc\",\"_song_wasm_free\",\"_song_wasm_layout\",\"_malloc\",\"_free\"]",
                "-g1", -- keep export symbol names for the JS glue
                "-sENVIRONMENT=node,web",
            }, {force = true})
        end
end
