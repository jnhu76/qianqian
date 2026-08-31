/*
 * qn_wamr_runner.c — E09 runtime ladder, WAMR (WAMR-2.4.5, fast interpreter;
 * the AOT runner is the same host code loading a wamrc-compiled artifact).
 *
 * Host responsibilities (identical across the E09 runner ladder):
 *   - owns the fixture bytes, serves the qianqian_host read/seek/size door;
 *   - pulls canonical PCM out of guest linear memory (Mode B direct read,
 *     Mode C chunked pull) and hashes it host-side;
 *   - reports host wall/RSS numbers; guest decode JSON passes through the
 *     WASI fd_write door untouched.
 *
 * Uses only public WAMR embedding APIs; no runtime modification.
 */

#include <inttypes.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "wasm_export.h"

#include "qn_runner_common.h"

#define QN_HANDLE ((int64_t)(intptr_t)&g_fixture)

static qn_fixture g_fixture;

/* --- the door as WAMR natives ---------------------------------------- */

static int64_t ns_read(wasm_exec_env_t exec_env, int64_t handle,
                       uint32_t app_dst, int32_t len) {
    wasm_module_inst_t inst = wasm_runtime_get_module_inst(exec_env);
    if (len < 0) return -1;
    if (len > 0 &&
        !wasm_runtime_validate_app_addr(inst, app_dst, (uint64_t)(uint32_t)len))
        return -1;
    uint8_t *dst = wasm_runtime_addr_app_to_native(inst, app_dst);
    return qn_door_read(handle, dst, len);
}

static int64_t ns_seek(wasm_exec_env_t exec_env, int64_t handle,
                       int64_t absolute_offset) {
    (void)exec_env;
    return qn_door_seek(handle, absolute_offset);
}

static int64_t ns_size(wasm_exec_env_t exec_env, int64_t handle) {
    (void)exec_env;
    return qn_door_size(handle);
}

static NativeSymbol g_natives[] = {
    { "read", ns_read, "(Iii)I", NULL },
    { "seek", ns_seek, "(II)I",  NULL },
    { "size", ns_size, "(I)I",   NULL },
};

/* --- small call helpers ------------------------------------------------
 * wasm_runtime_call_wasm_v is VARIADIC: num_results/results then num_args
 * followed by one raw va_arg per parameter (uint32 for i32, uint64 for i64).
 * Passing wasm_val_t structs here hands the guest the struct's address. */

static bool call_void(wasm_exec_env_t env, wasm_module_inst_t inst,
                      const char *name) {
    wasm_function_inst_t f = wasm_runtime_lookup_function(inst, name);
    if (!f) return false;
    return wasm_runtime_call_wasm_v(env, f, 0, NULL, 0);
}

static bool call_i64(wasm_exec_env_t env, wasm_module_inst_t inst,
                     const char *name, int64_t arg) {
    wasm_function_inst_t f = wasm_runtime_lookup_function(inst, name);
    if (!f) return false;
    return wasm_runtime_call_wasm_v(env, f, 0, NULL, 1, (uint64_t)arg);
}

static bool call0_i32(wasm_exec_env_t env, wasm_module_inst_t inst,
                      const char *name, int32_t *ret) {
    wasm_function_inst_t f = wasm_runtime_lookup_function(inst, name);
    if (!f) return false;
    if (ret) {
        wasm_val_t r[1] = { 0 };
        if (!wasm_runtime_call_wasm_v(env, f, 1, r, 0)) return false;
        *ret = r[0].of.i32;
    } else {
        if (!wasm_runtime_call_wasm_v(env, f, 0, NULL, 0)) return false;
    }
    return true;
}

static bool call1_i32(wasm_exec_env_t env, wasm_module_inst_t inst,
                      const char *name, int32_t a, int32_t *ret) {
    wasm_function_inst_t f = wasm_runtime_lookup_function(inst, name);
    if (!f) return false;
    if (ret) {
        wasm_val_t r[1] = { 0 };
        if (!wasm_runtime_call_wasm_v(env, f, 1, r, 1, (uint32_t)a)) return false;
        *ret = r[0].of.i32;
    } else {
        if (!wasm_runtime_call_wasm_v(env, f, 0, NULL, 1, (uint32_t)a)) return false;
    }
    return true;
}

