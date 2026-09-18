# VOLUME — player-local volume (design gate)

Status: reviewed design proposal, not canonical authority.
Authority base: D14.9 (volume authority/mechanism NOT frozen; forbidden
list), D14.10 stop-list, D13, D14.7 (control-routing precedent), PBK-001
§7 (parameter update ≠ topology update), §2.4 (realtime firewall).
Mechanism research: official Microsoft Learn WASAPI documentation
(citations inline; a mandated physical confirmation probe is defined in
§10).

---

## 1. Root question

`+ / -` changes **this player's playback level** — not the Windows
system/master volume, not another application's audio. The gate must
select the smallest per-player/per-stream mechanism.

## 2. Truth class

The product object is **desired volume** — application configuration:

```text
desired_volume ∈ 0..=100 (integer percent, clamped)
truth class:    application configuration / Command state (the
                user's standing request), routed to the mechanism the
                same way pause intent is routed (Command family, D14.7
                precedent)
NOT:            a Fact, a semantic state, mechanism evidence about
                actual loudness, or a fourth transport anything
```

A user setting `Volume = 70` means exactly: "the player requests its
output mechanism to use this level". No acoustic-loudness claim is
made. Microsoft's own documentation records that perceived loudness is
not linear in amplitude (v=0.5 ≈ −6 dB attenuation) and that the
effective level is the product of four volume factors (stream ×
channel-session × session-master × policy) — the player controls at
most one factor and claims nothing about the product.

## 3. Ownership

```text
App owns desired_volume persistently across tracks/episodes
    ↓ command routing (episode seam, §6)
current episode's output mechanism realizes it per-stream
```

Consequence (frozen): **Open / Next / Previous preserve the user's
desired volume** — it lives above episodes, so episode replacement
cannot lose it; each new episode's mechanism receives the App's current
desired level at construction (§8). This is the cross-check the
campaign asks for: if volume vanished every episode, its owner would be
wrong.

## 4. Range / representation — integer 0..=100 selected

```text
candidate            verdict
0.0..=1.0 f32        rejected at product level: NaN/saturation/
                     equality hazards buy nothing; the mechanism
                     converts once at its boundary (level/100.0)
decibels             rejected: no dB UI, no perceptual-curve promise
0..=100 integer      SELECTED: total ordering, exact equality, trivial
                     clamp, UI-native, conversion happens exactly once
                     inside the mechanism (a plain division; no NaN
                     representable)
```

`50` makes NO "half perceived loudness" claim (§2). Representation name
(open): `VolumeLevel(u8)` in 0..=100.

## 5. Mechanism candidates (Windows / WASAPI shared-mode path)

Current reality: the output mechanism is an event-driven **shared-mode
WASAPI** render loop; it contains no volume code today; the `windows`
crate 0.62 `Win32_Media_Audio` feature already exposes the candidate
interfaces (no new feature flags).

### A. Software PCM gain (`sample *= gain`)

```text
pros: portable; stream-local; simple semantics
cons: permanent per-sample RT cost on the hot path; clipping/gain
      policy becomes player code; future DSP coupling starts here
verdict: REJECTED on the Windows path — a stream-local control-plane
      mechanism exists (B2). Recorded as the portable FALLBACK for
      future platforms whose output mechanism offers no stream-local
      control; not built now (do not optimize prematurely — and do not
      pay a permanent RT tax when the platform already provides the
      control).
```

### B1. ISimpleAudioVolume (session master volume)

Official documentation facts: it controls the master volume of an
**audio session**; "WASAPI applies the volume setting for a session
uniformly to **all of the streams in the session**"; a client
implicitly creates a session by assigning the first stream to it;
SndVol's per-application sliders **reflect only ISimpleAudioVolume**
changes; those settings are **persistent across computer restarts**.

```text
verdict: REJECTED, with evidence-backed reasons:
  - NOT stream-local: both qianqian instances in one process share the
    default session ⇒ +/- on one player changes the other (campaign
    §47-B fails).
  - Mixer-coupled: SndVol displays and can change it ⇒ the TUI value
    can be externally falsified (§47-I) and the player would fight the
    user's mixer.
  - Persistent across restarts ⇒ a player would silently rewrite the
    user's saved per-app mixer level (global side effect — §28 fails).
  - Isolating one player would require per-stream session-GUID
    machinery — the complex session machinery §28 says to avoid.
```

### B2. IAudioStreamVolume (per-stream channel volume) — **SELECTED**

