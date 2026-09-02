/*
 * player_test_driver.h — TEST-ONLY backend driver for the C consumer.
 *
 * The product C ABI (include/player_engine.h) deliberately exposes no audio
 * backend and no manual render ticks: a managed caller (KMP) must not need
 * to understand how audio is driven. This internal helper is the stand-in
 * for WASAPI's callback — it manually ticks the engine's NullAudioBackend
 * from inside the test binary. It lives in tests/, is never part of
 * include/player_engine.h, and is not shipped (corrective §32).
 */

#ifndef QIANQIAN_TESTS_PLAYER_PLAYER_TEST_DRIVER_H
#define QIANQIAN_TESTS_PLAYER_PLAYER_TEST_DRIVER_H

#include <stdint.h>

#include "player_engine.h"

#ifdef __cplusplus
extern "C" {
#endif

/* Drive one manual backend tick: submit `period_frames` of output and render
 * the same amount (device consumed it). Returns 0 on success, 1 when the
 * engine was idle (not playing), -1 on a NULL engine. */
int pe_test_drive(pe_engine *engine, uint64_t period_frames);

/* Drive submit(period_frames) but render only `render_frames` (independent
 * device pacing). Same return contract as pe_test_drive. */
int pe_test_drive_render(pe_engine *engine, uint64_t period_frames,
                         uint64_t render_frames);

#ifdef __cplusplus
}  /* extern "C" */
#endif

#endif /* QIANQIAN_TESTS_PLAYER_PLAYER_TEST_DRIVER_H */
