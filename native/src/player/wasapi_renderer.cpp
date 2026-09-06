// wasapi_renderer.cpp — WASAPI shared-mode render thread (Windows only;
// compiled solely by the qianqian_runtime Windows flavor).
//
// Loop shape (docs/architecture/platform-audio.md):
//
//   idle    probe fill_output(nullptr, 0) — "idle" → bounded sleep, retry.
//           An idle return never touches dst, so nothing is submitted.
//   resume  negotiate the device stream for Float32 / source rate / source
//           channels; the OS audio engine converts to the mix format (the
//           system capability — no resampler code here). Start, then run
//           event-driven. Format changes only happen across control
//           commits, and every commit passes through idle, so the steady
//           path never re-initializes.
//   play    GetCurrentPadding → GetBuffer(available) → fill_output(pData,
//           available) → ReleaseBuffer → padding again →
//           rendered = written − padding → advance_render(delta).
//           Zero copy: the device buffer IS the fill destination. Media
//           position advances only when padding arithmetic proves playout
//           (submitted != rendered, docs §2.4).
//   pause   fill_output "idle" while started → advance_render(0) probe:
//           "paused" → Stop() only (pending device output stays pending and
//           resumes with content on play); otherwise ("idle"/"rendered":
//           not playing) → Stop() + Reset() so submitted-but-unrendered
//           frames die with the engine's timeline reset.
//   commit  the engine's commit boundary (open / play-after-ENDED / stop /
//           seek) runs the commit-flush protocol's CONTROL side
//           (CommitFlushHandshake): it publishes a request and waits for
//           this thread to claim it, run the physical flush (Stop + Reset,
//           accounting rebased, resampler delay drained) HERE on the
//           COM-owning thread, and complete it with a definitive verdict —
//           all BEFORE the engine's epoch/timeline reset and segment N+1
//           land. A request cancelled before the claim can never execute
//           here (the claim CAS refuses it); a claimed request can never be
//           cancelled — the engine waits for this verdict instead of
//           faking a rollback. Ordering, not post-hoc detection, enforces
//           the audible segment invariant: once segment N+1 has committed,
//           the buffer was already proven empty, and the closed admission
//           window means no fill can cross the boundary. A playing commit
//           used to be invisible here (#40): with the buffer topped up, the
//           new segment queued behind stale device PCM and played only
//           after it.
//
// Failure model: any WASAPI/COM failure tears the stream down and leaves
// the renderer silent with a bounded retry (format change or 2 s). No
// hotplug, no endpoint notifications, no exclusive mode, no MMCSS — Phase 1
// scope is exactly "play PCM through the default Windows output".

#include "wasapi_renderer.hpp"

#include "commit_flush_handshake.hpp"
#include "player_engine.hpp"
#include "wasapi_submit_accounting.hpp"

#include <windows.h>
#include <audioclient.h>
#include <mmdeviceapi.h>

extern "C" {
#include <libavutil/channel_layout.h>
#include <libswresample/swresample.h>
}

#include <atomic>
#include <chrono>
#include <condition_variable>
#include <cstring>
#include <mutex>
#include <thread>
#include <vector>

