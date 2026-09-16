# experiments/f3-pause-mechanism — F3-GATE mechanism evidence

Executable mechanism evidence for the Phase-F **F3 Pause/Resume gate**
(Issue #119; ADR-PBK-002 §20 D14.7). This crate is **not production
architecture**: it is workspace-excluded, imports no `qianqian-*` crate,
and nothing here ships in the product path (D14.7: the mechanism may be
prototyped/audited outside the product path; the product contract is
frozen by the ADR, implemented by a later F3 slice).

## What is here

```text
src/edge.rs      bounded ring edge — faithful copy of the production
                 PcmEdge synchronization semantics (crate-private there)
src/gate.rs      the session-owned pause gate (mechanism A control object):
                 pause_requested = command state, engaged = mechanism
                 acknowledgment, stopped = unpark-and-continue stop wake
src/sim.rs       simulated device: buffer accounting + consumption
                 independent of the render loop + device Stop/Start (B)
                 + a test-only drain-hold for deterministic sampling
src/worker.rs    mock decode worker in the production decode_worker shape
src/render.rs    the production steady_loop order with both mechanisms:
                 A = park at loop top, strictly before GetBuffer
                 B = the same park wrapped in device Stop/Start
                 plus the deliberately-broken negative-control loop
src/establishment.rs  the corrected D14.7 Paused/Resumed projection shape
                      (CORRECTIVE-1): command state + engagement + output-
                      tail quiescence; plus the pre-corrective engagement-
                      only conjunction kept as the negative-control mutant
tests/scenarios.rs  synchronization-shape oracle suite (all platforms)
src/bin/f3probe.rs  physical WASAPI probe (Windows only)
RESULTS.md       the evidence record: measurements, decision table,
                 authority proposal status
```

## What each mechanism is

```text
A  explicit render-loop pause gate located BEFORE WASAPI GetBuffer.
   The render leg parks at the loop top; no device buffer is ever held
   across a parked pause; already-submitted audio plays out (padding
   drains to zero) and the parked leg then submits nothing (device
   renders silence — platform semantics).

B  the same gate wrapped in IAudioClient::Stop / Start. The device
   freezes: already-submitted audio stays queued (padding frozen) and
   continues on resume.

Rejected outright (campaign rule): letting read_frames block while a
   GetBuffer is held — i.e. "park inside the edge read". That is the
   current empty-edge blocking shape and must NOT become a pause
   mechanism; the negative-control loop + oracle exist to prove the
   harness catches that shape.
```

Both mechanisms are **unpark-and-continue** on stop: the gate's stop
wake never aborts the render leg — the loop proceeds once more and the
data-plane terminal (`PcmPull::Stopped` mid-play, natural EOF+drain
post-EOF) decides. This is what keeps every D11 decision-table history
classification unchanged (see RESULTS.md §resolver-consistency).

## Running

```bash
# synchronization-shape scenarios (any platform)
cargo test --manifest-path experiments/f3-pause-mechanism/Cargo.toml

# physical probe (Windows; plays ~12 s of quiet 440 Hz tone)
cargo run --manifest-path experiments/f3-pause-mechanism/Cargo.toml \
    --bin f3probe --target x86_64-pc-windows-gnu
```

Whether the pause *sounds* right is deliberately not claimed by any
executable here — that is the human reviewer's acceptance on a real
device.
