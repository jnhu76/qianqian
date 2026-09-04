#include "pcm_ring.hpp"

#include <cstdio>
#include <cstdlib>
#include <cstring>

namespace qn {

PcmRing::PcmRing(std::uint64_t capacity_frames, std::int32_t slot_stride)
    : capacity_(capacity_frames), stride_(slot_stride), data_stride_(slot_stride) {
    // The engine constructor validates config; the standalone ring keeps its
    // own fail-loud guard for direct test use.
    if (capacity_frames == 0 || slot_stride <= 0) {
        std::fprintf(stderr, "PcmRing: capacity/stride must be > 0\n");
        std::abort();
    }
    slots_.resize(static_cast<std::size_t>(capacity_frames * stride_));
}

void PcmRing::set_data_stride(std::int32_t data_stride) {
    if (data_stride <= 0 || data_stride > stride_) {
        std::fprintf(stderr, "PcmRing: data stride %d outside (0, %d]\n",
                     data_stride, stride_);
        std::abort();
    }
    if (head_.load() != tail_.load()) {
        std::fprintf(stderr, "PcmRing: set_data_stride on a non-empty ring\n");
        std::abort();
    }
    data_stride_ = data_stride;
}

std::uint64_t PcmRing::writable() const {
    return capacity_ - buffered_total();
}

std::uint64_t PcmRing::readable() const {
    return buffered_total();
}

std::uint64_t PcmRing::buffered_total() const {
    const std::uint64_t head = head_.load();
    const std::uint64_t tail = tail_.load();
    return head - tail;  // monotonic counters: buffered never negative
}

std::uint64_t PcmRing::write(const float* src, std::uint64_t frames) {
    const std::uint64_t head = head_.load();
    const std::uint64_t tail = tail_.load();  // consumer progress (acquire)
    const std::uint64_t space = capacity_ - (head - tail);
    const std::uint64_t n = frames < space ? frames : space;
    const std::size_t slot_stride = static_cast<std::size_t>(stride_);
    const std::size_t data = static_cast<std::size_t>(data_stride_);
    std::size_t slot = static_cast<std::size_t>(head % capacity_);
    for (std::uint64_t i = 0; i < n; ++i) {
        std::memcpy(&slots_[slot * slot_stride], src + i * data,
                    data * sizeof(float));
        slot = (slot + 1 == capacity_) ? 0 : slot + 1;
    }
    produced_.fetch_add(n);
    head_.store(head + n);  // release: frames visible before the counter moves
    return n;
}

std::uint64_t PcmRing::read(float* dst, std::uint64_t frames) {
    const std::uint64_t tail = tail_.load();
    const std::uint64_t head = head_.load();  // producer progress (acquire)
    const std::uint64_t buffered = head - tail;
    const std::uint64_t n = frames < buffered ? frames : buffered;
    const std::size_t slot_stride = static_cast<std::size_t>(stride_);
    const std::size_t data = static_cast<std::size_t>(data_stride_);
    std::size_t slot = static_cast<std::size_t>(tail % capacity_);
    for (std::uint64_t i = 0; i < n; ++i) {
        std::memcpy(dst + i * data, &slots_[slot * slot_stride],
                    data * sizeof(float));
        slot = (slot + 1 == capacity_) ? 0 : slot + 1;
    }
    consumed_.fetch_add(n);
    tail_.store(tail + n);
    return n;
}

std::uint64_t PcmRing::flush() {
    const std::uint64_t head = head_.load();
    const std::uint64_t tail = tail_.load();
    const std::uint64_t dropped = head - tail;
    // Move the consumer cursor up to the producer cursor: unread frames die,
    // the ring is empty, and the slot space is fully writable again.
    discarded_.fetch_add(dropped);
    tail_.store(head);
    return dropped;
}

bool PcmRing::check_invariants(char* why, std::size_t why_len) const {
    const std::uint64_t head = head_.load();
    const std::uint64_t tail = tail_.load();
    const std::uint64_t buffered = head - tail;
    const std::uint64_t produced = produced_.load();
    const std::uint64_t consumed = consumed_.load();
    const std::uint64_t discarded = discarded_.load();
    if (buffered > capacity_) {
        std::snprintf(why, why_len, "buffered %llu outside [0, %llu]",
                      static_cast<unsigned long long>(buffered),
                      static_cast<unsigned long long>(capacity_));
        return false;
    }
    if (readable() + writable() != capacity_) {
        std::snprintf(why, why_len, "readable + writable != capacity");
        return false;
    }
    if (produced != consumed + buffered + discarded) {
        std::snprintf(why, why_len,
                      "accounting: produced=%llu consumed=%llu buffered=%llu "
                      "discarded=%llu",
                      static_cast<unsigned long long>(produced),
                      static_cast<unsigned long long>(consumed),
                      static_cast<unsigned long long>(buffered),
                      static_cast<unsigned long long>(discarded));
        return false;
    }
    return true;
}

}  // namespace qn