namespace qn {

namespace {

// IIDs defined locally: exports must not depend on which import library a
// given cross-mingw environment happens to carry.
constexpr IID kIID_IMMDeviceEnumerator = {
    0xA95664D2, 0x9614, 0x4F35, {0xA7, 0x46, 0xDE, 0x8D, 0xB6, 0x36, 0x17, 0xE6}};
constexpr CLSID kCLSID_MMDeviceEnumerator = {
    0xBCDE0395, 0xE52F, 0x467C, {0x8E, 0x3D, 0xC4, 0x57, 0x92, 0x91, 0x69, 0x2E}};
constexpr IID kIID_IAudioClient = {
    0x1CB9AD4C, 0xDBFA, 0x4C32, {0xB1, 0x78, 0xC2, 0xF5, 0x68, 0xA7, 0x03, 0xB2}};
constexpr IID kIID_IAudioRenderClient = {
    0xF294ACFC, 0x3146, 0x4483, {0xA7, 0xBF, 0xAD, 0xDC, 0xA7, 0xC2, 0x60, 0xE2}};
constexpr GUID kSubFormatIEEEFloat = {
    0x00000003, 0x0000, 0x0010, {0x80, 0x00, 0x00, 0xaa, 0x00, 0x38, 0x9b, 0x71}};

constexpr std::chrono::milliseconds kIdleSleep{10};     // probe cadence
constexpr std::chrono::milliseconds kEventTimeout{100}; // stop latency bound
constexpr std::chrono::seconds kRetryGate{2};           // device-failure retry gate
// Bound on the REQUESTED phase of a commit flush (cancel before claim is
// the only timeout outcome). Once the renderer CLAIMS a request its
// physical outcome is definitive and awaited without a bound — a faked
// rollback past a claim is the one thing the protocol must never do.
constexpr std::chrono::seconds kFlushAckTimeout{2};

// Minimal local RAII — deliberately NOT a COM framework.
template <typename T>
class ComPtr {
public:
    ComPtr() = default;
    ~ComPtr() { reset(); }
    ComPtr(const ComPtr&) = delete;
    ComPtr& operator=(const ComPtr&) = delete;

    void reset(T* p = nullptr) {
        if (p_ != nullptr) p_->Release();
        p_ = p;
    }
    T** put() {
        reset();
        return &p_;
    }
    T* detach() {
        T* p = p_;
        p_ = nullptr;
        return p;
    }
    T* get() const { return p_; }
    T* operator->() const { return p_; }

private:
    T* p_ = nullptr;
};

}  // namespace

// All state and behavior of the single render thread. The static member
// functions below run on that thread only; there is no cross-thread access
// except the atomic stop flag.
struct WasapiRenderer::State {
    PlayerEngine& engine;

    std::thread thread;
    std::atomic<bool> stop{false};
    // cv + mutex exist only to interrupt the idle sleep instantly on stop;
    // every stop check is the atomic above.
    std::mutex sleep_mtx;
    std::condition_variable wake;

    // Device session (render thread only).
    ComPtr<IAudioClient> client;
    ComPtr<IAudioRenderClient> render;
    HANDLE buffer_event = nullptr;
    std::uint32_t buffer_frames = 0;
    std::int32_t stream_rate = 0;    // negotiated stream format
    std::int32_t stream_channels = 0;
    std::uint64_t written_total = 0; // frames released to the device
    std::uint64_t last_advanced = 0; // rendered total already reported
                                     // (media equivalent when converting)
    std::int32_t attempted_rate = 0; // last failed negotiation request
    std::int32_t attempted_channels = 0;

    // Commit-flush handshake (#40 v3): the protocol state machine lives in
    // CommitFlushHandshake; only its atomics and the sleep cv cross
    // threads. The control thread NEVER touches buffer_event (render-
    // thread-owned): this loop discovers requests at its top, so a playing
    // period notices within one event timeout and an idle/paused sleep
    // within kIdleSleep — both far inside the control side's
    // kFlushAckTimeout. seq_cst inside the handshake makes a completion a
    // real synchronizes-with edge: the engine's segment N+1 lands strictly
    // after the physical Stop+Reset.
    CommitFlushHandshake handshake;

    // Device-side SRC (the frozen aresample/libswresample owner). Null in
    // BYPASS mode, i.e. when the device accepted the source format as is.
    SwrContext* swr = nullptr;
    std::int32_t engine_rate = 0;    // source format being converted from
    std::int32_t engine_channels = 0;
    std::vector<float> in_stage;     // engine PCM staging (frames * engine_channels)
    std::vector<float> out_stage;    // converted PCM staging (frames * stream_channels)
    std::chrono::steady_clock::time_point last_attempt{};

