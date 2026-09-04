-- ====================================================================
-- PlayerEngine (Phase 1.5): native engine + deterministic NullAudioBackend.
--
-- C++17 internal implementation calling ONLY the frozen SongCore C ABI
-- (include/songcore.h). Test binaries link tests/player/fake_songcore.cpp
-- INSTEAD of the real songcore target: the engine calls the production ABI
-- and the fake replaces its symbols at link time — no test abstraction
-- leaks into the engine. player_core therefore depends on no songcore
-- target; production linkage pairs it with songcore_static.
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

option("player_san")
    set_default("none")
    set_values("none", "asan", "ubsan", "tsan")
    set_showmenu(true)
    set_description("Sanitizer for player targets (asan/ubsan/tsan)")

local player_san_flags = function ()
    local san = get_config("player_san")
    if san == "asan" then return {"-fsanitize=address", "-fno-omit-frame-pointer"} end
    if san == "ubsan" then return {"-fsanitize=undefined", "-fno-omit-frame-pointer"} end
    if san == "tsan" then return {"-fsanitize=thread", "-fno-omit-frame-pointer"} end
    return {}
end

local player_common = function ()
    set_languages("c++17")
    if is_plat("linux", "macosx", "android", "iphoneos") then
        add_syslinks("pthread")
    end
    on_load(function (target)
        local flags = player_san_flags()
        if #flags > 0 then
            target:add("cxflags", flags, {force = true})
            target:add("ldflags", flags, {force = true})
        end
    end)
end

-- Shared player-source wiring: like songcore_common(), the player sources
-- are compiled per consuming artifact — a shared target needs its own
-- -fPIC objects (xmake adds them automatically), so the runtime does not
-- link the non-PIC libplayer_core.a that test binaries use. Callers decide
-- the wasapi_renderer.cpp question themselves (remove_files is
-- target-global and would undo a later re-add).
local player_sources = function ()
    add_files("$(projectdir)/native/src/player/*.cpp")
    add_includedirs("$(projectdir)/native/include", "$(projectdir)/native/src/player", {public = true})
end

target("player_core")
    set_kind("static")
    set_default(false)
    set_targetdir(artifact_dir)
    player_sources()
    -- wasapi_renderer.cpp is compiled ONLY by the qianqian_runtime Windows
    -- flavor (its COM/syslink surface must not enter test link lines).
    remove_files("$(projectdir)/native/src/player/wasapi_renderer.cpp")
    player_common()

-- Native semantic + realtime-contract gates (ring unit/property/SPSC,
-- lifecycle/clock/EOF semantics, thread stress, realtime bounds, GAP
-- zero-fill, admission quiescence, overflow, snapshot coherence).
-- Binary exit code gates.
target("player_gates")
    set_kind("binary")
    set_default(false)
    set_targetdir(artifact_dir)
    add_files("$(projectdir)/native/tests/player/pcm_ring_test.cpp", "$(projectdir)/native/tests/player/engine_gates_test.cpp",
              "$(projectdir)/native/tests/player/thread_stress_test.cpp", "$(projectdir)/native/tests/player/gates_main.cpp",
              "$(projectdir)/native/tests/player/realtime_bounds_test.cpp", "$(projectdir)/native/tests/player/fake_songcore.cpp")
    add_deps("player_core")
    player_common()
    add_tests("default")

-- Tiny external consumer: a pure-C99 TU
-- including ONLY the product C ABI headers, compiled by a C compiler and
-- linked against libplayer_core + the test-only backend driver — proof the
-- product surface is self-contained, C++-leak-free, and ABI-linkable from a
-- foreign TU WITHOUT any audio-backend knowledge. Links the test-only
-- SongCore stand-in instead of real SongCore (link-time substitution, like
-- every other player test).
target("player_consumer_c")
    set_kind("binary")
    set_default(false)
    set_targetdir(artifact_dir)
    add_files("$(projectdir)/native/tests/player/consumer_c/main.c", "$(projectdir)/native/tests/player/fake_songcore.cpp",
              "$(projectdir)/native/tests/player/player_test_driver.cpp")
    add_includedirs("$(projectdir)/native/tests/player") -- stand-in "filesystem" header only
    add_deps("player_core")
    player_common()
    set_languages("c99", "c++17")
    add_tests("default")

