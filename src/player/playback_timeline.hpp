// playback_timeline.hpp — bounded device→media mapping + output accounting.
//
// The output domain and the media domain are two coordinate systems; the
// timeline keeps the submitted-output span list that maps a rendered device
// position back onto the media timeline (docs/player-engine.md §2.5):
//
//   OutputSpan { output_begin, output_end, kind = MEDIA | GAP }
//
// MEDIA spans carry real media payload; GAP spans (underrun / preroll / EOS
// silence) have device duration and ZERO media duration. Output coordinates
// are per-generation: each commit (open/seek/stop/restart) restarts the
// output timeline at 0.
//
// Production representation: the span store is a FIXED-CAPACITY ring of
// preallocated spans (kCapacity). append() coalesces a contiguous same-kind
// span into its predecessor and advance() trims fully-rendered spans from
// the front, folding them into accumulated prefix anchors (trimmed_output_/
// trimmed_media_). Live memory is therefore bounded by the backend pending/
// output window, NOT by song duration: an arbitrarily long playback keeps
// span_count() small. append()/advance()/media_at_output() never allocate
// and never grow.
//
// Mapping over trimmed history: positions at or after the render cursor map
// exactly (prefix anchor + live spans). A position inside already-trimmed
// history maps to the prefix anchor; production only ever maps positions
// at/after the render cursor (the device clock reads the current output
// position).
//
// Capacity policy: if a pathological pattern still fills the store after
// coalescing and trimming, append() FAILS CLOSED (returns false and sets a
// sticky overflow flag) instead of growing or corrupting the mapping. The
// engine treats that as a deterministic diagnostic stop.
//
// Lifetime conservation, maintained here and checked after every engine op:
//   submitted_output == pending_output + rendered_output + discarded_output
//   submitted_media  == pending_media  + rendered_media + discarded_media
//   rendered_output  == rendered_media + rendered_gap
#ifndef QIANQIAN_PLAYER_PLAYBACK_TIMELINE_HPP
#define QIANQIAN_PLAYER_PLAYBACK_TIMELINE_HPP

#include <array>
#include <atomic>
#include <cstddef>
#include <cstdint>

namespace qn {

enum class SpanKind : std::uint8_t { Media, Gap };

// Concurrency model (docs §9): the span store and its cursors have exactly
// ONE runtime owner — the single backend/device thread that calls
// append()/advance(); the control thread touches them only after quiescing
// the backend (commit path). The scalar accounting counters below are
// ATOMIC so the control plane's polled snapshot (pe_get_snapshot may be
// polled concurrently) can read them lock-free while the owner mutates;
// individually coherent, cross-field conservation holds at quiescence.
class PlaybackTimeline {
public:
    // Fixed internal span capacity. Live spans are bounded far below this by
    // coalescing + trimming; the headroom is a safety margin so only a truly
    // pathological pattern can reach it (then fail-closed).
    static constexpr std::size_t kCapacity = 256;

    // Trimming is lazy: fully-rendered spans are folded into the prefix
    // anchors only once the live store grows past this soft limit. Small
    // generations (the semantic gate's mapping probes) therefore keep their
    // ENTIRE history exact while arbitrarily long playback stays bounded to
    // this window + headroom. 8 is the smallest bound that keeps
    // whole-history exactness for the semantic gate (3 spans) and holds the
    // pathological MEDIA/GAP alternation-with-render store to <= 8 live
    // spans.
    static constexpr std::size_t kSoftTrimLimit = 8;

    struct RenderSplit {
        std::uint64_t media;  // MEDIA frames proven rendered
        std::uint64_t gap;    // GAP frames proven rendered
    };

    void reset();  // generation restart (commit); keeps lifetime totals

    // -- submit side (engine audio callback, current generation) ------------
    // Appends one span at the end of the submitted output, coalescing a
    // contiguous same-kind span into its predecessor. Never allocates.
    // Returns false (and sets the sticky overflow flag) when the fixed store
    // is full after trimming — fail-closed, never growth, never corruption.
    bool append(SpanKind kind, std::uint64_t frames);