static bool call2_i32(wasm_exec_env_t env, wasm_module_inst_t inst,
                      const char *name, int32_t a, int32_t b, int32_t *ret) {
    wasm_function_inst_t f = wasm_runtime_lookup_function(inst, name);
    if (!f) return false;
    if (ret) {
        wasm_val_t r[1] = { 0 };
        if (!wasm_runtime_call_wasm_v(env, f, 1, r, 2, (uint32_t)a, (uint32_t)b))
            return false;
        *ret = r[0].of.i32;
    } else {
        if (!wasm_runtime_call_wasm_v(env, f, 0, NULL, 2, (uint32_t)a, (uint32_t)b))
            return false;
    }
    return true;
}

static void die_on_exception(wasm_module_inst_t inst, const char *where) {
    const char *ex = wasm_runtime_get_exception(inst);
    if (ex) {
        fprintf(stderr, "qn_wamr_runner: exception at %s: %s\n", where, ex);
        exit(1);
    }
}

/* --- modes ------------------------------------------------------------ */

static int run_correct(wasm_exec_env_t env, wasm_module_inst_t inst) {
    if (!call_i64(env, inst, "bench_bind", QN_HANDLE)) {
        fprintf(stderr, "MARK bind failed: %s\n",
                wasm_runtime_get_exception(inst) ?: "no exception");
        return 1;
    }
    fprintf(stderr, "MARK bound ok\n");
    int32_t rc = -1;
    if (!call0_i32(env, inst, "bench_correct", &rc)) {
        fprintf(stderr, "MARK correct call failed: %s\n",
                wasm_runtime_get_exception(inst) ?: "no exception");
        return 1;
    }
    fprintf(stderr, "MARK correct rc=%d\n", rc);
    return rc;
}

static int run_bench_mode(wasm_exec_env_t env, wasm_module_inst_t inst, int iters) {
    if (!call_i64(env, inst, "bench_bind", QN_HANDLE))
        { die_on_exception(inst, "bench_bind"); return 1; }
    int32_t rc = 0;
    if (!call1_i32(env, inst, "bench_bench", iters, &rc))
        { die_on_exception(inst, "bench_bench"); return 1; }
    return rc;
}

static const char *g_pcm_outfile; /* optional Mode B dump (E09 tolerance study) */

