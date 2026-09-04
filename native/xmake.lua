-- Native build router: native-wide session configuration + ownership-scoped
-- build modules. Global configuration in this file applies to every module
-- included below; sibling scopes (integration/) do not inherit it.

-- WASM sessions: every target in the session (including the FFmpeg closure
-- replay in qianqian_av) compiles with the guest toolchain. The native
-- default session stays untouched.
if get_config("wasm") == "wasi" then
    set_toolchains("wasi-sdk")
elseif get_config("wasm") == "emscripten" then
    set_toolchains("emcc")
end

includes("build/ffmpeg.lua")
includes("build/songcore.lua")
includes("build/player.lua")
includes("build/wasm.lua")
