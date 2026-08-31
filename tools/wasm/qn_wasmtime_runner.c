/*
 * qn_wasmtime_runner.c — E09 runtime ladder, Wasmtime (pinned v48.0.1 C API,
 * official prebuilt static library).
 *
 * Same host responsibilities as the other E09 runners: fixture door via
 * qianqian_host imports, Mode B direct PCM read, Mode C chunked pull,
 * host-side hashing/stats. WASI stdout inherited so guest JSON passes
 * through untouched.
 */

#include <inttypes.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "wasmtime.h"

#include "qn_runner_common.h"

#define QN_HANDLE ((int64_t)(intptr_t)&g_fixture)

static qn_fixture g_fixture;
static wasmtime_context_t *g_ctx;
static wasmtime_instance_t g_instance;

/* --- the door as wasmtime host functions ------------------------------ */

static wasm_trap_t *ns_read(void *env, wasmtime_caller_t *caller,
                            const wasmtime_val_t *args, size_t nargs,
                            wasmtime_val_t *results, size_t nresults) {
    (void)env; (void)nargs; (void)nresults;
    int64_t handle = args[0].of.i64;
    int32_t app_dst = args[1].of.i32;
    int32_t len = args[2].of.i32;

    wasmtime_extern_t item;
    if (!wasmtime_caller_export_get(caller, "memory", 6, &item)) {
        results[0].kind = WASMTIME_I64; results[0].of.i64 = -1; return NULL;
    }
    wasmtime_context_t *ctx = wasmtime_caller_context(caller);
    uint8_t *mem = wasmtime_memory_data(ctx, &item.of.memory);
    size_t mem_size = wasmtime_memory_data_size(ctx, &item.of.memory);
    if (len < 0 || (uint64_t)(uint32_t)app_dst + (uint64_t)(uint32_t)len > mem_size) {
        results[0].kind = WASMTIME_I64; results[0].of.i64 = -1; return NULL;
    }
    results[0].kind = WASMTIME_I64;
    results[0].of.i64 = qn_door_read(handle, mem + (uint32_t)app_dst, len);
    return NULL;
}

static wasm_trap_t *ns_seek(void *env, wasmtime_caller_t *caller,
                            const wasmtime_val_t *args, size_t nargs,
                            wasmtime_val_t *results, size_t nresults) {
    (void)env; (void)caller; (void)nargs; (void)nresults;
    results[0].kind = WASMTIME_I64;
    results[0].of.i64 = qn_door_seek(args[0].of.i64, args[1].of.i64);
    return NULL;
}

static wasm_trap_t *ns_size(void *env, wasmtime_caller_t *caller,
                            const wasmtime_val_t *args, size_t nargs,
                            wasmtime_val_t *results, size_t nresults) {
    (void)env; (void)caller; (void)nargs; (void)nresults;
    results[0].kind = WASMTIME_I64;
    results[0].of.i64 = qn_door_size(args[0].of.i64);
    return NULL;
}

/* --- call helpers ------------------------------------------------------ */

static void clear_err(wasmtime_error_t *err, const char *where) {
    if (!err) return;
    wasm_name_t msg;
    wasmtime_error_message(err, &msg);
    fprintf(stderr, "qn_wasmtime_runner: %s: %.*s\n", where,
            (int)msg.size, msg.data);
    wasm_name_delete(&msg);
    wasmtime_error_delete(err);
    exit(1);
}

static void clear_trap(wasm_trap_t *trap, const char *where) {
    if (!trap) return;
    wasm_name_t msg;
    wasm_trap_message(trap, &msg);
    fprintf(stderr, "qn_wasmtime_runner: trap at %s: %.*s\n", where,
            (int)msg.size, msg.data);
    wasm_name_delete(&msg);
    wasm_trap_delete(trap);
    exit(1);
}

static bool call_void(const char *name) {
    wasmtime_extern_t item;
    if (!wasmtime_instance_export_get(g_ctx, &g_instance, name, strlen(name), &item))
        return false;
    wasm_trap_t *trap = NULL;
    wasmtime_error_t *err = wasmtime_func_call(g_ctx, &item.of.func, NULL, 0, NULL, 0, &trap);
    if (err) { clear_err(err, name); return false; }
    if (trap) { clear_trap(trap, name); return false; }
    return true;
}

