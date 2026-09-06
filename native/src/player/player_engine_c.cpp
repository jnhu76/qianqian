// player_engine_c.cpp — product C ABI over the internal PlayerEngine.
//
// The C surface is deliberately narrow (include/player_engine.h): control +
// polled time-domain snapshot. No backend/test symbols appear. Every entry
// point is wrapped so no C++ exception can cross extern "C", and no
// caller-controlled input can abort the process — invalid input returns a
// typed status.
//
// Enum parity with the internal engine is asserted statically where they
// overlap; the shim adds its own validation statuses.
#include <cstdint>
#include <cstring>
#include <limits>
#include <new>

#include "player_engine.h"
#include "player_engine.hpp"

// Application-facing runtime flavor (docs/architecture/platform-audio.md):
// the qianqian_runtime Windows build composes the WASAPI render thread into
// pe_create/pe_destroy so the public C ABI drives real output. Tests and
// non-Windows builds never define QN_QIANQIAN_RUNTIME and keep the pure
// engine shim below — the NullAudioBackend stays the single seam consumer.
#if defined(QN_QIANQIAN_RUNTIME) && defined(_WIN32)
#include "wasapi_renderer.hpp"
#endif

namespace {

#if defined(QN_QIANQIAN_RUNTIME) && defined(_WIN32)
// Runtime flavor: the C handle owns the engine AND its platform renderer.
struct pe_runtime_handle {
    qn::PlayerEngine* engine;
    qn::WasapiRenderer* renderer;
};

qn::PlayerEngine* self(pe_engine* e) {
    return reinterpret_cast<pe_runtime_handle*>(e)->engine;
}
#else
qn::PlayerEngine* self(pe_engine* e) {
    return reinterpret_cast<qn::PlayerEngine*>(e);
}
#endif

static_assert((int)PE_OK == (int)qn::PlayerStatus::Ok);
static_assert((int)PE_ERR_ILLEGAL_CALL == (int)qn::PlayerStatus::ErrIllegalCall);
static_assert((int)PE_ERR_OPEN_FAILED == (int)qn::PlayerStatus::ErrOpenFailed);
static_assert((int)PE_ERR_SEEK_FAILED == (int)qn::PlayerStatus::ErrSeekFailed);
static_assert((int)PE_ERR_INTERNAL == (int)qn::PlayerStatus::ErrInternal);

static_assert((int)PE_STATE_EMPTY == (int)qn::PlayerState::Empty);
static_assert((int)PE_STATE_READY == (int)qn::PlayerState::Ready);
static_assert((int)PE_STATE_PLAYING == (int)qn::PlayerState::Playing);
static_assert((int)PE_STATE_PAUSED == (int)qn::PlayerState::Paused);
static_assert((int)PE_STATE_ENDED == (int)qn::PlayerState::Ended);
static_assert((int)PE_STATE_ERROR == (int)qn::PlayerState::Error);

static_assert((int)PE_QUALITY_CONFIRMED == (int)qn::LandingQuality::Confirmed);
static_assert((int)PE_QUALITY_ESTIMATED == (int)qn::LandingQuality::Estimated);

// Internal production defaults: the engine owns its decode worker thread;
// the queue is the only product knob. capacity 0 -> ~1 second at 48 kHz.
constexpr std::uint64_t kDefaultCapacityFrames = 48000;

qn::EngineConfig to_engine_config(const pe_config* c) {
    const std::uint64_t cap =
        (c != nullptr && c->capacity_frames > 0) ? c->capacity_frames
                                                 : kDefaultCapacityFrames;
    qn::EngineConfig cfg;
    cfg.capacity_frames = cap;
    cfg.read_chunk_frames = cap < 2048 ? cap / 2 : 1024;
    if (cfg.read_chunk_frames == 0) cfg.read_chunk_frames = 1;
    cfg.max_submit_frames = cap;  // internal fill bound; not a product knob
    cfg.max_channels = 8;
    cfg.worker_thread = true;  // production default: native worker thread
    return cfg;
}

// Convert a frame-domain value to microseconds at the rate the frame counts
// were captured with. Control ops (pe_seek) pass the engine's current rate —
// safe because the caller serializes control calls — and pe_get_snapshot
// passes the rate captured INSIDE the snapshot hold, never a post-lock read.
std::int64_t frames_to_us(std::int64_t frames, std::int32_t rate) {
    return rate > 0 ? qn::frames_to_us(frames, rate) : 0;
}

// Every public out parameter receives a deterministic value BEFORE
// validation/try, so "written on every return path where the pointer is
// valid" holds even for invalid-argument and exception paths (the engine
// then overwrites on success).
void init_seek_outs(std::int64_t* out_landing_us, std::int32_t* out_song_status) {
    if (out_landing_us) *out_landing_us = -1;
    if (out_song_status) *out_song_status = SONG_ERR_INVALID_ARGUMENT;
}

}  // namespace