static int run_pcm(wasm_exec_env_t env, wasm_module_inst_t inst) {
    if (!call_i64(env, inst, "bench_bind", QN_HANDLE))
        { die_on_exception(inst, "bench_bind"); return 1; }

    int32_t pages_before = 0;
    if (!call0_i32(env, inst, "bench_mem_pages", &pages_before)) return 1;

    int32_t prep_rc = -1;
    if (!call0_i32(env, inst, "bench_pcm_prepare", &prep_rc))
        { die_on_exception(inst, "bench_pcm_prepare"); return 1; }
    if (prep_rc != 0) return 1;

    int32_t pages_after = 0, channels = 0, ptr = 0, len = 0;
    if (!call0_i32(env, inst, "bench_mem_pages", &pages_after)) return 1;
    if (!call0_i32(env, inst, "bench_pcm_channels", &channels)) return 1;
    if (!call0_i32(env, inst, "bench_pcm_ptr", &ptr)) return 1;
    if (!call0_i32(env, inst, "bench_pcm_len", &len)) return 1;
    if (len <= 0 || ptr <= 0) { fprintf(stderr, "bad pcm len/ptr\n"); return 1; }

    /* Mode B: host reads guest linear memory directly (no boundary calls) */
    const uint8_t *src = wasm_runtime_addr_app_to_native(inst, (uint64_t)(uint32_t)ptr);
    char *hostbuf = malloc((size_t)len);
    if (!hostbuf) return 1;
    double t0 = qn_now_ms();
    size_t off = 0;
    while (off < (size_t)len) {
        size_t take = (size_t)len - off;
        if (take > (1u << 20)) take = 1u << 20;
        memcpy(hostbuf + off, src + off, take);
        off += take;
    }
    double copy_ms = qn_now_ms() - t0;
    char hex[65];
    qn_sha256(hostbuf, (size_t)len, hex);
    if (g_pcm_outfile) {
        FILE *of = fopen(g_pcm_outfile, "wb");
        if (!of || fwrite(hostbuf, 1, (size_t)len, of) != (size_t)len) {
            fprintf(stderr, "cannot write %s\n", g_pcm_outfile);
            if (of) fclose(of);
            return 1;
        }
        fclose(of);
    }
    double gbps = copy_ms > 0 ? ((double)len / 1e9) / (copy_ms / 1000.0) : 0;
    printf("{\"mode\":\"pcm_host\",\"bytes\":%d,\"copy_ms\":%.3f,\"effective_gbps\":%.3f,"
           "\"sha256\":\"%s\",\"pages_before\":%d,\"pages_after\":%d}\n",
           len, copy_ms, gbps, hex, pages_before, pages_after);
    free(hostbuf);

    /* Mode C: chunked guest->host pull (boundary call + copy per chunk) */
    const int32_t STAGE_CAP = 1 << 16;
    int32_t stage_app = 0;
    if (!call1_i32(env, inst, "bench_stage_alloc", STAGE_CAP, &stage_app))
        { die_on_exception(inst, "bench_stage_alloc"); return 1; }
    if (stage_app <= 0) { fprintf(stderr, "bench_stage_alloc failed\n"); return 1; }
    uint8_t *stage_native = wasm_runtime_addr_app_to_native(inst, stage_app);
    int chunk_frames[] = { 256, 1024, 4096 };
    for (unsigned ci = 0; ci < sizeof(chunk_frames)/sizeof(chunk_frames[0]); ci++) {
        int32_t chunk_bytes = chunk_frames[ci] * channels * 4;
        if (chunk_bytes > STAGE_CAP) chunk_bytes = STAGE_CAP;
        if (!call_void(env, inst, "bench_pcm_reset"))
            { die_on_exception(inst, "bench_pcm_reset"); return 1; }
        void *sha = qn_sha_new();
        double t = qn_now_ms();
        int32_t total = 0, calls = 0, n = 0;
        double max_call_ms = 0;
        do {
            double c0 = qn_now_ms();
            if (!call2_i32(env, inst, "bench_pcm_pull", (int32_t)stage_app,
                           chunk_bytes, &n))
                { die_on_exception(inst, "bench_pcm_pull"); return 1; }
            double dt = qn_now_ms() - c0;
            if (dt > max_call_ms) max_call_ms = dt;
            if (n > 0) {
                qn_sha_feed(sha, stage_native, (size_t)n);
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
    wasm_runtime_module_free(inst, stage_app);

    printf("{\"mode\":\"runner_stats\",\"peak_rss_kb\":%ld}\n", qn_peak_rss_kb());
    return 0;
}

int main(int argc, char **argv) {
    if (argc < 4) {
        fprintf(stderr,
#ifdef QN_WAMR_AOT_RUNNER
                "usage: %s <module.aot> <correct|bench|pcm> <fixture> [iters]\n",
#else
                "usage: %s <module.wasm> <correct|bench|pcm> <fixture> [iters]\n",
#endif
                argv[0]);
        return 2;
    }
    const char *module_path = argv[1];
    const char *mode = argv[2];
    const char *fixture_path = argv[3];
    int iters = argc > 4 ? atoi(argv[4]) : 5;
    g_pcm_outfile = argc > 4 ? argv[4] : NULL; /* pcm mode: <wasm> pcm <fixture> [outfile] */

    if (qn_fixture_load(&g_fixture, fixture_path) != 0) {
        fprintf(stderr, "cannot load fixture %s\n", fixture_path);
        return 1;
    }

    RuntimeInitArgs init_args;
    memset(&init_args, 0, sizeof(init_args));
    init_args.mem_alloc_type = Alloc_With_System_Allocator;
    init_args.native_module_name = "qianqian_host";
    init_args.n_native_symbols = sizeof(g_natives) / sizeof(g_natives[0]);
    init_args.native_symbols = g_natives;
    if (!wasm_runtime_full_init(&init_args)) {
        fprintf(stderr, "wasm_runtime_full_init failed\n");
        return 1;
    }
    wasm_runtime_set_log_level((log_level_t)3);

    FILE *mf = fopen(module_path, "rb");
    if (!mf) { fprintf(stderr, "cannot open module %s\n", module_path); return 1; }
    fseek(mf, 0, SEEK_END);
    long msize = ftell(mf);
    fseek(mf, 0, SEEK_SET);
    uint8_t *mbuf = malloc((size_t)msize);
    if (!mbuf || fread(mbuf, 1, (size_t)msize, mf) != (size_t)msize) {
        fprintf(stderr, "cannot read module\n"); return 1;
    }
    fclose(mf);

    char error_buf[256];
    wasm_module_t module = wasm_runtime_load(mbuf, (uint32_t)msize,
                                             error_buf, sizeof(error_buf));
    if (!module) { fprintf(stderr, "load failed: %s\n", error_buf); return 1; }

    double t_inst = qn_now_ms();
    /* WASI command guest (QN_GUEST_COMMAND): bind via _start with the door
     * handle in argv[1]. Reactor guests: exports only. */
    char *wasi_argv[] = { (char *)"qn_guest_bench_cmd", (char *)"1", NULL };
    char handle_str[32];
    bool is_command_guest = strstr(module_path, "_cmd.") != NULL;
    /* set_wasi_args is required for BOTH guest shapes: command guests need
     * argv (the door handle) and reactor guests need the stdio fd wiring —
     * without it guest printf (WASI fd_write) is silently discarded. */
    if (is_command_guest) {
        snprintf(handle_str, sizeof(handle_str), "%" PRId64, QN_HANDLE);
        wasi_argv[1] = handle_str;
    }
    wasm_runtime_set_wasi_args(module, NULL, 0, NULL, 0, NULL, 0,
                               wasi_argv, is_command_guest ? 2 : 1);
    wasm_module_inst_t inst = wasm_runtime_instantiate(module, 1 << 20, 0,
                                                       error_buf, sizeof(error_buf));
    if (!inst) { fprintf(stderr, "instantiate failed: %s\n", error_buf); return 1; }
    double inst_ms = qn_now_ms() - t_inst;

    wasm_exec_env_t env = wasm_runtime_create_exec_env(inst, 8 << 20);
    if (!env) { fprintf(stderr, "create_exec_env failed\n"); return 1; }

    /* E09 guest-side workarounds (see tools/wasm_patch_initialize_guard.py
     * and the xmake guest link flags) make wasi-sdk-34 reactor modules
     * initializable under WAMR: the __heap_base export keeps WAMR's app
     * heap out of guest .bss, and the nopped guard unreachable keeps the
     * mis-targeted br_if from trapping. With those in place the reactor
     * guest initializes cleanly here; the command guest goes through
     * _start instead. Both paths drive identical bench exports. */
    if (!is_command_guest) {
        if (!call_void(env, inst, "_initialize")) {
            const char *init_ex = wasm_runtime_get_exception(inst);
            fprintf(stderr, "qn_wamr_runner: _initialize failed (%s)\n",
                    init_ex ? init_ex : "?");
            return 1;
        }
    }
    else if (!call_void(env, inst, "_start")) {
        const char *start_ex = wasm_runtime_get_exception(inst);
        fprintf(stderr, "qn_wamr_runner: _start failed: %s\n",
                start_ex ? start_ex : "?");
        return 1;
    }

    int rc;
    if (strcmp(mode, "correct") == 0) rc = run_correct(env, inst);
    else if (strcmp(mode, "bench") == 0) rc = run_bench_mode(env, inst, iters);
    else if (strcmp(mode, "pcm") == 0) rc = run_pcm(env, inst);
    else { fprintf(stderr, "unknown mode %s\n", mode); rc = 2; }

    fprintf(stderr, "qn_wamr_runner: instantiate_ms=%.3f\n", inst_ms);
    wasm_runtime_destroy_exec_env(env);
    wasm_runtime_deinstantiate(inst);
    wasm_runtime_unload(module);
    wasm_runtime_destroy();
    qn_fixture_free(&g_fixture);
    free(mbuf);
    return rc;
}
