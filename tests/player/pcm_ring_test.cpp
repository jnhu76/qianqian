// pcm_ring_test.cpp — native ring unit + property gates.
//
// Mirrors scenarios.py g_ring_unit / g_ring_property plus a real-thread SPSC
// stress (the oracle is single-threaded; the native ring's lock-free
// contract gets its own evidence). The gate registry lives here and is
// shared with engine_gates_test / thread_stress_test; player_gates' main
// runs everything.
#include <atomic>
#include <cstdio>
#include <cstdlib>
#include <deque>
#include <thread>
#include <vector>

#include "pcm_ring.hpp"
#include "test_support.hpp"

namespace qn::test {

// -- ring helpers: tags are single-float frames (stride 1), like the oracle --
static std::uint64_t rw_write(PcmRing& r, int lo, int n) {
    std::vector<float> v;
    for (int i = 0; i < n; ++i) v.push_back(static_cast<float>(lo + i));
    return r.write(v.data(), static_cast<std::uint64_t>(n));
}

static std::vector<int> rw_read(PcmRing& r, std::uint64_t n) {
    std::vector<float> v(static_cast<std::size_t>(n));
    const std::uint64_t got = r.read(v.data(), n);
    std::vector<int> out;
    for (std::uint64_t i = 0; i < got; ++i) out.push_back(static_cast<int>(v[i]));
    return out;
}

// -- ring gates ----------------------------------------------------------------

GATE(ring_unit) {
    PcmRing r(8, 1);
    QN_CHECK(rw_write(r, 0, 6) == 6, "ring_unit");
    QN_CHECK(rw_read(r, 5) == (std::vector<int>{0, 1, 2, 3, 4}), "ring_unit");
    QN_CHECK(rw_write(r, 6, 7) == 7, "ring_unit");  // spans the boundary
    QN_CHECK(rw_read(r, 8) == (std::vector<int>{5, 6, 7, 8, 9, 10, 11, 12}), "ring_unit");
    QN_CHECK(rw_read(r, 1).empty(), "ring_unit");
    char why[128];
    QN_CHECK(r.check_invariants(why, sizeof why), "ring_unit");

    // exact full: write must never overwrite unread frames
    PcmRing full(4, 1);
    QN_CHECK(rw_write(full, 0, 4) == 4, "ring_unit");
    QN_CHECK(rw_write(full, 9, 1) == 0, "ring_unit");
    QN_CHECK(rw_read(full, 4) == (std::vector<int>{0, 1, 2, 3}), "ring_unit");
    QN_CHECK(rw_read(full, 1).empty(), "ring_unit");

    // one-frame ring
    PcmRing one(1, 1);
    QN_CHECK(rw_write(one, 0, 1) == 1 && rw_write(one, 1, 1) == 0, "ring_unit");
    QN_CHECK(rw_read(one, 1) == (std::vector<int>{0}), "ring_unit");
    QN_CHECK(rw_write(one, 1, 1) == 1, "ring_unit");

    // many wrap cycles on a tiny ring stay FIFO
    PcmRing tiny(3, 1);
    int expect = 0;
    for (int cycle = 0; cycle < 200; ++cycle) {
        std::vector<int> got = rw_read(tiny, 2);
        for (std::size_t i = 0; i < got.size(); ++i) {
            QN_CHECK(got[i] == expect + static_cast<int>(i), "ring_unit");
        }
        expect += static_cast<int>(got.size());
        rw_write(tiny, expect, 2);
    }
    QN_CHECK(tiny.check_invariants(why, sizeof why), "ring_unit");
    std::printf("  exact wrap (w6 r5 w7), full/empty/1-frame, 200 cycles OK\n");
}

GATE(ring_property) {
    // 400 seeds x 150 ops against a straight deque reference.
    for (int seed = 0; seed < 400; ++seed) {
        Rng rng(static_cast<std::uint64_t>(seed));
        const int cap = rng.uniform(1, 16);
        PcmRing r(static_cast<std::uint64_t>(cap), 1);
        std::deque<int> ref;
        int next_tag = 0;
        for (int i = 0; i < 150; ++i) {
            if (rng.unit() < 0.5) {
                const int k = rng.uniform(0, cap);
                std::vector<float> chunk;
                for (int j = 0; j < k; ++j) chunk.push_back(static_cast<float>(next_tag + j));
                next_tag += k;
                const std::uint64_t n = r.write(chunk.data(), static_cast<std::uint64_t>(k));
                for (std::uint64_t j = 0; j < n; ++j) {
                    ref.push_back(static_cast<int>(chunk[static_cast<std::size_t>(j)]));
                }
            } else {
                const int k = rng.uniform(0, cap);
                std::vector<int> got = rw_read(r, static_cast<std::uint64_t>(k));
                QN_CHECK_MSG(static_cast<int>(got.size()) <= k, "ring_property",
                             "seed %d over-read", seed);
                for (int tag : got) {
                    QN_CHECK_MSG(!ref.empty() && tag == ref.front(), "ring_property",
                                 "seed %d frame mismatch", seed);
                    if (!ref.empty()) ref.pop_front();
                }
            }
            if (rng.unit() < 0.05) {
                r.flush();
                ref.clear();
            }
            char why[128];
            QN_CHECK(r.check_invariants(why, sizeof why), "ring_property");
        }
        r.flush();
        char why[128];
        QN_CHECK(r.check_invariants(why, sizeof why), "ring_property");
    }
    std::printf("  400 seeds x 150 ops vs deque reference OK\n");
}

GATE(ring_spsc_threads) {
    // Real-thread SPSC without external locking: producer writes tagged
    // frames, consumer verifies FIFO order and the lifetime accounting.
    const std::uint64_t cap = 977;
    const std::uint64_t total = 200000;
    PcmRing r(cap, 1);
    std::atomic<bool> bad{false};
    std::thread producer([&] {
        std::uint64_t sent = 0;
        std::vector<float> chunk(64);
        while (sent < total) {
            const std::uint64_t want = total - sent < 64 ? total - sent : 64;
            for (std::uint64_t i = 0; i < want; ++i) {
                chunk[i] = static_cast<float>(sent + i);
            }
            sent += r.write(chunk.data(), want);
        }
    });
    std::thread consumer([&] {
        std::uint64_t got = 0;
        std::vector<float> out(64);
        while (got < total) {
            const std::uint64_t want = total - got < 64 ? total - got : 64;
            const std::uint64_t n = r.read(out.data(), want);
            for (std::uint64_t i = 0; i < n; ++i) {
                if (static_cast<std::uint64_t>(out[i]) != got + i) bad = true;
            }
            got += n;
        }
    });
    producer.join();
    consumer.join();
    char why[128];
    QN_CHECK(!bad.load(), "ring_spsc_threads: FIFO violated");
    QN_CHECK(r.check_invariants(why, sizeof why), "ring_spsc_threads");
    QN_CHECK(r.produced_total() == total && r.consumed_total() == total,
             "ring_spsc_threads: totals");
    std::printf("  SPSC 2 threads x %llu frames, FIFO + accounting OK\n",
                static_cast<unsigned long long>(total));
}

}  // namespace qn::test