Official documentation facts (Microsoft Learn, audioclient.h /
Session Volume Controls):

```text
- "controls the volume of an individual stream in a session RELATIVE
   TO THE OTHER STREAMS in the session" — stream-local by contract.
- obtained via IAudioClient::GetService(IID_IAudioStreamVolume) on the
   stream's own client — no session GUID required anywhere.
- SetAllVolumes(n, levels): one call sets ALL channels uniformly;
   levels 0.0..=1.0; out-of-range ⇒ E_INVALIDARG (fail-closed).
- shared-mode only (this player is shared-mode; exclusive mode is out
   of scope for the reference player).
- SndVol does NOT reflect it (the mixer shows only ISimpleAudioVolume)
   ⇒ no mixer coupling; external applications cannot reach another
   process's stream volume ⇒ §47-A/I pass by contract.
- it is one multiplicative factor of the engine's per-stream volume;
   the engine applies it to this stream's samples — no PCM passes
   through player code ⇒ no per-sample player-side RT cost.
- control-plane mutation: an ordinary client call, not a per-quantum
   operation (§7 of PBK-001: a cheap realtime-safe parameter update
   must not be forced through heavyweight composition).
```

Documentation-vs-reality caveat, honest: all behavioral claims above
are **contract/documentation** claims. The F5 precedent (E3 physical
probe before freezing mechanism behavior) is applied as **V-PROBE**
(§10), mandated at the VOLUME-IMPLEMENTATION gate before the mechanism
choice is treated as physically confirmed on the exercised endpoint.

### C. System endpoint master volume (IAudioEndpointVolume)

```text
verdict: REJECTED outright — changes the endpoint master level that all
applications share; Microsoft's guidance for shared-mode streams is to
use the session/stream interfaces instead. Fails campaign §23/§47-A.
```

### Selection scorecard (campaign §28 criteria)

```text
criterion                        A gain   B1 simple  B2 stream   C endpoint
player-local effect              yes      no         yes         no
no per-sample permanent tax      no       yes        yes         yes
control-plane mutation           n/a      yes        yes         yes
portable abstraction boundary    yes      yes        yes*        yes
no new lifecycle                 yes      yes        yes         yes
no global side effects           yes      no         yes         NO
no complex session machinery     yes      no         yes         n/a
* portability is preserved by putting the abstraction at the
  mechanism seam (§6) and recording software gain as the documented
  fallback for platforms without a stream-local control.
```

## 6. Routing / command shape (no new nouns)

Follows the D14.7 pattern exactly (session-owned control handed to the
render mechanism in `RenderRequest`, like the pause gate and the
position cell):

```text
App desired_volume
    ↓ episode seam: request_output_level(VolumeLevel)   (Command;
      idempotent, same family as pause/resume intent)
    ↓ session-owned output-level control (an owned episode resource —
      NOT a Capability, NOT a Plugin, NOT a Fact)
    ↓ carried in RenderRequest (representation open) to the render
      mechanism
    ↓ mechanism applies via IAudioStreamVolume::SetAllVolumes
      (all channels, level/100.0)
```

Rejected names/shapes: `VolumeManager`, `MixerPlugin`, `AudioPolicy`,
`DSPGraph`, a `set_volume` method on the output Capability used by the
App directly (the App holds no capability), and applying gain in the
decode leg (wrong owner: volume is an output-rendering concern).
Apply-point rule (mechanism-side, frozen shape):

```text
- applied once at stream open (before first submission), so a fresh
  episode never plays at full level before the first + press;
- re-applied at the render loop top when the routed value changed:
  one relaxed load + compare per iteration (the same cost class as the
  F5-approved loop-top seek-park flag test), and on change ONE
  SetAllVolumes performed on the leg's own thread, which also keeps
  the COM interface pointer on the thread that created it
  (GetService/Release thread discipline);
- never inside the quantum between GetBuffer and ReleaseBuffer.

What is deliberately NOT claimed: the documentation contracts
SetAllVolumes' stream-local scope and its 0.0–1.0 domain — it does NOT
promise non-blocking or bounded-latency behavior, and the
thread-discipline note above is about interface lifetime, not an
RT-safety proof. No non-blocking / bounded-latency guarantee is claimed
anywhere in this design. V-PROBE V4/V5 (§10) measures the render leg's
actual perturbation; a materially disturbing result reopens the
apply-point/ownership decision before VOLUME-IMPLEMENTATION freezes
it.
```

## 7. Volume change while playing / paused (non-events, frozen)

