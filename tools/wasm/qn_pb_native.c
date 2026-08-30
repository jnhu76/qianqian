/*
 * qn_pb_native.c — §14 PCM-copy microbenchmark, native baseline.
 *
 * Serves the same protocol as bench/wasm/qn_pb_guest.wasm so the runner
 * harness measures an identical work shape: a plain function call plus a
 * host memcpy from a 4 MiB pattern buffer. This is the "what does the same
 * bridge cost without WASM" reference for fixed call overhead and copy
 * bandwidth.
 */

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "qn_runner_common.h"

#define PB_BYTES (4u * 1024u * 1024u)

static uint8_t *g_buf;

static void pb_fill(uint8_t seed) {
    if (!g_buf) g_buf = malloc(PB_BYTES);
    for (uint32_t i = 0; i < PB_BYTES; i++) g_buf[i] = (uint8_t)(seed + i);
}

/* the "boundary": a real function call with the same arg shape as the
 * wasm export, doing the same guest-side memcpy the guest module does */
static int32_t pb_pull(int32_t dst, int32_t cap) {
    if (!g_buf || cap <= 0) return -1;
    uint32_t n = (uint32_t)cap;
    if (n > PB_BYTES) n = PB_BYTES;
    memcpy((void *)(intptr_t)dst, g_buf, n);
    return (int32_t)n;
}

static int32_t pb_reset(void) { return 0; } /* fixed-call-overhead baseline */

int main(void) {
    g_buf = malloc(PB_BYTES);
    if (!g_buf) return 1;
    char *hostbuf = malloc(PB_BYTES);
    if (!hostbuf) return 1;

    int sizes[] = { 1024, 4096, 16384, 65536, 262144, 1048576 };
    const int REPS = 2000;

    printf("{\"mode\":\"pb_native\",\"sizes\":[");
    for (unsigned si = 0; si < sizeof(sizes)/sizeof(sizes[0]); si++) {
        int bytes = sizes[si];
        pb_fill((uint8_t)si);

        /* fixed call overhead */
        double t0 = qn_now_ms();
        for (int i = 0; i < REPS; i++) pb_reset();
        double noop_ms = qn_now_ms() - t0;

        /* call + copy (guest memcpy + host memcpy in the wasm case; here the
         * equivalent is the call-into-memcpy + host copy) */
        t0 = qn_now_ms();
        for (int i = 0; i < REPS; i++) {
            pb_pull((int32_t)(intptr_t)hostbuf, bytes);
        }
        double call_copy_ms = qn_now_ms() - t0;

        /* direct memcpy baseline (Mode B equivalent: no boundary call) */
        t0 = qn_now_ms();
        for (int i = 0; i < REPS; i++) {
            memcpy(hostbuf, g_buf, (size_t)bytes);
        }
        double direct_ms = qn_now_ms() - t0;

        double noop_ns = noop_ms * 1e6 / REPS;
        double call_ns = call_copy_ms * 1e6 / REPS;
        double direct_ns = direct_ms * 1e6 / REPS;
        double gbps = direct_ms > 0
            ? (((double)bytes * REPS) / 1e9) / (direct_ms / 1000.0) : 0;
        printf("%s{\"bytes\":%d,\"noop_call_ns\":%.1f,\"call_copy_ns\":%.1f,"
               "\"direct_copy_ns\":%.1f,\"direct_gbps\":%.3f}",
               si ? "," : "", bytes, noop_ns, call_ns, direct_ns, gbps);
    }
    printf("]}\n");
    printf("{\"mode\":\"runner_stats\",\"peak_rss_kb\":%ld}\n", qn_peak_rss_kb());
    free(hostbuf);
    free(g_buf);
    return 0;
}
