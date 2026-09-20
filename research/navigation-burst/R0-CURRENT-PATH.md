# R0 — CURRENT PATH

Campaign: QIANQIAN-NAVIGATION-BURST-ROOT-CAUSE-AND-BOUNDARY-0
Base: `ad4e09dc9064077b97cc6f26dd9139d81dae6e99` (origin/main, PR #169 merge)
Branch: `research/navigation-burst-boundary-0`

This report describes the CURRENT mechanism, source-linked. No
recommendations here.

## 1. The N/P call graph (source-linked, exact functions)

```text
console/ConPTY input event (OS-side queue, see Q1)
    ↓ crossterm event::poll/read
apps/headless/src/tui/runtime.rs:73-107   run() event loop
    ↓ Event::Key
runtime.rs:139  handle_key()
    ↓ (normal mode; input-mode precedence model.rs:651-652)
    ↓ Action::Next / Action::Previous
runtime.rs:249-256
    ↓
runtime.rs:321  perform_navigation(model, player, Navigation::…)
    ↓
player.rs:418 / 426  ReferencePlayerApp::next_track() / previous_track()
    ↓
player.rs:433  navigate(forward)
    ├─ playlist.rs:433  TemporaryPlaylist::manual_step(forward)   [pure; None = inert]
    ├─ playlist.rs:456  path_at(position)
    ↓
player.rs:547  replace_episode(candidate)      ← THE WHOLE D14.6 SEQUENCE
    ├─ player.rs:558  start.probe(candidate)    = decode-songcore SourceFacts query
    │                 (entry.rs:345-355 → qianqian_decode_songcore::probe_media)
    ├─ player.rs:563  retire_old_episode(old)
    │     ├─ handle.rs:245  request_stop()      → completion.rs:435 (edge.stop())
    │     ├─ handle.rs:330  wait_terminal()     → completion.rs:1044 (condvar wait)
    │     └─ runtime.dispose()                  → K0 teardown; require Discharged
    ├─ player.rs:572  start.start(candidate, desired_volume)
    │     = entry.rs:356: FRESH QianqianApp + 3 registrations + revise_desired
    │       → session.rs:72-180 activation:
    │          resolve decode+output → decode open_media → PcmEdge(8192 frames)
    │          → output open_stream (DEVICE OPEN) → spawn decode worker
    └─ commit evidence: observe().source_format.is_some() ∧ no activation_error
    ↓
player.rs:438  playlist.commit_navigation(position)  (commit-on-activation)
    ↓
runtime.rs:346  model.set_status(feedback)   ("next: opened …")
```

Natural EOF auto-next shares everything from `replace_episode` down:
`runtime.rs:98 poll_eof_policy()` → `player.rs:521` →
`replace_episode` + `playlist.commit_eof`. It fires ONLY on a committed
D11 `Completed` Fact (player.rs:525-535, Issue #166 §13/§16).

`Enter` on the selected row (player.rs:447 play_selected) reaches the
same `replace_episode`; the live-row replay rule (player.rs:467)
refuses it while the selected row IS the unsettled episode.

## 2. Thread ownership and blocking points

| Thread | Owner | Work | Can block on |
|---|---|---|---|
| main (TUI shell) | `runtime::run` | draw, poll/read keys, **the whole replacement sequence synchronously**, EOF policy poll | `wait_terminal` (condvar), `dispose` (thread joins), `start` (device open), terminal I/O |
| `qianqian-decode` (per episode) | session activation (session.rs:159) | decode staging blocks → bounded edge writes; seek protocol | `wait_for_space` (2 ms slices), provider reads |
| render thread (per episode) | Output backend (wasapi.rs run_render_thread / device event) | loop-top gate, padding read, GetBuffer/read_frames/ReleaseBuffer, level apply, position publication | device event (≤100 ms EVENT_TIMEOUT_MS, wasapi.rs:91), edge `data_ready` |

The replacement is **App-thread-serialized** (D14.6: "Repeated Open is
App-thread-serialized — one complete replace operation at a time";
runtime.rs module doc states the synchronous stall deliberately).

## 3. Blocking-cost structure of one replacement (mechanism reading)

Old side (`retire_old_episode`):
- mid-play stop: `request_stop` → `edge.stop()` → render leg's
  `read_frames` returns `PcmPull::Stopped` (edge.rs:225-237 — buffered
  frames are ABANDONED) → leg exits Aborted → drain verdict →
  `resolve` commits `Stopped` under one lock hold
  (completion.rs:1136 publish_evidence). No drain wait for a manual
  stop. The bound is the render leg's CURRENT device wait
  (≤ EVENT_TIMEOUT_MS; measured 33 ms on the real host, campaign R2).
- post-EOF old episode: stop/wait skipped, dispose directly.

New side (`start`): fresh K0 root, decode `open_media` (SongCore/FFmpeg
open), edge allocation, **device open + format negotiation** (Tier-1
direct / Tier-2 engine SRC fallback, PBK-003 §8), worker spawn. The
device is opened EVERY episode by design (D14.6: "the per-episode
device open every candidate pays anyway").

## 4. Queue / buffer inventory (Q1–Q7)

| # | Queue / buffer | Owner | Bounded? | Contents / behavior |
|---|---|---|---|---|
| Q1 | Terminal/console input queue | OS (Windows console / ConPTY input buffer), consumed by crossterm `event::poll/read` — ONE event per loop iteration (runtime.rs:78-88) | OS-bounded (effectively unbounded for bursts) | Raw key events. While the shell thread runs a replacement, N/P events ACCUMULATE here; each is later replayed as a full navigation |
| Q2 | Future / pending navigation state | **DOES NOT EXIST** (source-verified: no pending target, no coalescing, no debounce anywhere in the path above) | — | — |
| Q3 | Decoder staging | decode worker (session.rs:45 STAGING_FRAMES=1024, one preallocated buffer; provider-internal buffers beyond) | bounded | Decoded-but-unpublished PCM (one staging block) |
| Q4 | PcmEdge | session-owned (session.rs:112; edge.rs) | 8192 frames ≈ 185 ms @44.1 kHz stereo | Producer blocks in `wait_for_space` (2 ms slices); consumer `read_frames` blocks when empty; `stop()` → TERMINAL_STOPPED (buffered frames abandoned); `invalidate()` (seek only); `close_eof()` |
| Q5 | Output software buffering | none beyond the edge — the WASAPI device buffer IS the fill destination (wasapi.rs "zero-copy period", GetBuffer dst written directly from read_frames) | n/a | — |
| Q6 | WASAPI buffer / padding | device session (wasapi.rs; observed `buffer 970 frames` ≈ 22 ms shared event-driven) | buffer_frames | Handed-off-but-not-consumed frames (`GetCurrentPadding`). On manual stop the leg exits and the client is released (padded audio dropped); on natural EOF `drain_to_zero` (wasapi.rs:751) waits until padding reaches 0 — EOF must be AUDIBLE |
| Q7 | Hardware/device tail | Windows audio engine / driver | NOT OBSERVABLE from current seams | — |

## 5. Current truth/authority boundaries touched by this path

- `replace_episode` is an application composition Command; its
  success/failure is operation feedback, never a playback semantic
  (D14.6). Commit = old-side clear ∧ authoritative `Activated` read
  from the episode seam (player.rs:578-581), never a snapshot.
- The old episode keeps its own D11 terminal (`Stopped` for a manual
  mid-play stop; `Completed` only after the natural drain).
- The playlist and both cursors are App navigation state (D14.6 §14
  closure + U2 amendment); commit-on-activation; no PlaylistFact /
  CurrentTrackFact exists; `▶` committed vs `>` selected are
  presentation markers.
- Pending navigation (if ever introduced) would be interaction/UI-App
  control state — NOT a Fact, NOT episode identity (campaign §25).
- The PCM data plane never re-enters K0 per block; the whole-root
  replacement guarantees no old-PCM-after-new-commit (one edge, one
  stream, one episode; old root fully discharged before the new root
  is constructed — D14.6 no-overlap).
