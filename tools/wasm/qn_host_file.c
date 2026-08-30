/*
 * qn_host_file.c — native twin of the qianqian_host door.
 *
 * E09 runs the byte-identical guest harness (bench/wasm/qn_guest_bench.c)
 * natively so T_native shares every host responsibility with the WASM
 * runners: fixtures are loaded into host memory once (outside measured
 * sections) and read/seek/size are pure memory operations. This removes
 * disk noise from the tax decomposition; the FILE*-based E08 qn_bench
 * numbers remain the corroborating baseline.
 *
 * Cursor model (identical in every E09 runner, native or WASM):
 *   - the door keeps ONE cursor per handle: last absolute seek position,
 *     advanced by reads;
 *   - the guest synchronizes it with qn_host_seek(handle, 0) at session
 *     open, so sessions never depend on stale cursor state.
 *
 * Native-only TU; never built for wasm targets.
 */

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static uint8_t *g_buf;
static int64_t g_len;
static int64_t g_pos; /* the one cursor; guests sync it via seek */

void qn_host_file_free(void);

int qn_host_file_load(const char *path) {
    qn_host_file_free();
    FILE *f = fopen(path, "rb");
    if (!f) return -1;
    if (fseek(f, 0, SEEK_END) != 0) { fclose(f); return -1; }
    long n = ftell(f);
    if (n < 0 || fseek(f, 0, SEEK_SET) != 0) { fclose(f); return -1; }
    g_buf = malloc(n > 0 ? (size_t)n : 1);
    if (!g_buf) { fclose(f); return -1; }
    if (fread(g_buf, 1, (size_t)n, f) != (size_t)n) {
        fclose(f); qn_host_file_free(); return -1;
    }
    fclose(f);
    g_len = n;
    g_pos = 0;
    return 0;
}

void qn_host_file_free(void) {
    free(g_buf);
    g_buf = NULL;
    g_len = 0;
    g_pos = 0;
}

int64_t qn_host_read(int64_t handle, uint8_t *dst, int32_t len) {
    (void)handle;
    if (len < 0) return -1;
    if (g_pos >= g_len) return 0;
    int64_t n = g_len - g_pos;
    if (n > len) n = len;
    memcpy(dst, g_buf + g_pos, (size_t)n);
    g_pos += n;
    return n;
}

int64_t qn_host_seek(int64_t handle, int64_t absolute_offset) {
    (void)handle;
    if (absolute_offset < 0 || absolute_offset > g_len) return -1;
    g_pos = absolute_offset;
    return g_pos;
}

int64_t qn_host_size(int64_t handle) {
    (void)handle;
    return g_len;
}