extern "C" {

uint32_t player_engine_abi_version(void) { return PLAYER_ENGINE_ABI_VERSION; }

pe_engine* pe_create(const pe_config* config) {
    qn::PlayerEngine* engine = nullptr;
    try {
        engine = new qn::PlayerEngine(to_engine_config(config));
    } catch (...) {
        return nullptr;
    }
#if defined(QN_QIANQIAN_RUNTIME) && defined(_WIN32)
    // Compose the platform render thread. A renderer failure (e.g. thread
    // spawn) degrades to a silent runtime — create's contract stays "NULL
    // only on allocation failure", and the frozen position in the snapshot
    // is the honest "no output" signal.
    pe_runtime_handle* handle = new (std::nothrow) pe_runtime_handle{engine, nullptr};
    if (handle == nullptr) {
        delete engine;
        return nullptr;
    }
    try {
        handle->renderer = new qn::WasapiRenderer(*engine);
    } catch (...) {
        handle->renderer = nullptr;
    }
    return reinterpret_cast<pe_engine*>(handle);
#else
    return reinterpret_cast<pe_engine*>(engine);
#endif
}

void pe_destroy(pe_engine* engine) {
    if (engine == nullptr) return;
    try {
#if defined(QN_QIANQIAN_RUNTIME) && defined(_WIN32)
        pe_runtime_handle* handle = reinterpret_cast<pe_runtime_handle*>(engine);
        // Hard requirement: after destroy returns, no render thread may
        // touch the engine — the renderer destructor stops and joins its
        // thread before the engine is destroyed.
        delete handle->renderer;
        delete handle->engine;
        delete handle;
#else
        delete reinterpret_cast<qn::PlayerEngine*>(engine);
#endif
    } catch (...) {
        // Destructors must not throw; if one ever does, never let it cross.
    }
}

pe_status pe_open(pe_engine* engine, const song_io* io, int32_t* out_song_status) {
    if (out_song_status) *out_song_status = SONG_ERR_INVALID_ARGUMENT;
    if (engine == nullptr || io == nullptr) return PE_ERR_INVALID_ARGUMENT;
    try {
        return static_cast<pe_status>(
            self(engine)->open(*io, out_song_status));
    } catch (const std::bad_alloc&) {
        return PE_ERR_NO_MEMORY;
    } catch (...) {
        return PE_ERR_INTERNAL;
    }
}

pe_status pe_play(pe_engine* engine) {
    if (engine == nullptr) return PE_ERR_INVALID_ARGUMENT;
    try {
        return static_cast<pe_status>(self(engine)->play());
    } catch (const std::bad_alloc&) {
        return PE_ERR_NO_MEMORY;
    } catch (...) {
        return PE_ERR_INTERNAL;
    }
}

pe_status pe_pause(pe_engine* engine) {
    if (engine == nullptr) return PE_ERR_INVALID_ARGUMENT;
    try {
        self(engine)->pause();
        return PE_OK;
    } catch (...) {
        return PE_ERR_INTERNAL;
    }
}

pe_status pe_stop(pe_engine* engine, int32_t* out_song_status) {
    if (out_song_status) *out_song_status = SONG_ERR_INVALID_ARGUMENT;
    if (engine == nullptr) return PE_ERR_INVALID_ARGUMENT;
    try {
        return static_cast<pe_status>(
            self(engine)->stop(out_song_status));
    } catch (const std::bad_alloc&) {
        return PE_ERR_NO_MEMORY;
    } catch (...) {
        return PE_ERR_INTERNAL;
    }
}

pe_status pe_seek(pe_engine* engine, int64_t position_us, int64_t* out_landing_us,
                  int32_t* out_song_status) {
    init_seek_outs(out_landing_us, out_song_status);
    if (engine == nullptr) return PE_ERR_INVALID_ARGUMENT;
    try {
        qn::PlayerEngine* e = self(engine);
        std::int64_t landing_frames = 0;
        const pe_status st = static_cast<pe_status>(
            e->seek(position_us, &landing_frames, out_song_status));
        if (st == PE_OK && out_landing_us != nullptr) {
            // Landing on success only; a failed seek leaves the deterministic
            // -1 default. PE_ERR_OPEN_FAILED / PE_ERR_SEEK_FAILED carry a
            // SongCore status in *out_song_status; PE_ERR_INTERNAL does not
            // (SongCore was never consulted on that path) — the pre-value
            // below stays and must not be read as a root cause.
            *out_landing_us = frames_to_us(landing_frames, e->source_rate());
        }
        return st;
    } catch (const std::bad_alloc&) {
        return PE_ERR_NO_MEMORY;
    } catch (...) {
        return PE_ERR_INTERNAL;
    }
}

pe_status pe_get_snapshot(pe_engine* engine, pe_snapshot* out) {
    // Deterministic default BEFORE validation (the header contract: every
    // field is written on every return path where `out` itself is valid —
    // including engine == NULL): EMPTY, unknown duration (-1), quiet
    // counters, empty last_error.
    if (out != nullptr) {
        std::memset(out, 0, sizeof *out);
        out->state = PE_STATE_EMPTY;
        out->duration_us = -1;
    }
    if (engine == nullptr || out == nullptr) return PE_ERR_INVALID_ARGUMENT;
    try {
        const qn::PlayerEngine* e = self(engine);
        const qn::EngineSnapshot s = e->snapshot();
        // One coherent instant: position, duration and
        // sample_rate all derive from the rate captured inside the snapshot
        // hold — never a post-lock e->source_rate() read.
        const std::int32_t rate = s.source_rate;
        out->state = static_cast<pe_state>(s.state);
        out->position_us = frames_to_us(s.media_position_frames, rate);
        out->duration_us =
            s.duration_frames < 0 ? -1 : frames_to_us(s.duration_frames, rate);
        out->duration_known = s.duration_known ? 1u : 0u;
        out->position_quality = static_cast<uint8_t>(s.position_quality);
        out->buffered_frames = s.queued_media_frames;
        out->underrun_count = s.underrun_count;
        out->sample_rate = rate;
        std::memcpy(out->last_error, s.last_error, sizeof out->last_error);
        return PE_OK;
    } catch (const std::bad_alloc&) {
        return PE_ERR_NO_MEMORY;
    } catch (...) {
        return PE_ERR_INTERNAL;
    }
}

}  // extern "C"
