set_project("qianqian")
set_version("0.1.0")
set_languages("c11")
set_config("builddir", "build/xmake")
add_rules("mode.debug", "mode.release")

includes("native")
includes("integration")
