/*
 * qn_host_imports.h — the only doorway between a Qianqian WASM guest and the
 * outside world.
 *
 * The guest never sees a path, fd, handle, or runtime type. It sees an opaque
 * 64-bit source handle and three operations: read / seek / size. The host
 * owns whatever stands behind the handle (FILE*, memory buffer, browser
 * File). 64-bit offsets are preserved end to end.
 *
 * On wasm32 targets these resolve to imports from module "qianqian_host".
 * On native builds (native twin) the same symbols are provided by a plain
 * C translation unit, so identical guest code runs in both worlds.
 */
#ifndef QIANQIAN_WASM_HOST_IMPORTS_H
#define QIANQIAN_WASM_HOST_IMPORTS_H

#include <stdint.h>

#if defined(__wasm__)

#define QN_HOST_IMPORT(ret, name, params)                                  \
    __attribute__((import_module("qianqian_host"), import_name(#name)))    \
    ret qn_host_##name params

QN_HOST_IMPORT(int64_t, read, (int64_t handle, uint8_t *dst, int32_t len));
QN_HOST_IMPORT(int64_t, seek, (int64_t handle, int64_t absolute_offset));
QN_HOST_IMPORT(int64_t, size, (int64_t handle));

#undef QN_HOST_IMPORT

#else /* native twin */

#include <string.h>

int64_t qn_host_read(int64_t handle, uint8_t *dst, int32_t len);
int64_t qn_host_seek(int64_t handle, int64_t absolute_offset);
int64_t qn_host_size(int64_t handle);

#endif

#if defined(__wasm__)
#define QN_EXPORT(name) __attribute__((export_name(name)))
#else
#define QN_EXPORT(name)
#endif

#endif