static bool call0_i32(const char *name, int32_t *ret) {
    wasmtime_extern_t item;
    if (!wasmtime_instance_export_get(g_ctx, &g_instance, name, strlen(name), &item))
        return false;
    wasmtime_val_t results[1];
    wasm_trap_t *trap = NULL;
    wasmtime_error_t *err = wasmtime_func_call(g_ctx, &item.of.func, NULL, 0, results, 1, &trap);
    if (err) { clear_err(err, name); return false; }
    if (trap) { clear_trap(trap, name); return false; }
    *ret = results[0].of.i32;
    return true;
}

static bool call1_i64(const char *name, int64_t a) {
    wasmtime_extern_t item;
    if (!wasmtime_instance_export_get(g_ctx, &g_instance, name, strlen(name), &item))
        return false;
    wasmtime_val_t args[1] = { { .kind = WASMTIME_I64, .of.i64 = a } };
    wasm_trap_t *trap = NULL;
    wasmtime_error_t *err = wasmtime_func_call(g_ctx, &item.of.func, args, 1, NULL, 0, &trap);
    if (err) { clear_err(err, name); return false; }
    if (trap) { clear_trap(trap, name); return false; }
    return true;
}

static bool call0_void(const char *name) {
    wasmtime_extern_t item;
    if (!wasmtime_instance_export_get(g_ctx, &g_instance, name, strlen(name), &item))
        return false;
    wasm_trap_t *trap = NULL;
    wasmtime_error_t *err = wasmtime_func_call(g_ctx, &item.of.func, NULL, 0, NULL, 0, &trap);
    if (err) { clear_err(err, name); return false; }
    if (trap) { clear_trap(trap, name); return false; }
    return true;
}

static bool call1_i32(const char *name, int32_t a, int32_t *ret) {
    wasmtime_extern_t item;
    if (!wasmtime_instance_export_get(g_ctx, &g_instance, name, strlen(name), &item))
        return false;
    wasmtime_val_t args[1] = { { .kind = WASMTIME_I32, .of.i32 = a } };
    wasmtime_val_t results[1];
    wasm_trap_t *trap = NULL;
    wasmtime_error_t *err = wasmtime_func_call(g_ctx, &item.of.func, args, 1, results, 1, &trap);
    if (err) { clear_err(err, name); return false; }
    if (trap) { clear_trap(trap, name); return false; }
    *ret = results[0].of.i32;
    return true;
}

static bool call2_i32(const char *name, int32_t a, int32_t b, int32_t *ret) {
    wasmtime_extern_t item;
    if (!wasmtime_instance_export_get(g_ctx, &g_instance, name, strlen(name), &item))
        return false;
    wasmtime_val_t args[2] = {
        { .kind = WASMTIME_I32, .of.i32 = a },
        { .kind = WASMTIME_I32, .of.i32 = b },
    };
    wasmtime_val_t results[1];
    wasm_trap_t *trap = NULL;
    wasmtime_error_t *err = wasmtime_func_call(g_ctx, &item.of.func, args, 2, results, 1, &trap);
    if (err) { clear_err(err, name); return false; }
    if (trap) { clear_trap(trap, name); return false; }
    *ret = results[0].of.i32;
    return true;
}

static void die_on(bool ok, const char *where) {
    if (!ok) {
        fprintf(stderr, "qn_wasmtime_runner: %s failed\n", where);
        exit(1);
    }
}

/* --- modes (same output contract as qn_wamr_runner) -------------------- */

static int run_correct(void) {
    fprintf(stderr, "MARK run_correct\n");
    die_on(call1_i64("bench_bind", QN_HANDLE), "bench_bind");
    fprintf(stderr, "MARK bound\n");
    int32_t rc = 0;
    die_on(call0_i32("bench_correct", &rc), "bench_correct");
    return rc;
}

static int run_bench_mode(int iters) {
    die_on(call1_i64("bench_bind", QN_HANDLE), "bench_bind");
    int32_t rc = 0;
    die_on(call1_i32("bench_bench", iters, &rc), "bench_bench");
    return rc;
}

