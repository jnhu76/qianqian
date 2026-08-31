/*
 * qn_runner_common.h — shared host-side helpers for the E09 runtime ladder.
 *
 * Every runner (native twin, WAMR interp/AOT, wasm3, Wasmtime) carries the
 * same host responsibility:
 *   - load the fixture into host memory once (host owns the source),
 *   - serve the qianqian_host read/seek/size door to the guest,
 *   - pull / hash canonical PCM,
 *   - report wall/CPU timing and RSS from the host side.
 */
#ifndef QIANQIAN_E09_RUNNER_COMMON_H
#define QIANQIAN_E09_RUNNER_COMMON_H

#include <stdint.h>
#include <stddef.h>

typedef struct qn_fixture {
    uint8_t *buf;
    int64_t len;
    int64_t pos; /* the one door cursor; guest syncs via seek(handle,0) */
} qn_fixture;

int qn_fixture_load(qn_fixture *fx, const char *path);
void qn_fixture_free(qn_fixture *fx);

/* the door (handle is opaque; runners bind exactly one fixture at handle 1) */
int64_t qn_door_read(int64_t handle, uint8_t *dst, int32_t len);
int64_t qn_door_seek(int64_t handle, int64_t absolute_offset);
int64_t qn_door_size(int64_t handle);

/* timing / process stats */
double qn_now_ms(void);
long qn_peak_rss_kb(void);
double qn_cpu_time_ms(void);

/* sha256 over host bytes (Mode B hashing) */
void qn_sha256(const void *data, size_t n, char hex_out[65]);

/* incremental sha256: feed chunks, finish once, then free the state */
void *qn_sha_new(void);
void qn_sha_feed(void *state, const void *data, size_t n);
void qn_sha_finish(void **state, char hex_out[65]);

#endif