    // -- render side (device clock evidence) ---------------------------------
    // Advance `frames` of proven-rendered output through the span list from
    // the render cursor. Rendering never exceeds the submitted endpoint.
    // Trims fully-rendered spans into the prefix anchors. Never allocates.
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
        return output_endpoint_.load() - rendered_output_.load();
    }
    // Pending MEDIA payload of the current generation.
    std::uint64_t pending_media() const {
        return span_media_total_.load() - span_media_rendered_.load();
    }
    std::uint64_t submitted_endpoint() const { return output_endpoint_.load(); }
    std::uint64_t rendered_current() const { return rendered_output_.load(); }

    // Map an output-domain position onto the media timeline relative to the
    // segment base (docs §2.5; the s3 gate probes this mapping). GAP spans
    // contribute zero media; positions at or beyond the live submitted
    // endpoint clamp to the last mapped media endpoint. Positions inside the
    // trimmed rendered history map to the trimmed prefix anchor. Never
    // allocates.
    std::uint64_t media_at_output(std::uint64_t output_pos) const;

    // Lifetime totals (conservation laws above).
    std::uint64_t submitted_output_total() const { return submitted_output_total_.load(); }
    std::uint64_t rendered_output_total() const { return rendered_output_total_.load(); }
    std::uint64_t discarded_output_total() const { return discarded_output_total_.load(); }
    std::uint64_t submitted_media_total() const { return submitted_media_total_.load(); }
    std::uint64_t rendered_media_total() const { return rendered_media_total_.load(); }
    std::uint64_t rendered_gap_total() const { return rendered_gap_total_.load(); }
    std::uint64_t discarded_media_total() const { return discarded_media_total_.load(); }

    std::size_t span_count() const { return count_; }
    std::uint64_t trimmed_output() const { return trimmed_output_; }
    std::uint64_t trimmed_media() const { return trimmed_media_; }
    bool overflow() const { return overflow_; }

private:
    struct Span {
        std::uint64_t output_begin;
        std::uint64_t output_end;
        SpanKind kind;
    };

    void trim_rendered();  // fold fully-rendered spans into the anchors

    std::array<Span, kCapacity> spans_;  // fixed store, never grows
    std::size_t head_ = 0;               // oldest live span (circular)
    std::size_t count_ = 0;              // live span count
    std::size_t render_rel_ = 0;         // live-relative first unrendered span
    std::uint64_t render_off_ = 0;       // rendered frames inside that span

    // Prefix anchors for trimmed rendered history.
    std::uint64_t trimmed_output_ = 0;
    std::uint64_t trimmed_media_ = 0;

    // Per-generation accounting (atomics: the polled snapshot reads these
    // while the device thread owns mutation; single writer otherwise).
    std::atomic<std::uint64_t> output_endpoint_{0};   // submitted output this generation
    std::atomic<std::uint64_t> rendered_output_{0};   // rendered output this generation
    std::atomic<std::uint64_t> span_media_total_{0};  // media payload submitted (gen)
    std::atomic<std::uint64_t> span_media_rendered_{0};

    // Lifetime totals (same reader/writer split).
    std::atomic<std::uint64_t> submitted_output_total_{0};
    std::atomic<std::uint64_t> rendered_output_total_{0};
    std::atomic<std::uint64_t> discarded_output_total_{0};
    std::atomic<std::uint64_t> submitted_media_total_{0};
    std::atomic<std::uint64_t> rendered_media_total_{0};
    std::atomic<std::uint64_t> rendered_gap_total_{0};
    std::atomic<std::uint64_t> discarded_media_total_{0};

    bool overflow_ = false;  // sticky fail-closed diagnostic
};

}  // namespace qn

#endif  // QIANQIAN_PLAYER_PLAYBACK_TIMELINE_HPP
