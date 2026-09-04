// player_test_driver.cpp — TEST-ONLY C++ backend driver (see header).
//
// Drives the engine's manual-tick internal API (submit + backend_render),
// which drives NullAudioBackend — the deterministic test backend. Not part
// of the product C ABI; never shipped.
#include <cstring>

#include "player_engine.hpp"
#include "player_test_driver.h"

extern "C" {

int pe_test_drive(pe_engine* engine, uint64_t period_frames) {
    if (engine == nullptr) return -1;
    qn::PlayerEngine* e = reinterpret_cast<qn::PlayerEngine*>(engine);
    const qn::SubmitReport sub = e->submit(period_frames);
    e->backend_render(static_cast<std::int64_t>(period_frames));
    return std::strcmp(sub.kind, "idle") == 0 ? 1 : 0;
}

int pe_test_drive_render(pe_engine* engine, uint64_t period_frames,
                         uint64_t render_frames) {
    if (engine == nullptr) return -1;
    qn::PlayerEngine* e = reinterpret_cast<qn::PlayerEngine*>(engine);
    const qn::SubmitReport sub = e->submit(period_frames);
    e->backend_render(static_cast<std::int64_t>(render_frames));
    return std::strcmp(sub.kind, "idle") == 0 ? 1 : 0;
}

}  // extern "C"
