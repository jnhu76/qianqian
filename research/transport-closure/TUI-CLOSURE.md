# TUI — integrated transport closure (design gate)

Status: reviewed design proposal, not canonical authority.
Integrates F6-OPEN-GATE.md, NAVIGATION-GATE.md, VOLUME-GATE.md into the
one reference-player surface. Authority base: D14.2 (episode seam),
D14.4/D14.5/D14.7/D14.8 (stop/seek/pause/position), D10 (UI is not
authority), D12 (feature ≠ Plugin).

---

## 1. Final ownership map (campaign §36)

| Concern | Owner | Truth class | Lifetime |
|---|---|---|---|
| Pause intent | episode seam (`request_pause`/`request_resume`) | Command | one episode |
| Seek target | episode seam (`request_seek`, proposed by frozen D14.5; arrives with F5-IMPLEMENTATION) | Command | one episode |
| Position | episode observation (`position`) | Projection (D14.8) | withdrawn at terminal Fact / activation failure |
| Duration | episode observation (`source_duration`) | Mechanism Evidence (D14.8) | source-scoped; outlives settlement |
| Terminal outcome | episode observation (`terminal_outcome`) | Fact (D11) | one episode, immutable |
| Open request | reference-player App | Command (composition intent) | one Open operation |
| active session handle | reference-player App (holds the handle the episode wiring gave it) | owned reference | until replacement/quit |
| playlist | reference-player App | application navigation state | process |
| current index | reference-player App | application navigation state (commit-on-activation) | process |
| Next/Previous | reference-player App (composes Open) | Command (composition intent) | keypress |
| desired volume | reference-player App | application configuration | process |
| output volume mechanism | output mechanism (per-stream IAudioStreamVolume), routed via session-owned control | mechanism realization of App configuration | one episode's render stream |
| episode playback truth | D14.2 observation seam only | mixed classes, labeled per field | one episode |

Deliberate overlap checks: the App owns no playback truth; the episode
owns no navigation/config state; the mechanism owns no desired-level
truth (it realizes a routed value). The two "current" nouns (navigation
index vs. live episode) are different truth classes in different owners
by design — NAVIGATION-GATE §3 defines their exact coupling point.

## 2. Expected minimal high-level shape (campaign §37), adversarially tested

```text
ReferencePlayerApp
    ├─ playlist: Vec<PathBuf>
    ├─ current_index: Option<usize>
    ├─ desired_volume: VolumeLevel            (0..=100)
    ├─ active: Option<ActiveEpisode>          { runtime: QianqianApp,
    │                                           handle: PlaybackSessionHandle,
    │                                           file: PathBuf }
    └─ transient diagnostic line              (presentation, not state)

Open / Next / Previous   = App composition (F6 replacement)
PlaybackSession          = exactly one playback episode (unchanged)
Output provider          = rendering + player-local volume mechanism
```

Adversarial test result: this shape suffices for every campaign §44
collision (ADVERSARIAL-REVIEW.md §4); therefore:

```text
NO PlaylistPlugin      NO NavigationPlugin
NO VolumePlugin        NO OpenPlugin
NO PlayerState enum    NO app-level transport state machine
```

Startup: positional path arguments populate the playlist and start the
first episode (existing `start_episode` wiring, reused as the
replacement mechanism, F6 §4). The `--machine` transport is untouched:
it remains the one-episode automation contract and gains none of these
keys.

## 3. Command graph (campaign §38) — one obvious path per control

```text
Space  → active.handle PauseResume (fresh-observation rule, existing)
         [no episode ⇒ inert]
← / →  → active.handle request_seek (D14.5 proposed surface)
         [keys reserved until F5-IMPLEMENTATION merges; until then
          unwired presentation noise, never a local stub state]
S      → active.handle request_stop        [no episode ⇒ inert]
O      → App Open(path from the input line)
         → probe → settle/dispose old → fresh episode (F6 §4)
         → on commit: playlist := [path], index := 0
N / P  → App select candidate index → probe → same F6 replacement
         → on commit: current_index := candidate
         [boundary ⇒ inert; probe failure ⇒ refused, nothing moves]
+ / -  → App desired_volume ± 5 (clamped 0..=100)
         → active.handle request_output_level (VOLUME-GATE §6)
         [no episode ⇒ desired level still updates; applied at the
          next episode's stream open]
Q      → App shutdown: stop unsettled episode, wait terminal,
         dispose, exit
```

Every episode command guards on `active`; every composition command
works regardless. Commands are processed sequentially on the App thread;
a replacement is one blocking composition operation (F6 §7), so there
is no concurrent-command state to design.

Fail-stop guard (F6 §6): once any disposal snapshot reports
`quiet == false` (latched teardown violation), the App enters
fail-stop — the status area is replaced by a fatal banner
(`Fatal teardown violation — restart required`), every transport and
composition command becomes inert, and only Q / Ctrl+C remain. This is
App composition control, not a new playback state; recovery would need
its own separately earned authority.

## 4. No second transport state machine (campaign §39)

