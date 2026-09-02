// playback_timeline.hpp — device→media mapping + output accounting (native).
//
// Native mirror of the Phase-1 oracle's OutputSpan model
// (tools/player_model/player_model.py). The output domain and the media
// domain are two coordinate systems; the timeline keeps the submitted-output
// span list that maps a rendered device position back onto the media
// timeline (docs/player-engine.md §2.5):
//
//   OutputSpan { output_begin, output_end, kind = MEDIA | GAP }
//
// MEDIA spans carry real media payload; GAP spans (underrun / preroll / EOS
// silence) have device duration and ZERO media duration. Output coordinates
// are per-generation: each commit (open/seek/stop/restart) restarts the
// output timeline at 0.
//
// Representation: spans of the CURRENT generation are kept appended for the
// whole generation (the mapping is queryable over already-rendered history,
// like the oracle's list) and dropped wholesale at the commit. Growth is
// bounded by one generation's total submitted output (media duration plus
// gap silence) — the engine's bounded ring bounds submission, and every
// commit clears it.
//
// Lifetime conservation, maintained here and checked after every engine op:
//   submitted_output == pending_output + rendered_output + discarded_output
//   submitted_media  == pending_media  + rendered_media + discarded_media
//   rendered_output  == rendered_media + rendered_gap
#ifndef QIANQIAN_PLAYER_PLAYBACK_TIMELINE_HPP
#define QIANQIAN_PLAYER_PLAYBACK_TIMELINE_HPP

#include <cstddef>
#include <cstdint>
#include <vector>

namespace qn {

enum class SpanKind : std::uint8_t { Media, Gap };

class PlaybackTimeline {
public:
    struct RenderSplit {
        std::uint64_t media;  // MEDIA frames proven rendered
        std::uint64_t gap;    // GAP frames proven rendered
    };

    void reset();  // generation restart (commit); keeps lifetime totals

    // -- submit side (engine audio callback, current generation) ------------
    // Appends one span at the end of the submitted output.
    void append(SpanKind kind, std::uint64_t frames);

    // -- render side (device clock evidence) ---------------------------------
    // Advance `frames` of proven-rendered output through the span list from
    // the render cursor. Rendering never exceeds the submitted endpoint.
    RenderSplit advance(std::uint64_t frames);

    // -- commit side ----------------------------------------------------------
    // Discard all pending output of the dying generation (a submitted-but-
    // unrendered span can never advance the next segment). Returns the
    // discarded {output, media} counts folded into the lifetime totals.
    struct Discard { std::uint64_t output; std::uint64_t media; };
    Discard invalidate();

    // -- queries ---------------------------------------------------------------
    // Pending output of the CURRENT generation (submitted - rendered).
    std::uint64_t pending_output() const {
        return output_endpoint_ - rendered_output_;
    }
    // Pending MEDIA payload of the current generation.
    std::uint64_t pending_media() const {
        return span_media_total_ - span_media_rendered_;
    }
    std::uint64_t submitted_endpoint() const { return output_endpoint_; }
    std::uint64_t rendered_current() const { return rendered_output_; }

    // Map an output-domain position onto the media timeline relative to the
    // segment base (docs §2.5; the s3 gate probes this mapping). GAP spans
    // contribute zero media; positions beyond the submitted endpoint clamp
    // to the last mapped media endpoint. Valid for the whole generation,
    // including already-rendered history.
    std::uint64_t media_at_output(std::uint64_t output_pos) const;

    // Lifetime totals (conservation laws above).
    std::uint64_t submitted_output_total() const { return submitted_output_total_; }
    std::uint64_t rendered_output_total() const { return rendered_output_total_; }
    std::uint64_t discarded_output_total() const { return discarded_output_total_; }
    std::uint64_t submitted_media_total() const { return submitted_media_total_; }
    std::uint64_t rendered_media_total() const { return rendered_media_total_; }
    std::uint64_t rendered_gap_total() const { return rendered_gap_total_; }
    std::uint64_t discarded_media_total() const { return discarded_media_total_; }

    std::size_t span_count() const { return spans_.size(); }

private:
    struct Span {
        std::uint64_t output_begin;
        std::uint64_t output_end;
        SpanKind kind;
    };

    std::vector<Span> spans_;        // whole current generation
    std::size_t render_span_ = 0;    // first span not fully rendered
    std::uint64_t render_off_ = 0;   // rendered frames inside spans_[render_span_]

    // Per-generation accounting.
    std::uint64_t output_endpoint_ = 0;   // submitted output this generation
    std::uint64_t rendered_output_ = 0;   // rendered output this generation
    std::uint64_t span_media_total_ = 0;  // media payload submitted (gen)
    std::uint64_t span_media_rendered_ = 0;

    // Lifetime totals.
    std::uint64_t submitted_output_total_ = 0;
    std::uint64_t rendered_output_total_ = 0;
    std::uint64_t discarded_output_total_ = 0;
    std::uint64_t submitted_media_total_ = 0;
    std::uint64_t rendered_media_total_ = 0;
    std::uint64_t rendered_gap_total_ = 0;
    std::uint64_t discarded_media_total_ = 0;
};

}  // namespace qn

#endif  // QIANQIAN_PLAYER_PLAYBACK_TIMELINE_HPP