-- Real-SongCore integration gate: the ONE
-- player test that links the REAL SongCore archive (not the stand-in) and
-- drives REAL corpus fixtures (FLAC + MP3) through host FILE* I/O. Proves
-- the frozen product semantics end to end: open -> play -> ENDED at the
-- real media duration, seek landing, stop -> READY @0, missing-file error
-- path. Drives the NullAudioBackend via the test-only backend driver.
target("player_real_songcore_smoke")
    set_kind("binary")
    set_default(false)
    set_targetdir(artifact_dir)
    add_files("$(projectdir)/native/tests/player/real_songcore_smoke.c",
              "$(projectdir)/native/tests/player/player_test_driver.cpp")
    add_deps("player_core", "songcore_static")
    player_common()
    set_languages("c99", "c++17")
    if is_plat("linux") or is_plat("macosx") then
        add_syslinks("m", "pthread")
    end
    if is_plat("mingw") then
        add_syslinks("bcrypt")
    end
    add_tests("default")
    on_test(function (target, opt)
        local root = os.projectdir()
        os.execv(target:targetfile(),
                 {path.join(root, "corpus", "fixtures", "flac-16-44-stereo.flac"),
                  path.join(root, "corpus", "fixtures", "mp3-short.mp3")})
        return true
    end)

-- ====================================================================
-- Qianqian runtime: the ONE application-facing dynamic library
-- (docs/architecture/platform-audio.md). Exports exactly the two frozen
-- C ABIs (songcore.h + player_engine.h); SongCore/FFmpeg/PlayerEngine
-- internals stay private — same hidden-visibility recipe as the audited
-- songcore.dll. On Windows the runtime flavor composes the WASAPI render
-- thread into pe_create/pe_destroy; on other platforms the runtime is the
-- engine-only ABI surface (no backend yet).
-- ====================================================================
target("qianqian_runtime")
    set_kind("shared")
    set_basename("qianqian") -- → libqianqian.so / qianqian.dll
    set_default(false)
    set_targetdir(path.join(artifact_dir, "runtime"))
    set_languages("c11", "c++17")
    -- Own PIC player objects + own songcore TU (SONGCORE_BUILD_SHARED):
    -- the songcore_static dep supplies only the FFmpeg closure archive;
    -- its own songcore_ffmpeg.c member is never pulled because the
    -- dllexport'd definitions here resolve every song_* reference.
    add_files("$(projectdir)/native/src/songcore_ffmpeg.c")
    player_sources()
    if is_plat("mingw", "windows") then
        -- WASAPI render-thread flavor (docs/architecture/platform-audio.md)
        add_files("$(projectdir)/native/src/player/wasapi_renderer.cpp")
        add_defines("QN_QIANQIAN_RUNTIME")
        add_syslinks("ole32")
        -- Keep the runtime dependency closure at system DLLs only (no
        -- libstdc++/libgcc side-by-side DLLs next to qianqian.dll). This
        -- xmake maps shared-link driver flags through shflags, not ldflags.
        add_shflags("-static-libgcc", "-static-libstdc++", {force = true})
        -- Keep the runtime dependency closure at system DLLs only: no
        -- libstdc++/libgcc side-by-side DLLs next to qianqian.dll.
        add_ldflags("-static-libgcc", "-static-libstdc++", {force = true})
        -- FFmpeg's av_random_bytes uses BCryptGenRandom on Windows
        add_syslinks("bcrypt")
    else
        remove_files("$(projectdir)/native/src/player/wasapi_renderer.cpp")
    end
    add_defines("SONGCORE_BUILD_SHARED", "PLAYER_ENGINE_BUILD_SHARED")
    set_symbols("hidden")
    if is_plat("mingw", "windows") then
        set_prefixname("") -- qianqian.dll (Windows convention), not libqianqian.dll
    end
    if is_plat("linux", "android") then
        add_ldflags("-Wl,--exclude-libs,ALL", {force = true})
    end
    if is_plat("linux", "macosx", "android", "iphoneos") then
        add_syslinks("m", "pthread")
    end
    add_deps("songcore_static")
