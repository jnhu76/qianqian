/*
 * qn_wasm3_runner.c — E09 runtime ladder, wasm3 (pinned commit 2b4a5fa4,
 * sources compiled into this runner; d_m3HasWASI=0 — the guest talks to the
 * host exclusively through the qianqian_host raw-linked imports).
 *
 * Same host responsibilities as qn_wamr_runner (see that file): fixture door,
 * Mode B direct PCM read, Mode C chunked pull, host-side hashing/stats.
 *
 * wasm3 raw-call conventions (verified against wasm3.h of the pinned commit):
 *   raw fn = const void *(IM3Runtime, IM3ImportContext, uint64_t *_sp, void *_mem)
 *   return slot(s) first at _sp, then args; m3ApiReturnType advances _sp.
 */

#define _POSIX_C_SOURCE 200809L
#include <inttypes.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

#include "wasm3.h"

#include "qn_runner_common.h"

#define QN_HANDLE ((int64_t)(intptr_t)&g_fixture)

static qn_fixture g_fixture;
static uint8_t *g_wasm;
static IM3Environment g_env;
static IM3Runtime g_runtime;
static IM3Module g_module;

/* --- the door as wasm3 raw functions ---------------------------------- */

static m3ApiRawFunction(ns_read) {
    m3ApiReturnType(int64_t)
    m3ApiGetArg(int64_t, handle)
    m3ApiGetArg(int32_t, app_dst)
    m3ApiGetArg(int32_t, len)
    size_t mem_size = m3_GetMemorySizeAt(_mem);
    if (len < 0 || (uint64_t)(uint32_t)app_dst + (uint64_t)(uint32_t)len > mem_size) {
        m3ApiReturn((int64_t)-1);
    }
    uint8_t *dst = m3ApiOffsetToPtr(app_dst);
    m3ApiReturn(qn_door_read(handle, dst, len));
}

static m3ApiRawFunction(ns_seek) {
    m3ApiReturnType(int64_t)
    m3ApiGetArg(int64_t, handle)
    m3ApiGetArg(int64_t, offset)
    m3ApiReturn(qn_door_seek(handle, offset));
}

static m3ApiRawFunction(ns_size) {
    m3ApiReturnType(int64_t)
    m3ApiGetArg(int64_t, handle)
    m3ApiReturn(qn_door_size(handle));
}

/* --- minimal WASI surface (test-only host environment) ------------------
 * The guest's wasi-libc imports a narrow WASI slice for stdio/clock/env.
 * Implemented directly against the host so the runner stays independent of
 * wasm3's uvwasi-based WASI module. Errnos: 0 ok, 8 badf, 29 spipe,
 * 52 nosys, 76 notcapable. */

#define WASI_EBADF 8
#define WASI_ESPIPE 29
#define WASI_ENOSYS 52
#define WASI_ENOTCAPABLE 76

static m3ApiRawFunction(wasi_environ_get) {
    m3ApiReturnType(int32_t)
    m3ApiReturn(0); /* environ_sizes_get reports zero entries */
}

static m3ApiRawFunction(wasi_environ_sizes_get) {
    m3ApiReturnType(int32_t)
    m3ApiGetArg(int32_t, count_ptr)
    m3ApiGetArg(int32_t, buf_ptr)
    if (!count_ptr || !buf_ptr) m3ApiReturn(WASI_EBADF);
    *(int32_t *)m3ApiOffsetToPtr(count_ptr) = 0;
    *(int32_t *)m3ApiOffsetToPtr(buf_ptr) = 0;
    m3ApiReturn(0);
}

static m3ApiRawFunction(wasi_clock_time_get) {
    m3ApiReturnType(int32_t)
    m3ApiGetArg(int32_t, id)
    m3ApiGetArg(int64_t, precision)
    m3ApiGetArg(int32_t, time_ptr)
    (void)precision;
    struct timespec ts;
    (id == 1 ? clock_gettime(CLOCK_MONOTONIC, &ts)
             : clock_gettime(CLOCK_REALTIME, &ts));
    uint64_t ns = (uint64_t)ts.tv_sec * 1000000000ull + (uint64_t)ts.tv_nsec;
    *(uint64_t *)m3ApiOffsetToPtr(time_ptr) = ns;
    m3ApiReturn(0);
}

