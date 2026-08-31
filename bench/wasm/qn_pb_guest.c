/*
 * qn_pb_guest.c — §14 PCM-copy microbenchmark guest (no FFmpeg).
 *
 * Measures exactly the guest→host PCM transfer path, isolated from decode:
 *   Mode B style: host reads [pb_ptr(), pb_ptr()+n) directly from linear memory
 *   Mode C style: host calls pb_pull(dst, cap) per chunk; guest memcpys into
 *                 the staging buffer; host then reads the staging bytes
 *
 * The same protocol is served natively by qn_pb_native.c so the fixed call
 * overhead and effective copy bandwidth can be compared against a plain
 * function-call + memcpy baseline.
 */

#include <stdint.h>
#include <stdlib.h>
#include <string.h>

#include "qn_host_imports.h"

#define PB_BYTES (4u * 1024u * 1024u)

static uint8_t *g_buf;

static uint8_t *pb_buf(void) {
    if (!g_buf) {
        g_buf = malloc(PB_BYTES);
        if (g_buf) memset(g_buf, 0xA5, PB_BYTES);
    }
    return g_buf;
}

/* fill the staging buffer with a fresh pattern (defeats copy elision) */
QN_EXPORT("pb_fill")
void pb_fill(uint8_t seed) {
    uint8_t *b = pb_buf();
    if (!b) return;
    for (uint32_t i = 0; i < PB_BYTES; i++) b[i] = (uint8_t)(seed + i);
}

QN_EXPORT("pb_ptr")
int32_t pb_ptr(void) {
    return (int32_t)(intptr_t)pb_buf();
}

/* Mode C: guest memcpy into caller-provided staging area */
QN_EXPORT("pb_pull")
int32_t pb_pull(int32_t dst, int32_t cap) {
    if (!g_buf || cap <= 0) return -1;
    uint32_t n = (uint32_t)cap;
    if (n > PB_BYTES) n = PB_BYTES;
    memcpy((void *)(intptr_t)dst, g_buf, n);
    return (int32_t)n;
}

/* host-side staging allocation for the Mode C pull loop (same protocol as
 * the bench guest's bench_stage_alloc; keeps the harness runtime-agnostic) */
QN_EXPORT("pb_stage_alloc")
int32_t pb_stage_alloc(int32_t size) {
    if (size <= 0) return 0;
    void *p = malloc((size_t)size);
    return p ? (int32_t)(intptr_t)p : 0;
}

QN_EXPORT("pb_reset")
void pb_reset(void) { /* staging cursor kept for protocol symmetry */ }

QN_EXPORT("pb_len")
int32_t pb_len(void) { return (int32_t)PB_BYTES; }

#if defined(QN_GUEST_NATIVE)
/* native twin: same exports, plain functions (linked with qn_pb_native) */
void pb_fill_native(uint8_t seed) { pb_fill(seed); }
int32_t pb_pull_native(int32_t dst, int32_t cap) { return pb_pull(dst, cap); }
uint8_t *pb_buf_native(void) { return pb_buf(); }
#endif
