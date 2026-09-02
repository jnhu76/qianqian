// player_engine_c.cpp — C ABI transcription of the internal PlayerEngine.
//
// Every function forwards 1:1 to qn::PlayerEngine; enum numeric parity is
// asserted statically so pe_status/pe_state can travel as plain integers.
// The snapshot is copied field-by-field (never as raw memory), so the C
// struct layout carries no constraint on the C++ one.
#include <cstring>

#include "player_engine.h"
#include "player_engine.hpp"

namespace {

static_assert((int)PE_OK == (int)qn::PlayerStatus::Ok);
static_assert((int)PE_ERR_ILLEGAL_CALL == (int)qn::PlayerStatus::ErrIllegalCall);
static_assert((int)PE_ERR_OPEN_FAILED == (int)qn::PlayerStatus::ErrOpenFailed);
static_assert((int)PE_ERR_SEEK_FAILED == (int)qn::PlayerStatus::ErrSeekFailed);

static_assert((int)PE_STATE_EMPTY == (int)qn::PlayerState::Empty);
static_assert((int)PE_STATE_READY == (int)qn::PlayerState::Ready);
static_assert((int)PE_STATE_PLAYING == (int)qn::PlayerState::Playing);
static_assert((int)PE_STATE_PAUSED == (int)qn::PlayerState::Paused);
static_assert((int)PE_STATE_ENDED == (int)qn::PlayerState::Ended);
static_assert((int)PE_STATE_ERROR == (int)qn::PlayerState::Error);

static_assert((int)PE_QUALITY_CONFIRMED == (int)qn::LandingQuality::Confirmed);
static_assert((int)PE_QUALITY_ESTIMATED == (int)qn::LandingQuality::Estimated);

qn::EngineConfig to_engine_config(const pe_config* c) {
    qn::EngineConfig cfg;
    cfg.capacity_frames = c->capacity_frames;
    cfg.read_chunk_frames = c->read_chunk_frames;
    cfg.max_submit_frames = c->max_submit_frames;
    cfg.max_channels = c->max_channels;
    cfg.worker_thread = c->worker_thread != 0;
    return cfg;
}

}  // namespace