static m3ApiRawFunction(wasi_fd_close) {
    m3ApiReturnType(int32_t)
    m3ApiGetArg(int32_t, fd)
    m3ApiReturn(fd <= 2 ? 0 : WASI_EBADF);
}

static m3ApiRawFunction(wasi_fd_fdstat_get) {
    m3ApiReturnType(int32_t)
    m3ApiGetArg(int32_t, fd)
    m3ApiGetArg(int32_t, stat_ptr)
    if (fd > 2) m3ApiReturn(WASI_EBADF);
    uint8_t *st = m3ApiOffsetToPtr(stat_ptr); /* 8 u8 + 2 u64 layout */
    memset(st, 0, 24);
    st[0] = 2; /* filetype: character_device for std streams */
    uint64_t all = ~0ull;
    memcpy(st + 8, &all, 8);
    memcpy(st + 16, &all, 8);
    m3ApiReturn(0);
}

static m3ApiRawFunction(wasi_fd_fdstat_set_flags) {
    m3ApiReturnType(int32_t)
    m3ApiReturn(0);
}

static m3ApiRawFunction(wasi_fd_prestat_get) {
    m3ApiReturnType(int32_t)
    m3ApiReturn(WASI_EBADF); /* no preopens */
}

static m3ApiRawFunction(wasi_fd_prestat_dir_name) {
    m3ApiReturnType(int32_t)
    m3ApiReturn(WASI_EBADF);
}

static m3ApiRawFunction(wasi_fd_read) {
    m3ApiReturnType(int32_t)
    m3ApiReturn(WASI_EBADF);
}

static m3ApiRawFunction(wasi_fd_seek) {
    m3ApiReturnType(int32_t)
    m3ApiGetArg(int32_t, fd)
    m3ApiReturn(fd <= 2 ? WASI_ESPIPE : WASI_EBADF);
}

static m3ApiRawFunction(wasi_fd_write) {
    m3ApiReturnType(int32_t)
    m3ApiGetArg(int32_t, fd)
    m3ApiGetArg(int32_t, iovs_ptr)
    m3ApiGetArg(int32_t, iovs_len)
    m3ApiGetArg(int32_t, nwritten_ptr)
    FILE *out = fd == 1 ? stdout : (fd == 2 ? stderr : NULL);
    if (!out) m3ApiReturn(WASI_EBADF);
    size_t total = 0;
    uint8_t *iovs = m3ApiOffsetToPtr(iovs_ptr);
    for (int32_t i = 0; i < iovs_len; i++) {
        uint32_t p, l;
        memcpy(&p, iovs + (size_t)i * 8, 4);
        memcpy(&l, iovs + (size_t)i * 8 + 4, 4);
        if (l) {
            fwrite(m3ApiOffsetToPtr((int32_t)p), 1, l, out);
            total += l;
        }
    }
    *(uint32_t *)m3ApiOffsetToPtr(nwritten_ptr) = (uint32_t)total;
    m3ApiReturn(0);
}

static m3ApiRawFunction(wasi_path_open) {
    m3ApiReturnType(int32_t)
    m3ApiReturn(WASI_ENOTCAPABLE);
}

static m3ApiRawFunction(wasi_poll_oneoff) {
    m3ApiReturnType(int32_t)
    m3ApiReturn(WASI_ENOSYS);
}

static m3ApiRawFunction(wasi_random_get) {
    m3ApiReturnType(int32_t)
    m3ApiGetArg(int32_t, buf_ptr)
    m3ApiGetArg(int32_t, len)
    uint8_t *buf = m3ApiOffsetToPtr(buf_ptr);
    for (int32_t i = 0; i < len; i++)
        buf[i] = (uint8_t)(rand() >> 7);
    m3ApiReturn(0);
}

static m3ApiRawFunction(wasi_proc_exit) {
    m3ApiGetArg(int32_t, code)
    fprintf(stderr, "qn_wasm3_runner: guest proc_exit(%d)\n", code);
    exit(code);
}

