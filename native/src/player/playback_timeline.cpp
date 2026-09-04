#include "playback_timeline.hpp"

namespace qn {

void PlaybackTimeline::reset() {
    head_ = 0;
    count_ = 0;
    render_rel_ = 0;
    render_off_ = 0;
    trimmed_output_ = 0;
    trimmed_media_ = 0;
    output_endpoint_.store(0);
    rendered_output_.store(0);
    span_media_total_.store(0);
    span_media_rendered_.store(0);
    overflow_ = false;
}

bool PlaybackTimeline::append(SpanKind kind, std::uint64_t frames) {
    if (frames == 0) return true;
    if (overflow_) return false;  // sticky fail-closed
    if (count_ > 0) {
        Span& last = spans_[(head_ + count_ - 1) % kCapacity];
        // Coalesce a contiguous same-kind span: the common case in normal
        // playback collapses media runs (and underrun runs) into one span
        // regardless of how many periods they span. EXCEPT when the render
        // cursor is currently inside that last span: extending the span the
        // cursor is chasing re-opens the boundary ahead of it, so a render
        // that keeps pace with submission (480 in / 480 out) can never cross
        // it — zero relative progress, trim stalls, and the pending window
        // drifts without bound. A fresh span lets the cursor cross the old
        // one at its own pace while the new tail accumulates behind it.
        if (last.kind == kind && render_rel_ != count_ - 1) {
            const bool cursor_past_end = (render_rel_ == count_);
            const std::uint64_t old_len = last.output_end - last.output_begin;
            last.output_end += frames;
            output_endpoint_.fetch_add(frames);
            submitted_output_total_.fetch_add(frames);
            if (kind == SpanKind::Media) {
                span_media_total_.fetch_add(frames);
                submitted_media_total_.fetch_add(frames);
            }
            if (cursor_past_end) {
                // The whole old span was already rendered and the cursor sat
                // at its end; the extension re-opens the span, so pull the
                // render cursor back to the junction (render_off_ = old
                // length) or the new tail would never render.
                render_rel_ = count_ - 1;
                render_off_ = old_len;
            }
            return true;
        }
    }
    if (count_ == kCapacity) {
        trim_rendered();
        if (count_ == kCapacity) {
            // No representation left: fail closed rather than grow or
            // corrupt. Correctness is no longer representable; the engine
            // stops submission with a diagnostic.
            overflow_ = true;
            return false;
        }
    }
    spans_[(head_ + count_) % kCapacity] =
        Span{output_endpoint_.load(), output_endpoint_.load() + frames, kind};
    ++count_;
    output_endpoint_.fetch_add(frames);
    submitted_output_total_.fetch_add(frames);
    if (kind == SpanKind::Media) {
        span_media_total_.fetch_add(frames);
        submitted_media_total_.fetch_add(frames);
    }
    return true;
}

PlaybackTimeline::RenderSplit PlaybackTimeline::advance(std::uint64_t frames) {
    RenderSplit split{0, 0};
    std::uint64_t remaining = frames;
    while (remaining > 0 && render_rel_ < count_) {
        Span& span = spans_[(head_ + render_rel_) % kCapacity];
        const std::uint64_t len = span.output_end - span.output_begin;
        const std::uint64_t avail = len - render_off_;
        const std::uint64_t take = avail < remaining ? avail : remaining;
        if (span.kind == SpanKind::Media) {
            split.media += take;
            span_media_rendered_.fetch_add(take);
        } else {
            split.gap += take;
        }
        render_off_ += take;
        remaining -= take;
        if (render_off_ == len) {
            ++render_rel_;
            render_off_ = 0;
        }
    }
    const std::uint64_t advanced = frames - remaining;
    rendered_output_.fetch_add(advanced);
    rendered_output_total_.fetch_add(advanced);
    rendered_media_total_.fetch_add(split.media);
    rendered_gap_total_.fetch_add(split.gap);
    if (count_ >= kSoftTrimLimit) trim_rendered();
    return split;
}

PlaybackTimeline::Discard PlaybackTimeline::invalidate() {
    Discard d{pending_output(), pending_media()};
    discarded_output_total_.fetch_add(d.output);
    discarded_media_total_.fetch_add(d.media);
    reset();
    return d;
}

void PlaybackTimeline::trim_rendered() {
    // Every span before the render cursor is fully rendered; fold it into
    // the prefix anchors and drop it from the live store. O(1) amortized.
    while (count_ > 0 && render_rel_ > 0) {
        const Span& s = spans_[head_];
        const std::uint64_t len = s.output_end - s.output_begin;
        trimmed_output_ += len;
        if (s.kind == SpanKind::Media) trimmed_media_ += len;
        head_ = (head_ + 1) % kCapacity;
        --count_;
        --render_rel_;
    }
}

std::uint64_t PlaybackTimeline::media_at_output(std::uint64_t output_pos) const {
    if (output_pos <= trimmed_output_) return trimmed_media_;
    std::uint64_t media = trimmed_media_;
    for (std::size_t i = 0; i < count_; ++i) {
        const Span& span = spans_[(head_ + i) % kCapacity];
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
