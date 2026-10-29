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

-- Resolve the target recipe for THIS session, mirroring the fail-closed
-- matching in build/ffmpeg.lua: the JSON files under ffmpeg/targets/ are the
-- machine-readable target intent, Xmake only matches them. Returns the
-- matching recipe table or nil; it never raises — callers decide how a
-- missing recipe fails.
--
-- The json module is handed in by the caller: `import` is only available
-- inside action-scope callbacks (on_config / before_build), never at
-- description scope where these helpers are defined.
local session_recipe = function (target, json)
    local targets_dir = path.join(os.projectdir(), "ffmpeg", "targets")
    for _, recipe_file in ipairs(os.files(path.join(targets_dir, "*.json")) or {}) do
        local r = json.loadfile(recipe_file)
        local plats = (r.xmake or {}).plat or {}
        if type(plats) == "string" then plats = {plats} end
        local plat_ok = false
        for _, p in ipairs(plats) do
            if p == target:plat() then plat_ok = true end
        end
        if plat_ok and (r.xmake or {}).arch == target:arch() then
            return r
        end
    end
    return nil
end

-- "static, shared" statement of which artifact forms a recipe enables.
local enabled_forms = function (recipe)
    if not recipe then
        return "<none: no matching recipe>"
    end
    local artifacts = recipe.artifacts or {}
    local forms = {}
    for _, form in ipairs({"static", "shared"}) do
        if artifacts[form] == true then
            table.insert(forms, form)
        end
    end
    if #forms == 0 then
        return "none"
    end
    return table.concat(forms, ", ")
end

-- Artifact-policy gate shared by both artifact targets. Returns nil when the
-- form about to be built is enabled by the session's target recipe
-- (artifacts.<form> = true), else the fail-closed error text. The raise
-- happens in the callback scope — like build/ffmpeg.lua's
-- manifest_target_problem, helpers defined at description scope must not
-- call xmake action globals (raise/import): only the callback body is run in
-- the action-scope sandbox where they exist.
--
-- A raise from before_build fails exactly the offending build with a nonzero
-- exit; unrelated targets in the same session are unaffected, because
-- before_build runs per built target.
--
-- This is the fail-closed enforcement of the recipe artifact policy (#172,
-- confirmed defect D2 in #324): the windows-x86_64 recipe declares
-- artifacts.static=true / artifacts.shared=false, yet the songcore phony
-- unconditionally pulled songcore_shared into `xmake build songcore`, whose
-- DLL link died on the FFmpeg closure's undefined BCrypt* references
-- (libavutil/random_seed.c) after the static side had already produced a
-- partial artifact set.
local recipe_artifact_problem = function (target, form, json)
    local recipe = session_recipe(target, json)
    if not recipe then
        return format(
            "songcore_%s: no target recipe in ffmpeg/targets/ matches this " ..
            "session (plat '%s', arch '%s'). Target provenance is " ..
            "fail-closed, mirroring the FFmpeg closure gate.",
            form, target:plat(), target:arch())
    end
    local artifacts = recipe.artifacts or {}
    if artifacts[form] ~= true then
        return format(
            "songcore_%s is not supported by this session's target recipe " ..
            "'%s' (ffmpeg/targets/%s.json declares artifacts.%s=%s; this " ..
            "recipe enables: %s). The unsupported form is rejected here " ..
            "instead of being linked into a half-configured artifact.",
            form, recipe.id, recipe.id, form, tostring(artifacts[form]),
            enabled_forms(recipe))
    end
    return nil
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
    before_build(function (target)
        import("core.base.json")
        local problem = recipe_artifact_problem(target, "static", json)
        if problem then
            raise(problem)
        end
    end)

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
    before_build(function (target)
        import("core.base.json")
        local problem = recipe_artifact_problem(target, "shared", json)
        if problem then
            raise(problem)
        end
    end)

-- Build convenience: every artifact form the session's target recipe
-- enables, under the historical name. The dependency set is resolved in
-- on_config — it fires once per command after configuration and before the
-- build jobs are planned, has action-scope facilities available, and
-- dynamically added deps are honored by the build planner — from the SAME
-- recipe the per-target before_build gates above enforce, so the phony can
-- never drag an unsupported form into the build (the D2 defect). Static-first:
-- recipes that enable only static get exactly songcore_static here.
--
-- (on_load cannot host this: adding deps there forces a full project reload
-- whose sandbox lacks the action-scope environment — every later command,
-- `xmake f` included, dies on the reloaded load pass.)
--
-- When the session recipe enables no artifact form at all (or no recipe
-- matches this session), the phony depends on songcore_artifact_gate below,
-- whose before_build raises the precise reason. No raise may live here:
-- on_config also runs during plain `xmake f`, and a load/config-phase error
-- would poison every later command.
target("songcore")
    set_kind("phony")
    set_default(false)
    on_config(function (target)
        import("core.base.json")
        local recipe = session_recipe(target, json)
        local deps = {}
        if recipe then
            local artifacts = recipe.artifacts or {}
            if artifacts.static == true then
                table.insert(deps, "songcore_static")
            end
            if artifacts.shared == true then
                table.insert(deps, "songcore_shared")
            end
        end
        if #deps == 0 then
            target:add("deps", "songcore_artifact_gate")
        else
            for _, dep in ipairs(deps) do
                target:add("deps", dep)
            end
        end
    end)

-- Fail-closed terminal for sessions whose target recipe enables no SongCore
-- artifact form. Never reached on a healthy session (the phony only deps
-- this when no form is enabled); set_default(false) keeps it out of a bare
-- `xmake`.
target("songcore_artifact_gate")
    set_kind("phony")
    set_default(false)
    before_build(function (target)
        import("core.base.json")
        local recipe = session_recipe(target, json)
        if not recipe then
            raise(format(
                "songcore: no target recipe in ffmpeg/targets/ matches this " ..
                "session (plat '%s', arch '%s'); no SongCore artifact form " ..
                "can be built. Target provenance is fail-closed, mirroring " ..
                "the FFmpeg closure gate.",
                target:plat(), target:arch()))
        end
        raise(format(
            "songcore: no SongCore artifact form to build for this session " ..
            "(recipe '%s' enables: %s). songcore_artifact_gate is the " ..
            "fail-closed terminal the songcore phony depends on only when " ..
            "no form is enabled; build 'songcore' or 'songcore_static' " ..
            "instead.",
            recipe.id, enabled_forms(recipe)))
    end)