/* --- call helpers ------------------------------------------------------ */

static M3Result call_void(const char *name) {
    IM3Function f = NULL;
    M3Result r = m3_FindFunction(&f, g_runtime, name);
    if (r) return r;
    if (!f) return "export not found";
    return m3_CallV(f);
}

/* calls (i32,i32)->i32, or with ret==NULL any arg shape ending in i32s */
static M3Result call_i32(const char *name, int32_t a, int32_t b, int32_t *ret) {
    IM3Function f = NULL;
    M3Result r = m3_FindFunction(&f, g_runtime, name);
    if (r) return r;
    if (!f) return "export not found";
    if (ret) {
        int32_t out = 0;
        r = m3_CallV(f, a, b);
        if (!r) r = m3_GetResultsV(f, &out);
        *ret = out;
        return r;
    }
    return m3_CallV(f, a, b);
}

static M3Result call_i64arg(const char *name, int64_t arg) {
    IM3Function f = NULL;
    M3Result r = m3_FindFunction(&f, g_runtime, name);
    if (r) return r;
    if (!f) return "export not found";
    return m3_CallV(f, arg);
}

static void die_on(M3Result r, const char *where) {
    if (r) {
        fprintf(stderr, "qn_wasm3_runner: %s: %s\n", where, r);
        exit(1);
    }
}

/* --- modes ------------------------------------------------------------- */

static int run_correct(void) {
    die_on(call_i64arg("bench_bind", QN_HANDLE), "bench_bind");
    int32_t rc = 0;
    die_on(call_i32("bench_correct", 0, 0, &rc), "bench_correct");
    return rc;
}

static int run_bench_mode(int iters) {
    die_on(call_i64arg("bench_bind", QN_HANDLE), "bench_bind");
    int32_t rc = 0;
    die_on(call_i32("bench_bench", iters, 0, &rc), "bench_bench");
    return rc;
}

static int run_lifecycle_mode(void) {
    die_on(call_i64arg("bench_bind", QN_HANDLE), "bench_bind");
    int32_t rc = 0;
    die_on(call_i32("bench_lifecycle", 0, 0, &rc), "bench_lifecycle");
    return rc;
}