extern "C" {

uint32_t player_engine_abi_version(void) { return PLAYER_ENGINE_ABI_VERSION; }

pe_engine* pe_create(const pe_config* config) {
    // Fail-closed before the C++ ctor: its bad-config path aborts, but a C
    // consumer must get a NULL, not a dead process.
    if (config == nullptr || config->capacity_frames == 0 ||
        config->read_chunk_frames == 0 || config->max_submit_frames == 0 ||
        config->max_channels <= 0 ||
        config->read_chunk_frames > config->capacity_frames) {
        return nullptr;
    }
    return reinterpret_cast<pe_engine*>(new qn::PlayerEngine(to_engine_config(config)));
}

void pe_destroy(pe_engine* engine) {
    delete reinterpret_cast<qn::PlayerEngine*>(engine);
}

pe_status pe_open(pe_engine* engine, const song_io* io, int32_t* out_song_status) {
    if (engine == nullptr || io == nullptr) return PE_ERR_ILLEGAL_CALL;
    return static_cast<pe_status>(
        reinterpret_cast<qn::PlayerEngine*>(engine)->open(*io, out_song_status));
}

pe_status pe_play(pe_engine* engine) {
    if (engine == nullptr) return PE_ERR_ILLEGAL_CALL;
    return static_cast<pe_status>(reinterpret_cast<qn::PlayerEngine*>(engine)->play());
}

pe_status pe_pause(pe_engine* engine) {
    if (engine == nullptr) return PE_ERR_ILLEGAL_CALL;
    reinterpret_cast<qn::PlayerEngine*>(engine)->pause();
    return PE_OK;
}

pe_status pe_stop(pe_engine* engine, int32_t* out_song_status) {
    if (engine == nullptr) return PE_ERR_ILLEGAL_CALL;
    return static_cast<pe_status>(
        reinterpret_cast<qn::PlayerEngine*>(engine)->stop(out_song_status));
}

pe_status pe_seek(pe_engine* engine, int64_t position_us, int64_t* out_landing_frames,
                  int32_t* out_song_status) {
    if (engine == nullptr) return PE_ERR_ILLEGAL_CALL;
    return static_cast<pe_status>(reinterpret_cast<qn::PlayerEngine*>(engine)->seek(
        position_us, out_landing_frames, out_song_status));
}

pe_submit_report pe_submit(pe_engine* engine, uint64_t period_frames) {
    pe_submit_report r{0, 0, 0, "idle"};
    if (engine == nullptr) return r;
    const qn::SubmitReport rep =
        reinterpret_cast<qn::PlayerEngine*>(engine)->submit(period_frames);
    r.segment = rep.segment;
    r.media_frames = rep.media_frames;
    r.silence_frames = rep.silence_frames;
    r.kind = rep.kind;
    return r;
}

pe_render_report pe_render(pe_engine* engine, int64_t frames, int64_t generation) {
    pe_render_report r{0, "stale", 0, 0, 0};
    if (engine == nullptr) return r;
    const qn::RenderReport rep =
        reinterpret_cast<qn::PlayerEngine*>(engine)->backend_render(frames, generation);
    r.segment = rep.segment;
    r.kind = rep.kind;
    r.rendered_output_frames = rep.rendered_output_frames;
    r.rendered_media_frames = rep.rendered_media_frames;
    r.generation = rep.generation;
    return r;
}

pe_status pe_get_snapshot(pe_engine* engine, pe_snapshot* out) {
    if (engine == nullptr || out == nullptr) return PE_ERR_ILLEGAL_CALL;
    const qn::EngineSnapshot s = reinterpret_cast<qn::PlayerEngine*>(engine)->snapshot();
    out->state = static_cast<pe_state>(s.state);
    out->media_position_frames = s.media_position_frames;
    out->position_quality = static_cast<uint8_t>(s.position_quality);
    out->decoded_source_position = s.decoded_source_position;
    out->duration_frames = s.duration_frames;
    out->duration_known = s.duration_known ? 1u : 0u;
    out->queued_media_frames = s.queued_media_frames;
    out->capacity_frames = s.capacity_frames;
    out->epoch = s.epoch;
    out->segment = s.segment;
    out->source_eof = s.source_eof ? 1u : 0u;
    out->underrun_count = s.underrun_count;
    out->underrun_silence_output_frames = s.underrun_silence_output_frames;
    out->preroll_events = s.preroll_events;
    out->preroll_silence_output_frames = s.preroll_silence_output_frames;
    out->eos_silence_output_frames = s.eos_silence_output_frames;
    out->decoded_source_frames = s.decoded_source_frames;
    out->submitted_output_frames = s.submitted_output_frames;
    out->rendered_output_frames = s.rendered_output_frames;
    out->pending_output_frames = s.pending_output_frames;
    out->discarded_output_frames = s.discarded_output_frames;
    out->submitted_media_frames = s.submitted_media_frames;
    out->rendered_media_frames = s.rendered_media_frames;
    out->rendered_gap_output_frames = s.rendered_gap_output_frames;
    out->pending_media_frames = s.pending_media_frames;
    out->discarded_output_media_frames = s.discarded_output_media_frames;
    out->discarded_stale_media_frames = s.discarded_stale_media_frames;
    out->stale_render_events = s.stale_render_events;
    out->in_flight_frames = s.in_flight_frames;
    out->ring_produced_total = s.ring_produced_total;
    out->ring_consumed_total = s.ring_consumed_total;
    out->ring_discarded_total = s.ring_discarded_total;
    std::memcpy(out->last_error, s.last_error, sizeof out->last_error);
    return PE_OK;
}

}  // extern "C"
