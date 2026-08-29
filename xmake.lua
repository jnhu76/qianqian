set_project("qianqian")
set_version("0.1.0")
set_languages("c11")
set_config("buildir", "build/xmake")
add_rules("mode.debug", "mode.release")

local artifact_dir = path.join(os.projectdir(), "build", "artifacts")
local ffmpeg_manifest = path.join(os.projectdir(), "build", "ffmpeg-xmake", "manifest.json")

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
        if not os.isfile(ffmpeg_manifest) then
            return
        end

        local m = json.loadfile(ffmpeg_manifest)
        local srcroot = path.join(os.projectdir(), m.source_root)
        local buildroot = path.join(os.projectdir(), m.config_root)

        -- Generated config headers must win over the pristine source tree.
        target:add("includedirs", buildroot, srcroot, {public = true})

        for _, unit in ipairs(m.units) do
            local root = unit.origin == "generated" and buildroot or srcroot
            local source = path.join(root, unit.path)
            local flags = {}
            for _, flag in ipairs(unit.flags or {}) do
                flag = flag:gsub("@SRC@", function () return srcroot end)
                flag = flag:gsub("@BUILD@", function () return buildroot end)
                table.insert(flags, flag)
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
        if not os.isfile(ffmpeg_manifest) then
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
