// wasapi_renderer.hpp — production Windows output (runtime flavor only).
//
// The AudioBackend seam's first real implementation (docs/player-engine.md
// §9/§10): exactly ONE backend-owned, event-driven render thread that calls
// the engine's mutex-free production seam — fill_output() for PCM, then
// advance_render() with padding-proven playout — into a WASAPI shared-mode
// stream on the default render endpoint. This header is platform-neutral
// text on purpose (no windows.h); every Windows type lives in the .cpp,
// which is compiled only by the qianqian_runtime Windows flavor.
//
// Ownership (docs/wasapi-native-runtime-closure.md §4): the renderer is
// created after the engine and destroyed BEFORE it — the destructor
// requests stop, joins the thread, and releases the device, so after
// pe_destroy returns no thread can touch the engine again.
#ifndef QIANQIAN_PLAYER_WASAPI_RENDERER_HPP
#define QIANQIAN_PLAYER_WASAPI_RENDERER_HPP

namespace qn {

class PlayerEngine;

class WasapiRenderer {
public:
    // Starts the render thread. Never throws except on allocation failure;
    // device failures degrade to a silent, bounded-retry renderer (the
    // audible position freezing is the honest "no output" signal).
    explicit WasapiRenderer(PlayerEngine& engine);
    ~WasapiRenderer();  // stop + join + device release; then engine may die

    WasapiRenderer(const WasapiRenderer&) = delete;
    WasapiRenderer& operator=(const WasapiRenderer&) = delete;

private:
    struct State;  // all Windows/thread state, render thread only
    State* state_;
};

}  // namespace qn

#endif  // QIANQIAN_PLAYER_WASAPI_RENDERER_HPP
