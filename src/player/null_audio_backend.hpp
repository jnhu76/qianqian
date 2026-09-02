// null_audio_backend.hpp — deterministic manual-tick audio backend.
//
// The stand-in for WASAPI (docs/player-engine.md): a fake device driven
// entirely by explicit submit/render ticks. It depends on no wall clock, no
// sleeps, no OS audio API, and no hardware; tests drive it manually and the
// submit/render entry points stay separate so the suite can prove that
// submitted output is not rendered output (docs §2.4):
//
//   submit_media/submit_silence   PCM queue -> backend buffer (NOT audible)
//   render(n)                     device consumes n frames — proof of
//                                 playout; the deterministic device-domain
//                                 authority standing in for the future
//                                 IAudioClock reading
//
// This backend is test scaffolding shipped until the WASAPI phase: its
// buffers may allocate (std::vector/std::deque), which the production
// realtime contract forbids on a real device path. The engine-side realtime
// rules still hold — the backend never calls SongCore, never decodes, never
// blocks on the producer.
#ifndef QIANQIAN_PLAYER_NULL_AUDIO_BACKEND_HPP
#define QIANQIAN_PLAYER_NULL_AUDIO_BACKEND_HPP

#include <cstdint>
#include <deque>
#include <vector>

namespace qn {

class NullAudioBackend {
public:
    // Stream layout for the current song; the engine configures it at open
    // (production-realistic: backends are told the format they will get).
    void configure(std::int32_t channels) { channels_ = channels; }

    // -- submit side (engine audio callback) ----------------------------------
    // Copy `frames` of interleaved media PCM (frames*channels floats) into
    // the pending device buffer, attributed to `segment` (the test-side
    // rendered-content log is keyed by it).
    void submit_media(const float* pcm, std::uint64_t frames, std::uint64_t segment);
    // Append `frames` of GAP silence (zero samples, stored frame-count only).
    void submit_silence(std::uint64_t frames, std::uint64_t segment);

    // -- render progression (device clock) -------------------------------------
    // The device has consumed up to `frames` of pending output; returns the
    // count actually rendered (<= pending). Rendered media PCM is preserved
    // in the per-segment rendered log for the content-continuity check.
    // The frame identities live in the TEST FAKE's sample encoding — a
    // production engine never sees them.
    std::uint64_t render(std::uint64_t frames);

    // -- queries -----------------------------------------------------------------
    std::uint64_t pending() const { return pending_frames_; }
    std::int32_t channels() const { return channels_; }

    // Test-only observables: rendered media PCM per submit-time segment.
    struct RenderedLog {
        std::uint64_t segment;
        std::vector<float> pcm;  // interleaved media frames (GAP skipped)
    };
    const std::vector<RenderedLog>& rendered_log() const { return rendered_log_; }
    void clear_rendered_log() { rendered_log_.clear(); }

    // Drop everything (commit boundaries invalidate pending device output).
    void reset();

private:
    struct Block {
        std::uint64_t segment;
        bool media;
        std::uint64_t frames;       // block length in frames
        std::vector<float> pcm;     // media payload (empty for silence)
        std::uint64_t rendered = 0; // frames already rendered out of block
    };

    std::int32_t channels_ = 2;
    std::deque<Block> pending_;
    std::uint64_t pending_frames_ = 0;
    std::vector<RenderedLog> rendered_log_;
};

}  // namespace qn

#endif  // QIANQIAN_PLAYER_NULL_AUDIO_BACKEND_HPP