The App keeps **composition/navigation/configuration state only**:

```text
playlist, current_index, desired_volume, Option<ActiveEpisode>,
one transient diagnostic line
```

None of these is a transport state; none derives or duplicates episode
truth. `enum PlayerState { Opening, Seeking, ... }` is rejected: every
question it would answer is already answered authoritatively by the
D14.2 observation (terminal Fact / pause-intent + engagement evidence /
Position projection) or by App composition state above. The TUI's
existing discipline is kept and extended: no local `paused` bool, no
local position, no cached terminal state.

## 5. Error presentation (campaign §40)

One transient diagnostic line (status area), rendered from the
operation result:

```text
Open failed: <reason>          (probe refusal / composition refusal)
Next: <path> is not readable    (probe refusal; playback unchanged)
volume: <mechanism diagnostic>  (rare; playback unchanged)
```

Diagnostics are presentation: they never become states, never join the
observation, and clear on the next successful operation or tick.
Episode-sourced diagnostics (activation failure, failure_diagnostic)
continue to render through the existing Diagnostics panel exactly as
today. Where a failure leaves old playback untouched (probe refusal,
volume failure), the Track/State panels keep rendering the live
observation — the diagnostic never overwrites playback truth. One case
is not transient: a latched teardown violation is the fatal banner of
the fail-stop guard (§3), never a status line.

## 6. Open input (v1)

`O` opens a one-line path input in the status area (typed or pasted;
Esc cancels); it yields exactly one path (NAVIGATION-GATE §4). No
native file dialog, no directory scan, no fuzzy finder — library
surface is explicitly out of the closure scope.

## 7. v1 reference layout (campaign §41, frozen)

```text
┌──────────────────────────────────────────────┐
│ 千千 Reference Player                       │
├──────────────────────────────────────────────┤
│ Track: 01 - xxx.flac                         │
│ Format: 48 kHz / 2ch                         │
│                                              │
│ 00:42 / 03:58                               │
│ State: Paused              Vol: 70%         │
├──────────────────────────────────────────────┤
│ Playlist                                     │
│ > 01 xxx.flac                               │
│   02 yyy.mp3                                │
│   03 zzz.flac                               │
├──────────────────────────────────────────────┤
│ Space Pause   ←/→ Seek   +/- Volume         │
│ N/P Track     O Open     S Stop     Q Quit  │
└──────────────────────────────────────────────┘
```

Panel sources — one rendering rule for the Track area: when an episode
is active, the Track line names that episode's file and the
Format/timeline/State lines render the D14.2 observation (existing
renderers); when NO episode exists (e.g. a post-replacement activation
failure), Track/State render the no-episode state plus the diagnostic
while the playlist `>` marker stays on the committed index entry
(NAVIGATION-GATE §3). Vol = App desired level; Playlist = App state
with `>` = committed current item (NAVIGATION-GATE §7); status area =
transient diagnostics / Open input line. Explicitly NOT in the closure
scope: lyrics, spectrum, cover, EQ, device picker, favorites, library
search, repeat, shuffle, gapless.

Key map (complete): Space pause/resume · ←/→ seek (post-F5) · S stop ·
O open · N/P next/previous · +/= up · -/_ down · Q quit · Ctrl+C quit.
Everything else stays presentation noise (existing rule).

## 8. Implementation order after design (campaign §42, preserved)

```text
1. F5-IMPLEMENTATION            (separate branch, already authorized
                                 against frozen D14.5; merge first)
2. F6-S-PROBE                   (evidence-only slice: probe during
                                 live playback on a Windows host,
                                 ×3 green runs — no production feature
                                 work; F6 §3)
3. AUTHORITY-PROMOTION          (only on S-PROBE GREEN: promote this
                                 package's accepted decisions into
                                 ADR-PBK-002 / Issue #119 —
                                 DECISION-MATRIX §2; a RED S-PROBE
                                 reopens the F6 mechanism decision
                                 instead of promoting)
4. F6-IMPLEMENTATION            (probe surface + App replacement loop)
5. NAVIGATION-IMPLEMENTATION    (playlist/index on top of F6)
6. V-PROBE                      (volume mechanism physical facts —
                                 VOLUME §10; the apply mechanism does
                                 not fully close before this)
7. VOLUME-IMPLEMENTATION        (seam command + mechanism realization
                                 against the V-PROBE-measured apply
                                 points)
8. TRANSPORT-DOGFOOD            (whole key map, Windows physical pass)
```

This campaign authorizes none of those slices; each needs its own
explicit issue/task against the promoted text. The TUI keys light up
progressively with their gates (←/→ only after 1; N/P after 4; +/-
after 5; O after 3).

## 9. Realtime / firewall conformance (record)

All new per-quantum work on the render path is zero beyond one relaxed
load+compare at the existing loop-top (volume apply-on-change) — the
same cost shape the F5-GATE amendment already accepted for the
seek-park flag. No command, key, playlist, index, or volume value ever
touches the data plane; K0 performs no per-quantum work; the App reads
only the D14.2 observation and kernel snapshots on control boundaries.