static int run_lifecycle_mode(void) {
    die_on(call1_i64("bench_bind", QN_HANDLE), "bench_bind");
    int32_t rc = 0;
    die_on(call0_i32("bench_lifecycle", &rc), "bench_lifecycle");
    return rc;
}

static int run_pcm(void) {
    die_on(call1_i64("bench_bind", QN_HANDLE), "bench_bind");

    int32_t pages_before = 0;
    die_on(call0_i32("bench_mem_pages", &pages_before), "bench_mem_pages");

    int32_t prep_rc = -1;
    die_on(call0_i32("bench_pcm_prepare", &prep_rc), "bench_pcm_prepare");
    if (prep_rc != 0) return 1;

    int32_t pages_after = 0, channels = 0, ptr = 0, len = 0;
    die_on(call0_i32("bench_mem_pages", &pages_after), "bench_mem_pages");
    die_on(call0_i32("bench_pcm_channels", &channels), "bench_pcm_channels");
    die_on(call0_i32("bench_pcm_ptr", &ptr), "bench_pcm_ptr");
    die_on(call0_i32("bench_pcm_len", &len), "bench_pcm_len");
    if (len <= 0 || ptr <= 0) { fprintf(stderr, "bad pcm len/ptr\n"); return 1; }

    /* resolve memory once; re-resolve the data pointer after any growth */
    wasmtime_extern_t mem_item;
    die_on(wasmtime_instance_export_get(g_ctx, &g_instance, "memory", 6, &mem_item),
           "memory export");
    uint8_t *mem = wasmtime_memory_data(g_ctx, &mem_item.of.memory);

    /* Mode B */
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

    /* Mode C */
    const int32_t STAGE_CAP = 1 << 16;
    int32_t stage_app = 0;
    die_on(call1_i32("bench_stage_alloc", STAGE_CAP, &stage_app), "bench_stage_alloc");
    if (stage_app <= 0) { fprintf(stderr, "bench_stage_alloc failed\n"); return 1; }
    int chunk_frames[] = { 256, 1024, 4096 };
    for (unsigned ci = 0; ci < sizeof(chunk_frames)/sizeof(chunk_frames[0]); ci++) {
        int32_t chunk_bytes = chunk_frames[ci] * channels * 4;
        if (chunk_bytes > STAGE_CAP) chunk_bytes = STAGE_CAP;
        die_on(call0_void("bench_pcm_reset"), "bench_pcm_reset");
        void *sha = qn_sha_new();
        double t = qn_now_ms();
        int32_t total = 0, calls = 0, n = 0;
        double max_call_ms = 0;
        do {
            double c0 = qn_now_ms();
            die_on(call2_i32("bench_pcm_pull", stage_app, chunk_bytes, &n), "bench_pcm_pull");
            double dt = qn_now_ms() - c0;
            if (dt > max_call_ms) max_call_ms = dt;
            if (n > 0) {
                /* memory can have grown during prepare; data ptr resolved above */
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

/* --- setup -------------------------------------------------------------- */

/*
 * NOTE (typed finding): wasmtime v48.0.1 C API crashes with a deterministic
 * double-free inside wasm_functype_new when the param/result vectors are
 * built with wasm_valtype_vec_new_uninitialized + element assignment.
 * Minimal repro: 100% hit. Workaround: build vectors with
 * wasm_valtype_vec_new from malloc'd element arrays (ownership transfer).
 * Re-check against the wasmtime issue tracker when network allows.
 */
static wasm_functype_t *ft_from_kinds(const wasm_valkind_t *kinds, size_t n) {
    wasm_valtype_t **arr = malloc(n * sizeof(wasm_valtype_t *));
    for (size_t i = 0; i < n; i++) arr[i] = wasm_valtype_new(kinds[i]);
    wasm_valtype_vec_t vec;
    wasm_valtype_vec_new(&vec, n, arr);
    wasm_valtype_t **rv = malloc(sizeof(wasm_valtype_t *));
    rv[0] = wasm_valtype_new(WASM_I64);
    wasm_valtype_vec_t results;
    wasm_valtype_vec_new(&results, 1, rv);
    return wasm_functype_new(&vec, &results);
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

    wasm_engine_t *engine = wasm_engine_new();
    if (!engine) return 1;
    wasmtime_store_t *store = wasmtime_store_new(engine, NULL, NULL);
    if (!store) return 1;
    g_ctx = wasmtime_store_context(store);

    wasi_config_t *wasi = wasi_config_new();
    wasi_config_inherit_stdout(wasi);
    wasi_config_inherit_stderr(wasi);
    wasmtime_context_set_wasi(g_ctx, wasi);

    wasmtime_linker_t *linker = wasmtime_linker_new(engine);
    clear_err(wasmtime_linker_define_wasi(linker), "define_wasi");

    /* door imports: (i64, i32, i32)->i64, (i64, i64)->i64, (i64)->i64 */
    static const wasm_valkind_t k_read[3] = { WASM_I64, WASM_I32, WASM_I32 };
    static const wasm_valkind_t k_seek[2] = { WASM_I64, WASM_I64 };
    static const wasm_valkind_t k_size[1] = { WASM_I64 };
    wasm_functype_t *ft_read = ft_from_kinds(k_read, 3);
    wasm_functype_t *ft_seek = ft_from_kinds(k_seek, 2);
    wasm_functype_t *ft_size = ft_from_kinds(k_size, 1);

    clear_err(wasmtime_linker_define_func(linker, "qianqian_host", sizeof("qianqian_host") - 1, "read", 4,
                                          ft_read, ns_read, NULL, NULL), "define read");
    clear_err(wasmtime_linker_define_func(linker, "qianqian_host", sizeof("qianqian_host") - 1, "seek", 4,
                                          ft_seek, ns_seek, NULL, NULL), "define seek");
    clear_err(wasmtime_linker_define_func(linker, "qianqian_host", sizeof("qianqian_host") - 1, "size", 4,
                                          ft_size, ns_size, NULL, NULL), "define size");

    double t_load = qn_now_ms();
    FILE *mf = fopen(module_path, "rb");
    if (!mf) { fprintf(stderr, "cannot open module\n"); return 1; }
    fseek(mf, 0, SEEK_END);
    long msize = ftell(mf);
    fseek(mf, 0, SEEK_SET);
    uint8_t *mbuf = malloc((size_t)msize);
    if (!mbuf || fread(mbuf, 1, (size_t)msize, mf) != (size_t)msize) {
        fprintf(stderr, "cannot read module\n"); return 1;
    }
    fclose(mf);
    double load_ms = qn_now_ms() - t_load;

    double t_compile = qn_now_ms();
    wasmtime_module_t *module = NULL;
    clear_err(wasmtime_module_new(engine, mbuf, (size_t)msize, &module), "module_new");
    double compile_ms = qn_now_ms() - t_compile;

    double t_inst = qn_now_ms();
    wasm_trap_t *trap = NULL;
    wasmtime_error_t *err = wasmtime_linker_instantiate(linker, g_ctx, module,
                                                        &g_instance, &trap);
    double inst_ms = qn_now_ms() - t_inst;
    if (err) clear_err(err, "instantiate");
    if (trap) clear_trap(trap, "instantiate");
    fprintf(stderr, "qn_wasmtime_runner: instantiate_ms=%.3f\n", inst_ms);

    /* reactor initialization: wasi-libc ctors/malloc/stdout setup */
    fprintf(stderr, "MARK before _initialize\n");
    die_on(call0_void("_initialize"), "_initialize");
    fprintf(stderr, "MARK after _initialize\n");

    int rc;
    if (strcmp(mode, "correct") == 0) rc = run_correct();
    else if (strcmp(mode, "bench") == 0) rc = run_bench_mode(iters);
    else if (strcmp(mode, "pcm") == 0) rc = run_pcm();
    else if (strcmp(mode, "lifecycle") == 0) {
        rc = run_lifecycle_mode();
        printf("{\"mode\":\"lifecycle_host\",\"runtime\":\"wasmtime\",\"load_ms\":%.3f,"
               "\"compile_ms\":%.3f,\"instantiate_ms\":%.3f}\n",
               load_ms, compile_ms, inst_ms);
    }
    else { fprintf(stderr, "unknown mode %s\n", mode); rc = 2; }

    wasmtime_module_delete(module);
    wasmtime_linker_delete(linker);
    wasmtime_store_delete(store);
    wasm_engine_delete(engine);
    qn_fixture_free(&g_fixture);
    free(mbuf);
    return rc;
}