static int run_pcm(void) {
    die_on(call_i64arg("bench_bind", QN_HANDLE), "bench_bind");

    int32_t pages_before = 0;
    die_on(call_i32("bench_mem_pages", 0, 0, &pages_before), "bench_mem_pages");

    int32_t prep_rc = -1;
    die_on(call_i32("bench_pcm_prepare", 0, 0, &prep_rc), "bench_pcm_prepare");
    if (prep_rc != 0) return 1;

    int32_t pages_after = 0, channels = 0, ptr = 0, len = 0;
    die_on(call_i32("bench_mem_pages", 0, 0, &pages_after), "bench_mem_pages");
    die_on(call_i32("bench_pcm_channels", 0, 0, &channels), "bench_pcm_channels");
    die_on(call_i32("bench_pcm_ptr", 0, 0, &ptr), "bench_pcm_ptr");
    die_on(call_i32("bench_pcm_len", 0, 0, &len), "bench_pcm_len");
    if (len <= 0 || ptr <= 0) { fprintf(stderr, "bad pcm len/ptr\n"); return 1; }

    size_t mem_size = 0;
    uint8_t *mem = m3_GetMemory(g_module, &mem_size, 0);
    if (!mem) { fprintf(stderr, "no linear memory\n"); return 1; }

    /* Mode B: direct host read of guest linear memory */
    char *hostbuf = malloc((size_t)len);
    if (!hostbuf) return 1;
    double t0 = qn_now_ms();
    size_t off = 0;
    while (off < (size_t)len) {
        size_t take = (size_t)len - off;
        if (take > (1u << 20)) take = 1u << 20;
        memcpy(hostbuf + off, mem + ptr + off, take);
        off += take;
    }
    double copy_ms = qn_now_ms() - t0;
    char hex[65];
    qn_sha256(hostbuf, (size_t)len, hex);
    double gbps = copy_ms > 0 ? ((double)len / 1e9) / (copy_ms / 1000.0) : 0;
    printf("{\"mode\":\"pcm_host\",\"bytes\":%d,\"copy_ms\":%.3f,\"effective_gbps\":%.3f,"
           "\"sha256\":\"%s\",\"pages_before\":%d,\"pages_after\":%d}\n",
           len, copy_ms, gbps, hex, pages_before, pages_after);
    free(hostbuf);

    /* Mode C: chunked pull through the boundary into guest staging memory */
    const int32_t STAGE_CAP = 1 << 16;
    int32_t stage_app = 0;
    die_on(call_i32("bench_stage_alloc", STAGE_CAP, 0, &stage_app), "bench_stage_alloc");
    if (stage_app <= 0 || (uint64_t)(uint32_t)stage_app + STAGE_CAP > mem_size) {
        fprintf(stderr, "bench_stage_alloc returned %d\n", stage_app);
        return 1;
    }
    int chunk_frames[] = { 256, 1024, 4096 };
    for (unsigned ci = 0; ci < sizeof(chunk_frames)/sizeof(chunk_frames[0]); ci++) {
        int32_t chunk_bytes = chunk_frames[ci] * channels * 4;
        if (chunk_bytes > STAGE_CAP) chunk_bytes = STAGE_CAP;
        die_on(call_i32("bench_pcm_reset", 0, 0, NULL), "bench_pcm_reset");
        void *sha = qn_sha_new();
        double t = qn_now_ms();
        int32_t total = 0, calls = 0, n = 0;
        double max_call_ms = 0;
        do {
            double c0 = qn_now_ms();
            die_on(call_i32("bench_pcm_pull", stage_app, chunk_bytes, &n), "bench_pcm_pull");
            double dt = qn_now_ms() - c0;
            if (dt > max_call_ms) max_call_ms = dt;
            if (n > 0) {
                qn_sha_feed(sha, mem + stage_app, (size_t)n);
                total += n;
                calls++;
            }
        } while (n > 0);
        double total_ms = qn_now_ms() - t;
        char hex3[65] = "";
        qn_sha_finish(&sha, hex3);
        double calls_per_s = total_ms > 0 ? ((double)calls * 1000.0) / total_ms : 0;
        printf("{\"mode\":\"pcm_chunk\",\"chunk_frames\":%d,\"chunk_bytes\":%d,"
               "\"calls\":%d,\"bytes\":%d,\"total_ms\":%.3f,\"max_call_ms\":%.3f,"
               "\"calls_per_s\":%.1f,\"sha256\":\"%s\"}\n",
               chunk_frames[ci], chunk_bytes, calls, total, total_ms, max_call_ms,
               calls_per_s, hex3);
    }
    printf("{\"mode\":\"runner_stats\",\"peak_rss_kb\":%ld}\n", qn_peak_rss_kb());
    return 0;
}

