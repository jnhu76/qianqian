// test_support.hpp — shared gate registry / rng / check macros for the
// native PlayerEngine test binaries (player_gates).
#ifndef QIANQIAN_TESTS_PLAYER_TEST_SUPPORT_HPP
#define QIANQIAN_TESTS_PLAYER_TEST_SUPPORT_HPP

#include <cstdio>
#include <cstdint>
#include <vector>

namespace qn::test {

// Deterministic xorshift64* rng (gates only need determinism, not Python
// draw-for-draw equality — each gate asserts semantics).
struct Rng {
    std::uint64_t s;
    explicit Rng(std::uint64_t seed)
        : s(seed * 6364136223846793005ULL + 1442695040888963407ULL) {}
    std::uint64_t next() {
        s ^= s >> 12;
        s ^= s << 25;
        s ^= s >> 27;
        return s * 2685821657736338717ULL;
    }
    std::uint64_t below(std::uint64_t n) { return n ? next() % n : 0; }
    int uniform(int lo, int hi) {
        return lo + static_cast<int>(below(static_cast<std::uint64_t>(hi - lo + 1)));
    }
    double unit() {
        return static_cast<double>(next() >> 11) * (1.0 / 9007199254740992.0);
    }
};

struct Gate {
    const char* name;
    void (*fn)();
};
inline std::vector<Gate>& gate_registry() {
    static std::vector<Gate> reg;
    return reg;
}
inline int& gate_failures() {
    static int f = 0;
    return f;
}
struct GateRegister {
    GateRegister(const char* name, void (*fn)()) {
        gate_registry().push_back(Gate{name, fn});
    }
};

inline int run_all_gates() {
    for (const Gate& g : gate_registry()) {
        std::printf("RUN  %s\n", g.name);
        std::fflush(stdout);
        const int before = gate_failures();
        g.fn();
        std::printf(gate_failures() == before ? "PASS %s\n" : "FAIL %s\n", g.name);
        std::fflush(stdout);
    }
    if (gate_failures() != 0) {
        std::printf("RESULT: %d gate failure(s)\n", gate_failures());
        return 1;
    }
    std::printf("RESULT: ALL %zu GATES PASS\n", gate_registry().size());
    return 0;
}

}  // namespace qn::test

#define QN_CHECK(cond, ctx)                                                          \
    do {                                                                             \
        if (!(cond)) {                                                               \
            std::fprintf(stderr, "CHECK-FAIL %s: %s (%s:%d)\n", ctx, #cond, __FILE__, \
                         __LINE__);                                                  \
            ++qn::test::gate_failures();                                             \
            return;                                                                  \
        }                                                                            \
    } while (0)

#define QN_CHECK_MSG(cond, ctx, ...)                                                 \
    do {                                                                             \
        if (!(cond)) {                                                               \
            std::fprintf(stderr, "CHECK-FAIL %s: ", ctx);                            \
            std::fprintf(stderr, __VA_ARGS__);                                       \
            std::fprintf(stderr, " (%s:%d)\n", __FILE__, __LINE__);                  \
            ++qn::test::gate_failures();                                             \
            return;                                                                  \
        }                                                                            \
    } while (0)

#define GATE(name)                                                                   \
    static void gate_fn_##name();                                                    \
    static qn::test::GateRegister gate_reg_##name(#name, gate_fn_##name);            \
    static void gate_fn_##name()

#endif  // QIANQIAN_TESTS_PLAYER_TEST_SUPPORT_HPP
