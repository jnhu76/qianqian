#include "null_audio_backend.hpp"

#include <cstring>

namespace qn {

void NullAudioBackend::submit_media(const float* pcm, std::uint64_t frames,
                                    std::uint64_t segment) {
    if (frames == 0) return;
    Block b;
    b.segment = segment;
    b.media = true;
    b.frames = frames;
    b.pcm.assign(pcm, pcm + static_cast<std::size_t>(frames * channels_));
    pending_.push_back(std::move(b));
    pending_frames_ += frames;
}

void NullAudioBackend::submit_silence(std::uint64_t frames, std::uint64_t segment) {
    if (frames == 0) return;
    Block b;
    b.segment = segment;
    b.media = false;
    b.frames = frames;
    pending_.push_back(std::move(b));
    pending_frames_ += frames;
}

std::uint64_t NullAudioBackend::render(std::uint64_t frames) {
    std::uint64_t remaining = frames;
    const std::size_t stride = static_cast<std::size_t>(channels_);
    while (remaining > 0 && !pending_.empty()) {
        Block& b = pending_.front();
        const std::uint64_t avail = b.frames - b.rendered;
        const std::uint64_t take = avail < remaining ? avail : remaining;
        if (b.media && take > 0) {
            // Preserve the rendered media payload for the content oracle.
            RenderedLog* log = nullptr;
            if (!rendered_log_.empty() && rendered_log_.back().segment == b.segment) {
                log = &rendered_log_.back();
            } else {
                rendered_log_.emplace_back();
                rendered_log_.back().segment = b.segment;
                log = &rendered_log_.back();
            }
            const float* src = b.pcm.data() + static_cast<std::size_t>(b.rendered) * stride;
            log->pcm.insert(log->pcm.end(), src, src + static_cast<std::size_t>(take) * stride);
        }
        b.rendered += take;
        remaining -= take;
        pending_frames_ -= take;
        if (b.rendered == b.frames) pending_.pop_front();
    }
    return frames - remaining;
}

void NullAudioBackend::reset() {
    pending_.clear();
    pending_frames_ = 0;
}

}  // namespace qn
