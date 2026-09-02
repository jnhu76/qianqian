#include "playback_timeline.hpp"

namespace qn {

void PlaybackTimeline::reset() {
    spans_.clear();
    render_span_ = 0;
    render_off_ = 0;
    output_endpoint_ = 0;
    rendered_output_ = 0;
    span_media_total_ = 0;
    span_media_rendered_ = 0;
}

void PlaybackTimeline::append(SpanKind kind, std::uint64_t frames) {
    if (frames == 0) return;
    spans_.push_back(Span{output_endpoint_, output_endpoint_ + frames, kind});
    output_endpoint_ += frames;
    submitted_output_total_ += frames;
    if (kind == SpanKind::Media) {
        span_media_total_ += frames;
        submitted_media_total_ += frames;
    }
}

PlaybackTimeline::RenderSplit PlaybackTimeline::advance(std::uint64_t frames) {
    RenderSplit split{0, 0};
    std::uint64_t remaining = frames;
    while (remaining > 0 && render_span_ < spans_.size()) {
        const Span& span = spans_[render_span_];
        const std::uint64_t avail = (span.output_end - span.output_begin) - render_off_;
        const std::uint64_t take = avail < remaining ? avail : remaining;
        if (span.kind == SpanKind::Media) {
            split.media += take;
            span_media_rendered_ += take;
        } else {
            split.gap += take;
        }
        render_off_ += take;
        remaining -= take;
        if (render_off_ == span.output_end - span.output_begin) {
            ++render_span_;
            render_off_ = 0;
        }
    }
    const std::uint64_t advanced = frames - remaining;
    rendered_output_ += advanced;
    rendered_output_total_ += advanced;
    rendered_media_total_ += split.media;
    rendered_gap_total_ += split.gap;
    return split;
}

PlaybackTimeline::Discard PlaybackTimeline::invalidate() {
    Discard d{pending_output(), pending_media()};
    discarded_output_total_ += d.output;
    discarded_media_total_ += d.media;
    reset();
    return d;
}

std::uint64_t PlaybackTimeline::media_at_output(std::uint64_t output_pos) const {
    std::uint64_t media = 0;
    for (const Span& span : spans_) {
        if (output_pos <= span.output_begin) break;
        if (span.kind == SpanKind::Media) {
            const std::uint64_t end =
                output_pos < span.output_end ? output_pos : span.output_end;
            media += end - span.output_begin;
        }
    }
    return media;
}

}  // namespace qn