```text
volume change is:  non-terminal · same episode · no PCM topology cut
it NEVER:          flushes the edge, parks the render leg, resets
                   Position, or creates a discontinuity — it is not
                   Seek
while playing:     takes effect at the next loop-top apply point
while paused:      desired level changes immediately (App state);
                   the mechanism applies it no later than the first
                   post-release loop iteration — "before/when playback
                   resumes", exactly the campaign §30 contract; pause
                   state unchanged, no unpause
```

A failed `SetAllVolumes` (e.g. `AUDCLNT_E_DEVICE_INVALIDATED`) is a
mechanism diagnostic on the existing device-failure paths; the volume
command itself never settles D11 and never fails an episode.

## 8. Persistence across Open / Next / Previous (frozen)

App-held `desired_volume` survives every episode replacement because
replacement rebuilds the episode, not the App (F6 §4): each fresh
episode's output-level control is initialized from the App's current
desired level, and the mechanism applies it at stream open. The
WASAPI-side stream volume intentionally resets to 1.0 with each new
stream — irrelevant, because the App re-applies before first
submission (§6). No persistence to disk in v1 (no config file exists;
out of scope).

## 9. Zero / mute / observation / TUI

```text
Volume = 0   sufficient for v1 (stream-level silence; other audio
             unaffected — §5-B2). NO separate Mute state (no restore
             semantics are needed yet; Mute can be earned later).
TUI keys     + / =  up;  - / _  down; fixed step 5; clamp 0..=100;
             no acceleration, no dB display, no mute key.
TUI value    "Vol 70%" renders the App's DESIRED level only. The
             mechanism-observed actual level is deliberately NOT read
             back (GetAllVolumes stays unexposed): no false hardware
             fact, and §47-I is structurally impossible — the displayed
             number is the request, not a measurement.
```

Microsoft's UI-confusion caution (app sliders for stream volume may be
mistaken for SndVol sliders) is acknowledged: the label says "Vol" in a
player shell that owns no system-volume pretensions; accepted for the
v1 reference player.

## 10. V-PROBE — mandated physical confirmation (implementation gate)

Before VOLUME-IMPLEMENTATION treats §5-B2 as physically confirmed, one
narrow Windows-host probe (E3 precedent) must demonstrate, ×3 green
runs, on the exercised endpoint:

```text
V1a SetAllVolumes on stream A does not alter a simultaneously
    rendering stream B in the SAME process / same default audio
    session — the DISCRIMINATING experiment against
    ISimpleAudioVolume, which fails exactly here (session volume
    applies to ALL streams in the session)
V1b a simultaneously rendering stream of ANOTHER process is unchanged
    (secondary confirmation; cannot substitute for V1a)
V2  SndVol/mixer interaction: mixer slider moves do not change the
    stream's applied level (no coupling)
V3  a replaced stream starts at engine level 1.0 and the re-applied
    desired level is audible-immediately at open (persistence rule)
V4  change application: repeated +/- presses, and changes while
    paused, produce no render-loop perturbation — measured render
    iteration latency and underrun/glitch behavior; no stream
    discontinuity (no click/pop beyond the gain step), no Position
    reset
V5  failure path: SetAllVolumes under device invalidation / service
    failure degrades to the mechanism diagnostic without wedging the
    render leg

If V1a shows cross-stream coupling within one audio session, the
stream-local isolation claim fails and the §5 mechanism decision
REOPENS. If V4/V5 show the loop-top apply materially perturbs the
render leg, the apply-point/ownership mechanism MUST be reconsidered
before VOLUME-IMPLEMENTATION freezes it. The mechanism candidate
(IAudioStreamVolume) stays selected on documentation evidence; its
physical RT placement does not close before V-PROBE.
```

## 11. D13 / D14.9 admission check (record)

```text
candidate            verdict
VolumePlugin         NO  (D14.9 forbids by name; D13: existing owners suffice)
MixerPlugin/AudioPolicy/DSPGraph   NO (forbidden shapes)
global volume store  NO  (D14.9: no global mutable control store)
per-quantum dispatch NO  (D14.9: forbidden; loop-top load+compare only)
generic control bus  NO  (D14.9: forbidden)
episode seam command + session-owned control + mechanism call  = the
                     selected smallest expression of D14.9's
                     "session-owned control / output-provider control"
                     question: the SESSION owns the control state and
                     routes it; the OUTPUT PROVIDER owns the mechanism
                     that realizes it.
```
