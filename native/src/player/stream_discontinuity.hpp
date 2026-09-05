// stream_discontinuity.hpp — renderer-side segment-discontinuity decision.
//
// The pure, platform-neutral half of the playing-seek stale-audio corrective
// (#40): every engine commit (open / seek / stop / restart-from-ENDED) bumps
// the per-commit `segment` that fill_output()/advance_render() report; a
// device stream that still holds submitted-but-unrendered PCM of an older
// segment must be physically dropped (WASAPI Stop + Reset) before any PCM of
// the new segment is submitted, or the old audio plays out first.
//
// This type owns ONLY the decision, so the Linux gates can exercise the
// commit-detection contract deterministically without WASAPI; the Windows
// WasapiRenderer maps the DropDeviceBuffer action onto the real
// Stop/Reset/accounting-rebase/swr-drain sequence
// (docs/architecture/platform-audio.md, invariants).
//
// Pause is deliberately NOT a discontinuity: it does not commit a segment,
// and the frozen pause semantics keep pending device output pending (it
// resumes with content on play). The decision therefore fires exactly when a
// SUPERSEDED segment's PCM could still become audible — never on steady
// playback, never on pause/resume, never on underrun/preroll gaps (they
// carry the current segment).
#ifndef QIANQIAN_PLAYER_STREAM_DISCONTINUITY_HPP
#define QIANQIAN_PLAYER_STREAM_DISCONTINUITY_HPP

#include <cstdint>

namespace qn {

class StreamDiscontinuity {
public:
    // Unchanged: the engine segment is compatible with the device buffer.
    // DropDeviceBuffer: the stream still holds an older segment's PCM.
    enum class Action { Unchanged, DropDeviceBuffer };

    // A (re)opened stream: device buffer empty, nothing submitted yet.
    void stream_opened() { holds_ = false; }

    // Frames of `segment` were released into the device buffer.
    void submitted(std::uint64_t segment) {
        holds_ = true;
        segment_ = segment;
    }

    // The device buffer was dropped (Stop + Reset) or the session torn down;
    // nothing submitted-but-unrendered can remain.
    void device_buffer_dropped() { holds_ = false; }

    // True while the open stream may still hold unrendered PCM of `segment`.
    bool holds_pcm() const { return holds_; }
    std::uint64_t segment() const { return segment_; }

    // Decision for one engine observation (a fill/probe) that reported the
    // engine at `segment`.
    Action on_engine_segment(std::uint64_t segment) const {
        return holds_ && segment != segment_ ? Action::DropDeviceBuffer
                                             : Action::Unchanged;
    }

private:
    bool holds_ = false;        // device buffer may hold unrendered PCM
    std::uint64_t segment_ = 0; // the segment that PCM belongs to
};

}  // namespace qn

#endif  // QIANQIAN_PLAYER_STREAM_DISCONTINUITY_HPP