int main(int argc, char **argv) {
    if (argc < 4) {
        fprintf(stderr, "usage: %s <module.wasm> <correct|bench|pcm|lifecycle> <fixture> [iters]\n", argv[0]);
        return 2;
    }
    const char *module_path = argv[1];
    const char *mode = argv[2];
    const char *fixture_path = argv[3];
    int iters = argc > 4 ? atoi(argv[4]) : 5;

    if (qn_fixture_load(&g_fixture, fixture_path) != 0) {
        fprintf(stderr, "cannot load fixture %s\n", fixture_path);
        return 1;
    }

    double t_load = qn_now_ms();
    FILE *mf = fopen(module_path, "rb");
    if (!mf) { fprintf(stderr, "cannot open module\n"); return 1; }
    fseek(mf, 0, SEEK_END);
    long msize = ftell(mf);
    fseek(mf, 0, SEEK_SET);
    g_wasm = malloc((size_t)msize);
    if (!g_wasm || fread(g_wasm, 1, (size_t)msize, mf) != (size_t)msize) {
        fprintf(stderr, "cannot read module\n"); return 1;
    }
    fclose(mf);
    double load_ms = qn_now_ms() - t_load;

    g_env = m3_NewEnvironment();
    if (!g_env) { fprintf(stderr, "m3_NewEnvironment failed\n"); return 1; }
    g_runtime = m3_NewRuntime(g_env, 8 << 20, NULL);
    if (!g_runtime) { fprintf(stderr, "m3_NewRuntime failed\n"); return 1; }

    double t_compile = qn_now_ms();
    M3Result r = m3_ParseModule(g_env, &g_module, g_wasm, (uint32_t)msize);
    die_on(r, "m3_ParseModule");
    r = m3_LoadModule(g_runtime, g_module);
    die_on(r, "m3_LoadModule");
    double compile_ms = qn_now_ms() - t_compile;

    /* link the qianqian_host door (module owns the links after LoadModule) */
    r = m3_LinkRawFunction(g_module, "qianqian_host", "read", "I(Iii)", ns_read);
    die_on(r, "link read");
    r = m3_LinkRawFunction(g_module, "qianqian_host", "seek", "I(II)", ns_seek);
    die_on(r, "link seek");
    r = m3_LinkRawFunction(g_module, "qianqian_host", "size", "I(I)", ns_size);
    die_on(r, "link size");

    /* link the WASI slice the guest's wasi-libc imports (unused imports are
     * only reached via deep libc paths; linking everything keeps compilation
     * deterministic). */
    IM3Module m = g_module;
    struct { const char *name; const char *sig; void *fn; } wasi_fns[] = {
        { "environ_get",         "i(ii)",     wasi_environ_get },
        { "environ_sizes_get",   "i(ii)",     wasi_environ_sizes_get },
        { "clock_time_get",      "i(iIi)",    wasi_clock_time_get },
        { "fd_close",            "i(i)",      wasi_fd_close },
        { "fd_fdstat_get",       "i(ii)",     wasi_fd_fdstat_get },
        { "fd_fdstat_set_flags", "i(ii)",     wasi_fd_fdstat_set_flags },
        { "fd_prestat_get",      "i(ii)",     wasi_fd_prestat_get },
        { "fd_prestat_dir_name", "i(iii)",    wasi_fd_prestat_dir_name },
        { "fd_read",             "i(iiii)",   wasi_fd_read },
        { "fd_seek",             "i(iIii)",   wasi_fd_seek },
        { "fd_write",            "i(iiii)",   wasi_fd_write },
        { "path_open",           "i(iiiiiIIii)", wasi_path_open },
        { "poll_oneoff",         "i(iiii)",   wasi_poll_oneoff },
        { "random_get",          "i(ii)",     wasi_random_get },
        { "proc_exit",           "(i)",       wasi_proc_exit },
    };
    for (size_t i = 0; i < sizeof(wasi_fns)/sizeof(wasi_fns[0]); i++) {
        M3Result lr = m3_LinkRawFunction(m, "wasi_snapshot_preview1",
                                         wasi_fns[i].name, wasi_fns[i].sig,
                                         wasi_fns[i].fn);
        if (lr)
            fprintf(stderr, "qn_wasm3_runner: link %s: %s (continuing)\n",
                    wasi_fns[i].name, lr);
    }

    double t0 = qn_now_ms();
    r = call_void("_initialize");
    double init_ms = qn_now_ms() - t0;
    die_on(r, "_initialize");
    fprintf(stderr, "qn_wasm3_runner: initialize_ms=%.3f\n", init_ms);

    int rc;
    if (strcmp(mode, "correct") == 0) rc = run_correct();
    else if (strcmp(mode, "bench") == 0) rc = run_bench_mode(iters);
    else if (strcmp(mode, "pcm") == 0) rc = run_pcm();
    else if (strcmp(mode, "lifecycle") == 0) {
        rc = run_lifecycle_mode();
        printf("{\"mode\":\"lifecycle_host\",\"runtime\":\"wasm3\",\"load_ms\":%.3f,"
               "\"compile_ms\":%.3f,\"instantiate_ms\":null,\"init_ms\":%.3f}\n",
               load_ms, compile_ms, init_ms);
    }
    else { fprintf(stderr, "unknown mode %s\n", mode); rc = 2; }

    m3_FreeRuntime(g_runtime);
    m3_FreeEnvironment(g_env);
    free(g_wasm);
    qn_fixture_free(&g_fixture);
    return rc;
}