    // -- render thread body ---------------------------------------------------
    static void idle_sleep(State* s);
    static void teardown_stream(State* s);
    static bool open_stream(State* s, std::int32_t rate, std::int32_t channels);
    // Commit-boundary executor: every submitted-but-unrendered frame of the
    // open stream dies — Stop (idempotent) then Reset (requires the stopped
    // state; flushes the device buffer) — render accounting restarts at a
    // fresh origin so no phantom advance can report dropped frames as
    // played, and the resampler's delayed input, still old-segment PCM, is
    // drained and discarded. Returns false on ANY failure: the caller must
    // escalate to teardown_stream — a buffer that cannot be PROVEN flushed
    // must never be claimed clean.
    static bool drop_device_buffer(State* s);
    // Control side of the commit-flush protocol: publish a request, wake
    // the render thread's idle/paused sleep, and wait for THIS request's
    // definitive outcome. Bounded while REQUESTED (timeout -> cancel before
    // the claim — the only safe rollback); once the renderer claims, the
    // outcome is definitive and awaited without a bound.
    static CommitFlushResult commit_flush(State* s);
    // Render side: claim the request, execute the physical flush on THIS
    // thread, publish the verdict. A cancelled request is never claimed.
    static void commit_flush_execute(State* s, std::uint64_t id);
    // Shared idle branch with nothing to submit: distinguishes pause
    // (pending output stays pending and resumes with content on play) from
    // not-playing (Stop + Reset, matching the engine's timeline reset).
    // Returns false: the loop must leave the started state.
    static bool idle_after_probe(State* s);
    // Returns false when the loop must leave the started state: either the
    // session failed (teardown) or the engine went idle (stream stopped;
    // session kept for a content-preserving resume).
    static bool playing_period(State* s);
    static void render_loop(State* s);      // exception barrier + teardown
    static void render_loop_inner(State* s);
};

void WasapiRenderer::State::idle_sleep(State* s) {
    std::unique_lock<std::mutex> lk(s->sleep_mtx);
    s->wake.wait_for(lk, kIdleSleep,
                     [&] { return s->stop.load(std::memory_order_seq_cst); });
}

void WasapiRenderer::State::teardown_stream(State* s) {
    // Stop first so the device cannot pull from a client we are releasing.
    if (s->client.get() != nullptr) s->client->Stop();
    if (s->render.get() != nullptr) s->render.reset();
    if (s->client.get() != nullptr) s->client.reset();
    if (s->buffer_event != nullptr) {
        CloseHandle(s->buffer_event);
        s->buffer_event = nullptr;
    }
    if (s->swr != nullptr) {
        swr_free(&s->swr);
        s->swr = nullptr;
    }
    s->buffer_frames = 0;
    s->stream_rate = 0;
    s->stream_channels = 0;
    s->engine_rate = 0;
    s->engine_channels = 0;
    s->written_total = 0;
    s->last_advanced = 0;
    s->in_stage.clear();
    s->in_stage.shrink_to_fit();
    s->out_stage.clear();
    s->out_stage.shrink_to_fit();
}

// Accept the closest match iff it is float32 — rate/channel count are free
// because the device-side swr converts. The negotiation prefers a direct
// source match first, so conversion only happens when the engine refused
// the source format.
bool negotiate_float32(WAVEFORMATEX* closest) {
    if (closest == nullptr || closest->wBitsPerSample != 32) return false;
    if (closest->wFormatTag == WAVE_FORMAT_IEEE_FLOAT) return true;
    if (closest->wFormatTag == WAVE_FORMAT_EXTENSIBLE) {
        const WAVEFORMATEXTENSIBLE* ext =
            reinterpret_cast<const WAVEFORMATEXTENSIBLE*>(closest);
        return ext->SubFormat == kSubFormatIEEEFloat;
    }
    return false;
}

void build_wfx(WAVEFORMATEXTENSIBLE* wfx, std::int32_t rate,
               std::int32_t channels) {
    std::memset(wfx, 0, sizeof *wfx);
    wfx->Format.wFormatTag = WAVE_FORMAT_EXTENSIBLE;
    wfx->Format.nChannels = static_cast<WORD>(channels);
    wfx->Format.nSamplesPerSec = static_cast<DWORD>(rate);
    wfx->Format.wBitsPerSample = 32;
    wfx->Format.nBlockAlign = static_cast<WORD>(channels * 4);
    wfx->Format.nAvgBytesPerSec =
        static_cast<DWORD>(rate) * static_cast<DWORD>(channels) * 4;
    wfx->Format.cbSize = sizeof(WAVEFORMATEXTENSIBLE) - sizeof(WAVEFORMATEX);
    wfx->Samples.wValidBitsPerSample = 32;
    wfx->dwChannelMask = channels == 1   ? 0x4u /* FC */
                         : channels == 2 ? 0x3u /* FL|FR */
                         : channels < 32 ? (1u << channels) - 1u
                                         : 0xFFFFFFFFu;
    wfx->SubFormat = kSubFormatIEEEFloat;
}

bool WasapiRenderer::State::open_stream(State* s, std::int32_t src_rate,
                                        std::int32_t src_channels) {
    // Bounded retry: a dead endpoint (or unsupported format) must not turn
    // into a 100 Hz negotiation storm. Retry on a format change or after
    // the gate elapses.
    const auto now = std::chrono::steady_clock::now();
    if (s->attempted_rate == src_rate &&
        s->attempted_channels == src_channels &&
        now - s->last_attempt < kRetryGate) {
        return false;
    }
    s->attempted_rate = src_rate;
    s->attempted_channels = src_channels;
    s->last_attempt = now;

    teardown_stream(s);

    ComPtr<IMMDeviceEnumerator> enumerator;
    if (FAILED(CoCreateInstance(kCLSID_MMDeviceEnumerator, nullptr, CLSCTX_ALL,
                                kIID_IMMDeviceEnumerator,
                                reinterpret_cast<void**>(enumerator.put())))) {
        return false;
    }
    ComPtr<IMMDevice> device;
    if (FAILED(enumerator->GetDefaultAudioEndpoint(
            eRender, eMultimedia,
            reinterpret_cast<IMMDevice**>(device.put())))) {
        return false;
    }
    ComPtr<IAudioClient> client;
    if (FAILED(device->Activate(kIID_IAudioClient, CLSCTX_ALL, nullptr,
                                reinterpret_cast<void**>(client.put())))) {
        return false;
    }

    // Tier 1 — the source format itself: the engine accepts it as is and
    // the period loop stays zero-conversion (BYPASS).
    WAVEFORMATEXTENSIBLE src_wfx;
    build_wfx(&src_wfx, src_rate, src_channels);
    WAVEFORMATEX* fmt = &src_wfx.Format;
    HRESULT hr = client->Initialize(AUDCLNT_SHAREMODE_SHARED,
                                    AUDCLNT_STREAMFLAGS_EVENTCALLBACK, 0, 0,
                                    fmt, nullptr);
    std::int32_t accepted_rate = 0;
    std::int32_t accepted_channels = 0;
    if (hr == AUDCLNT_E_UNSUPPORTED_FORMAT) {
        // Tier 2 — closest float32 match + device-side swr SRC (the frozen
        // aresample/libswresample owner; the shared engine does not
        // resample — machine evidence in the phase document).
        WAVEFORMATEX* closest = nullptr;
        const HRESULT hris = client->IsFormatSupported(
            AUDCLNT_SHAREMODE_SHARED, fmt, &closest);
        if (hris == S_FALSE && negotiate_float32(closest)) {
            accepted_rate =
                static_cast<std::int32_t>(closest->nSamplesPerSec);
            accepted_channels = closest->nChannels;
            hr = client->Initialize(AUDCLNT_SHAREMODE_SHARED,
                                    AUDCLNT_STREAMFLAGS_EVENTCALLBACK, 0, 0,
                                    closest, nullptr);
        }
        // Initialize copies the format; the closest-match buffer is never
        // needed again.
        if (closest != nullptr) CoTaskMemFree(closest);
    }
    if (FAILED(hr)) return false;
    if (accepted_rate == 0) {
        accepted_rate = src_rate;
        accepted_channels = src_channels;
    }

    HANDLE ev = CreateEventW(nullptr, FALSE, FALSE, nullptr);  // auto-reset
    if (ev == nullptr) return false;
    if (FAILED(client->SetEventHandle(ev))) {
        CloseHandle(ev);
        return false;
    }
    std::uint32_t frames = 0;
    if (FAILED(client->GetBufferSize(&frames)) || frames == 0) {
        CloseHandle(ev);
        return false;
    }
    ComPtr<IAudioRenderClient> render;
    if (FAILED(client->GetService(
            kIID_IAudioRenderClient,
            reinterpret_cast<void**>(render.put())))) {
        CloseHandle(ev);
        return false;
    }

    s->client.reset(client.detach());
    s->render.reset(render.detach());
    s->buffer_event = ev;
    s->buffer_frames = frames;
    s->stream_rate = accepted_rate;
    s->stream_channels = accepted_channels;
    s->written_total = 0;
    s->last_advanced = 0;

    // SRC setup: only when the accepted device format differs from the
    // source format. Both sides are float32 interleaved, so swr is a pure
    // rate/rematrix stage.
    if (accepted_rate != src_rate || accepted_channels != src_channels) {
        AVChannelLayout out_ch, in_ch;
        av_channel_layout_default(&out_ch, accepted_channels);
        av_channel_layout_default(&in_ch, src_channels);
        s->swr = nullptr;  // callee allocates through the out-parameter and
                           // returns 0 / a negative AVERROR status
        if (swr_alloc_set_opts2(&s->swr, &out_ch, AV_SAMPLE_FMT_FLT,
                                accepted_rate, &in_ch, AV_SAMPLE_FMT_FLT,
                                src_rate, 0, nullptr) < 0 ||
            s->swr == nullptr || swr_init(s->swr) < 0) {
            av_channel_layout_uninit(&out_ch);
            av_channel_layout_uninit(&in_ch);
            teardown_stream(s);
            return false;
        }
        av_channel_layout_uninit(&out_ch);
        av_channel_layout_uninit(&in_ch);
        s->engine_rate = src_rate;
        s->engine_channels = src_channels;
        // Largest pull per period: the output-period need plus resampler
        // delay, expressed in input frames.
        const std::uint64_t in_capacity =
            (static_cast<std::uint64_t>(frames) * src_rate +
             static_cast<std::uint32_t>(accepted_rate) - 1) /
                static_cast<std::uint32_t>(accepted_rate) +
            64;
        s->in_stage.assign(in_capacity * src_channels, 0.0f);
        s->out_stage.assign(static_cast<std::uint64_t>(frames) *
                                accepted_channels,
                            0.0f);
    }
    return true;
}

bool WasapiRenderer::State::drop_device_buffer(State* s) {
    // HRESULT-transparent: Reset only guarantees the flush of pending data
    // when it SUCCEEDS (and requires the stopped state), so a best-effort
    // drop that silently continues would let the engine's commit land with
    // stale PCM still pending. Any failure here is the caller's teardown.
    if (FAILED(s->client->Stop())) return false;
    if (FAILED(s->client->Reset())) return false;
    s->written_total = 0;
    s->last_advanced = 0;
    if (s->swr != nullptr) {
        // The resampler's internal delay still holds input of the dead
        // segment; a commit reset must drop it too, or the next segment
        // starts with sub-millisecond stale PCM (spec §13). Drain into the
        // staging buffer and discard.
        uint8_t* discard_ptr =
            reinterpret_cast<uint8_t*>(s->out_stage.data());
        const int discard_cap = static_cast<int>(
            s->out_stage.size() /
            static_cast<std::uint64_t>(s->stream_channels));
        while (swr_convert(s->swr, &discard_ptr, discard_cap, nullptr, 0) >
               0) {
        }
    }
    return true;
}

CommitFlushResult WasapiRenderer::State::commit_flush(State* s) {
    // Control thread (the engine's commit boundary inside invalidate(),
    // admission closed, seam drained). Publish the request, wake the
    // render thread's idle/paused sleep, and wait for THIS request id to
    // resolve. The id binding is the ABA defense: a stale completion can
    // never satisfy a newer request.
    const std::uint64_t id = s->handshake.request();
    {
        std::lock_guard<std::mutex> lk(s->sleep_mtx);
        s->wake.notify_all();
    }
    const auto resolved = [s, id] {
        return s->handshake.completed(id) || s->handshake.cancelled(id);
    };
    bool done = false;
    {
        std::unique_lock<std::mutex> lk(s->sleep_mtx);
        done = s->wake.wait_for(lk, kFlushAckTimeout, resolved);
    }
    if (!done) {
        // Timeout while REQUESTED: cancel atomically — the only safe
        // "old generation continues" outcome (the operation never began).
        // If the renderer's claim won the race, try_cancel fails: the
        // physical flush may have started, so no rollback is faked — the
        // definitive outcome is awaited however long the device takes.
        if (s->handshake.try_cancel(id)) return CommitFlushResult::kCancelled;
        std::unique_lock<std::mutex> lk(s->sleep_mtx);
        s->wake.wait(lk, resolved);
    }
    return s->handshake.outcome() ? CommitFlushResult::kPerformed
                                  : CommitFlushResult::kFailed;
}

void WasapiRenderer::State::commit_flush_execute(State* s, std::uint64_t id) {
    // Render thread. The claim is the protocol's point of no return: past
    // it, control can no longer cancel and must await this verdict. A
    // flush that cannot be PROVEN escalates to teardown — a released
    // client cannot leave audible PCM behind — and reports kFailed so the
    // engine aborts the commit instead of landing on an unverified buffer.
    if (!s->handshake.claim(id)) return;  // cancelled/superseded: never execute
    bool ok = true;
    if (s->client.get() != nullptr) ok = drop_device_buffer(s);
    if (!ok) teardown_stream(s);
    s->handshake.complete(id, ok);
    {
        std::lock_guard<std::mutex> lk(s->sleep_mtx);
        s->wake.notify_all();
    }
}

bool WasapiRenderer::State::idle_after_probe(State* s) {
    const RenderReport probe = s->engine.advance_render(0);
    if (std::strcmp(probe.kind, "paused") == 0) {
        if (FAILED(s->client->Stop())) {
            teardown_stream(s);
            return false;
        }
        return false;  // session kept; the probe loop re-Starts on resume
    }
    if (!drop_device_buffer(s)) teardown_stream(s);
    return false;
}

bool WasapiRenderer::State::playing_period(State* s) {
    const DWORD waited =
        WaitForSingleObject(s->buffer_event, kEventTimeout.count());
    if (s->stop.load(std::memory_order_seq_cst)) return false;
    if (waited != WAIT_OBJECT_0) return true;  // timeout: nothing free yet

    std::uint32_t padding = 0;
    if (FAILED(s->client->GetCurrentPadding(&padding))) {
        teardown_stream(s);
        return false;
    }
    const std::uint32_t available =
        padding < s->buffer_frames ? s->buffer_frames - padding : 0;
    if (available == 0) return true;

    BYTE* device_bytes = nullptr;
    if (FAILED(s->render->GetBuffer(available, &device_bytes))) {
        teardown_stream(s);
        return false;
    }

    std::uint32_t submitted = 0;
    if (s->swr == nullptr) {
        // BYPASS: the device buffer IS the fill destination — fill_output
        // zeroes GAP spans in place, and an idle return leaves the
        // acquisition to be released empty; untouched dst is never
        // submitted.
        const OutputFillResult r =
            s->engine.fill_output(reinterpret_cast<float*>(device_bytes),
                                  available);
        if (std::strcmp(r.kind, "idle") == 0) {
            // A discarded acquisition must actually be released: a silent
            // failure would make the later Reset fail
            // (AUDCLNT_E_BUFFER_OPERATION_PENDING) and turn a kept session
            // into a torn-down one. Fail closed immediately instead.
            if (FAILED(s->render->ReleaseBuffer(0, 0))) {
                teardown_stream(s);
                return false;
            }
            idle_after_probe(s);
            return false;
        }
        submitted = available;
    } else {
        // Converted: pull engine PCM into the input staging, convert into
        // the device acquisition. The pull size compensates the resampler
        // delay so output keeps pace with device consumption.
        const std::uint64_t delay_out = swr_get_delay(s->swr, s->stream_rate);
        const std::uint64_t need_in =
            (static_cast<std::uint64_t>(available) + delay_out) *
                static_cast<std::uint32_t>(s->engine_rate) /
                static_cast<std::uint32_t>(s->stream_rate) +
            1;
        const std::uint64_t capacity_in = s->in_stage.size() / static_cast<std::uint64_t>(s->engine_channels);
        const std::uint64_t pull = need_in < capacity_in ? need_in : capacity_in;
        const OutputFillResult r = s->engine.fill_output(s->in_stage.data(), pull);
        if (std::strcmp(r.kind, "idle") == 0) {
            if (FAILED(s->render->ReleaseBuffer(0, 0))) {
                teardown_stream(s);
                return false;
            }
            idle_after_probe(s);
            return false;
        }
        const uint8_t* in_ptr = reinterpret_cast<const uint8_t*>(s->in_stage.data());
        uint8_t* out_ptr = reinterpret_cast<uint8_t*>(s->out_stage.data());
        const int converted = swr_convert(
            s->swr, &out_ptr, available, &in_ptr,
            static_cast<int>(pull < INT32_MAX ? pull : INT32_MAX));
        if (converted < 0) {
            s->render->ReleaseBuffer(0, 0);
            teardown_stream(s);
            return false;
        }
        if (converted > 0) {
            std::memcpy(device_bytes, s->out_stage.data(),
                        static_cast<std::uint64_t>(converted) *
                            static_cast<std::uint64_t>(s->stream_channels) * 4);
            submitted = static_cast<std::uint32_t>(converted);
        }
        // converted may legally be 0 (input absorbed into the resampler's
        // delay): release the acquisition empty and let the next period's
        // larger `available` pull more input.
    }
    // Submission is the period's linearization: with the commit-flush
    // protocol, a commit cannot complete until this thread completes the
    // flush request, and that happens after THIS ReleaseBuffer —
    // old-segment PCM can never cross a landed commit. I6: accounting
    // moves only after the physical release SUCCEEDS — a failed release
    // means the device did not accept the frames, so written_total must
    // not advance, no advance_render evidence may be fabricated, and the
    // session is torn down.
    const HRESULT released = s->render->ReleaseBuffer(submitted, 0);
    const SubmitAccounting accounting =
        apply_release_result(SUCCEEDED(released), submitted, s->written_total);
    if (accounting.teardown) {
        teardown_stream(s);
        return false;
    }
    s->written_total = accounting.written_total;

    // Proven playout: frames no longer pending in the device buffer. In
    // converted mode the media equivalent uses the exact integer ratio;
    // the engine's pending clamp bounds any trailing-silence rounding to
    // under one period.
    std::uint32_t padding_now = 0;
    if (FAILED(s->client->GetCurrentPadding(&padding_now))) {
        teardown_stream(s);
        return false;
    }
    const std::uint64_t rendered =
        s->written_total > padding_now ? s->written_total - padding_now : 0;
    const std::uint64_t media_rendered =
        s->swr != nullptr
            ? rendered * static_cast<std::uint32_t>(s->engine_rate) /
                    static_cast<std::uint32_t>(s->stream_rate)
            : rendered;
    if (media_rendered > s->last_advanced) {
        s->engine.advance_render(
            static_cast<std::int64_t>(media_rendered - s->last_advanced));
        s->last_advanced = media_rendered;
    }
    return true;
}

void WasapiRenderer::State::render_loop(State* s) {
    // A throw escaping a thread function terminates the process; the C ABI
    // contract forbids that. Staging allocations are the only realistic
    // source and the renderer degrades to silent without them.
    try {
        render_loop_inner(s);
    } catch (...) {
        try {
            teardown_stream(s);
        } catch (...) {
        }
        s->stop.store(true, std::memory_order_seq_cst);
    }
}

void WasapiRenderer::State::render_loop_inner(State* s) {
    // COM on THIS thread: initialized and uninitialized by the same thread
    // that owns every COM interface below.
    const HRESULT coinit = CoInitializeEx(nullptr, COINIT_MULTITHREADED);
    const bool com_owner = SUCCEEDED(coinit);
    bool started = false;
    while (!s->stop.load(std::memory_order_seq_cst)) {
        // Commit boundary (#40): the engine's commit hook waits for this
        // thread's verdict on the pending request, so a commit can never
        // land while stale PCM is pending and the renderer never has to
        // detect one after the fact. Leaving the started state routes the
        // next iterations through the probe: after the commit lands, the
        // probe re-negotiates the format (commits may change it) and
        // re-Starts on the clean buffer.
        if (const std::uint64_t flush_id = s->handshake.pending_request()) {
            commit_flush_execute(s, flush_id);
            started = false;
            continue;
        }
        if (started) {
            started = playing_period(s);
            continue;
        }
        // Idle probe: admission-safe, zero-frame, never touches dst.
        const OutputFillResult probe = s->engine.fill_output(nullptr, 0);
        if (std::strcmp(probe.kind, "idle") == 0) {
            idle_sleep(s);
            continue;
        }
        // Activation requested with a concrete source format.
        if (s->client.get() == nullptr ||
            s->stream_rate != probe.source_rate ||
            s->stream_channels != probe.channels) {
            if (!open_stream(s, probe.source_rate, probe.channels)) {
                idle_sleep(s);  // silent, bounded retry (gate inside)
                continue;
            }
        }
        if (FAILED(s->client->Start())) {
            teardown_stream(s);
            idle_sleep(s);
            continue;
        }
        started = true;
    }
    teardown_stream(s);
    if (com_owner) CoUninitialize();
}

WasapiRenderer::WasapiRenderer(PlayerEngine& engine)
    : state_(new State{engine}) {
    // The engine's commit boundary calls commit_flush on its control
    // thread. Registered before the thread starts and unregistered before
    // it is joined, so the hook can never observe a torn-down State.
    engine.set_commit_flush_hook([s = state_] { return State::commit_flush(s); });
    try {
        state_->thread = std::thread([s = state_] { State::render_loop(s); });
    } catch (...) {
        engine.set_commit_flush_hook(nullptr);
        delete state_;
        state_ = nullptr;
        throw;  // pe_create's boundary catches and degrades to a silent
                // runtime
    }
}

WasapiRenderer::~WasapiRenderer() {
    // Lifecycle proof (v3 review): control calls are serialized by the
    // caller (player_engine.h threading contract) and the runtime's
    // pe_destroy runs on that same control thread, so no commit-flush hook
    // invocation can be in flight here. Unregistration removes the last
    // cross-thread entry into this object BEFORE any later control op
    // could copy the hook; the render thread is then the only remaining
    // user, and the engine outlives it (the render thread dereferences it
    // up to the join). The handshake state lives and dies with this
    // State, so no request id can outlive the renderer and be completed
    // by a future instance.
    state_->engine.set_commit_flush_hook(nullptr);
    state_->stop.store(true, std::memory_order_seq_cst);
    state_->wake.notify_all();
    if (state_->thread.joinable()) state_->thread.join();
    delete state_;
}

}  // namespace qn
