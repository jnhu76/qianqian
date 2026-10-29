-- Description-scope probe: only string prints (nothing can error here).
-- Question: does is_plat WITH an argument work at file scope in an included
-- file during a mingw/linux session?
print("[scope] other.lua loaded; is_plat(linux)=" .. tostring(is_plat("linux"))
    .. " is_plat(mingw)=" .. tostring(is_plat("mingw"))
    .. " is_arch(x86_64)=" .. tostring(is_arch("x86_64")))
