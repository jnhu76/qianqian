# ADR-PBK-003 — Stable Output Plugin / Pluggable Host Render Backend Boundary

| Field | Value |
|---|---|
| Status | **ACCEPTED** |
| Date | 2026-09-19 |
| Decision | Separate the stable K0 `Output Plugin` composition identity from concrete host-audio backend mechanisms |
| Amends | ADR-PBK-002 D5/D13 and the platform interpretation of D14.5/D14.7/D14.8/D14.9 |
| Does not change | ADR-PBK-001 foundations; D11 terminal authority; F3 pause semantics; F4 Position/Duration truth classes; F5 seek semantics; F6 Open/Navigation semantics; D14.9 desired-volume ownership |
| Decision basis | Current Windows production reality + the first real second-platform pressure (Linux) exposed a backend-identity leak that the existing D13 razor already predicts |

---

## 1. Problem / differential

Qianqian deliberately split playback into stable composition roles:

```text
Decode Plugin
Output Plugin
Playback Session Plugin
```

Playback Session consumes the typed `AudioOutputCapability`; it does not directly depend on WASAPI. That boundary is correct.

However, the current Windows realization still fuses two different identities:

```text
stable composition role          concrete platform mechanism
        Output Plugin        ==      WASAPI backend
```

Current production makes that fusion visible through spellings such as:

```text
qianqian-output-wasapi
wasapi_output_plugin()
desired("output", "wasapi_output_plugin")
```

The same historical path also caused platform-specific mechanism vocabulary (`GetBuffer`, `GetCurrentPadding`, `padding`, `IAudioStreamVolume`) to appear in descriptions of otherwise platform-neutral render obligations.

That was sufficient while Windows was the only real backend. It is not an acceptable long-term architecture once a second platform is introduced.

The architecture requirement is **backend-less playback semantics**, not “one platform-specific Output Plugin per operating system.”

---

## 2. Decision

The architecture is frozen as:

```text
Qianqian App / host assembly
        │ chooses one concrete host backend
        ▼
Output Plugin
    stable K0 composition identity
    provides AudioOutputCapability
        │ owns one concrete backend mechanism
        ▼
Host Render Backend
    backend-neutral AudioOutput contract
       /          |           \
      /           |            \
  WASAPI        ALSA        PipeWire / other
```

The three identities are distinct:

```text
Output Plugin
    composition/lifecycle identity

AudioOutputCapability / AudioOutput contract
    typed host-render dependency/mechanism seam

Concrete Host Render Backend
    platform-specific implementation mechanism
```

Normative shorthand:

> **Output is the Plugin. Backend is the mechanism. `AudioOutput` is the backend-neutral contract between them.**

---

## 3. Output Plugin is the stable composition role

The `Output Plugin` is the K0-composed role.

It keeps the same architecture identity independent of operating system or concrete host API:

```text
Windows     → Output Plugin
Linux       → Output Plugin
future OS   → Output Plugin
```

A desired composition identifies the stable output role, not a backend brand.

Therefore current/future desired-composition truth MUST NOT depend on names such as:

```text
wasapi_output_plugin
alsa_output_plugin
pipewire_output_plugin
coreaudio_output_plugin
```

Concrete Rust names remain representation, but the architecture identity observed by K0 is the stable Output Plugin role.

Playback Session continues to require only the output Capability. It MUST NOT branch on, depend on, or derive playback semantics from concrete backend identity.

---

## 4. Concrete backend is an owned mechanism, not a Plugin by default

WASAPI / ALSA / PipeWire / CoreAudio / CPAL-backed implementations are **not** independently K0-composed Plugins merely because they are:

```text
platform-specific
replaceable
large
stateful
implemented in separate crates
implemented with different threads/callback models
```

PBK-002 D13 applies directly.

For the current architecture a concrete Host Render Backend has:

```text
independent desired-composition truth?        NO
independent K0 lifecycle/dependency ordering? NO
can Output Plugin own it without losing
composition correctness/lifecycle ordering?  YES
```

Therefore:

> **A concrete host-audio backend MUST remain an owned mechanism/resource of the Output Plugin unless a future D13 review proves that independent K0 composition identity is required.**

Backend replaceability is an implementation/mechanism requirement, not Plugin admission evidence.

A future backend may earn Plugin identity only if all D13 conditions are independently demonstrated — for example a genuinely shared host service with independent desired presence, independent K0 lifecycle/dependency ordering, and correctness that cannot be preserved when owned by Output Plugin. That is a future authority event, not an implication of this ADR.

---

## 5. Backend-neutral Host Render Contract

`AudioOutputCapability` is the stable typed dependency. The current Rust `AudioOutput` / `RenderRequest` / `RenderStream` family is the current realization of the host-render contract; exact factory/trait/injection spelling remains replaceable representation.

A conforming Host Render Backend owes the same platform-neutral obligations regardless of host API:

```text
OPEN
    acquire one episode-scoped render stream or fail cleanly

FORMAT
    establish the render format required by the episode contract

PCM CONSUMPTION
    consume the already-bound RenderPcmInput directly
    without routing PCM through K0 / Capability resolution per quantum

TERMINAL EVIDENCE
    report drain/abort mechanism evidence through the existing owner seam
    without becoming D11 semantic authority

PARK / RELEASE
    honor the session-routed render gate before acquiring/holding a
    backend submission reservation that would be kept across the park;
    while parked submit no further PCM; release/stop remains bounded

TAIL OBSERVATION
    provide sound mechanism evidence for this stream's
    queued-to-future-presentation contribution

POSITION EVIDENCE
    publish the existing episode Position mechanism evidence from the
    backend's own submission/tail accounting; never become Position authority

OUTPUT LEVEL
    realize the App-owned desired stream factor under D14.9; never own
    the desired value or reinterpret it as acoustic truth

FAILURE
    surface backend/device/service failure into the existing output
    failure path; never invent a new playback terminal authority

TEARDOWN
    stop/join/release all backend-owned render resources under the
    existing Playback Session / Output Plugin ownership ordering
```

A backend is free to use a push loop, callback, event-driven engine, graph scheduler, or other native mechanism provided these obligations are refined correctly.

**Mechanism shape need not be identical across platforms. Semantic/mechanism obligations must be.**

---

## 6. Platform-neutral tail and cutover vocabulary

The generic contract MUST NOT define tail quiescence in terms of a specific host API.

Canonical abstract term:

```text
queued-to-future-presentation tail
```

Meaning:

> PCM already submitted by this episode's render leg that can still contribute to this stream's future presentation according to the host backend.

Canonical quiescence proposition:

> **TailQuiesced iff no PCM submitted before the relevant park/cut remains able to contribute to future presentation, while the parked leg is prevented from submitting more PCM.**

This is Mechanism Evidence, never a semantic Fact and never an acoustic-silence claim.

The existing F5 stale-PCM invariant remains unchanged:

> After a committed seek cutover, no pre-seek PCM may later contribute to this stream's queued-to-play / future-presentation output.

The backend must provide a sound refinement of that proposition. It does not have to expose a variable named `padding`.

---

## 7. Platform-neutral Position refinement

D14.8's product truth class remains unchanged:

```text
Position = non-authoritative episode-local Projection
```

The backend-neutral derivation concept is:

```text
handed_off_to_host
-
queued_to_future_presentation
=
host-render-consumed estimate
```

where both mechanism inputs belong to one render execution path and feed the existing single-writer Position evidence discipline.

A backend may obtain these inputs differently. The product contract does not require a particular OS counter or API call.

This ADR does not promote Position into a Fact and does not change the frozen seek-rebase rule.

---

## 8. Current Windows refinement

The current WASAPI implementation is one refinement of the generic contract, not the definition of it.

For the current Windows backend:

```text
backend submission reservation / handoff
    ← IAudioRenderClient::GetBuffer / ReleaseBuffer

queued-to-future-presentation tail
    ← IAudioClient::GetCurrentPadding

TailQuiesced while the render leg is parked
    ← GetCurrentPadding() == 0

stream-local desired level realization
    ← IAudioStreamVolume

backend/device failure evidence
    ← WASAPI / HRESULT failure classes
```

Accordingly, existing PBK-002 D14 references to:

```text
WASAPI
GetBuffer
ReleaseBuffer
GetCurrentPadding
padding
IAudioStreamVolume
HRESULT / AUDCLNT_*
```

are to be read as **current Windows realization / evidence** unless the text explicitly states a Windows-only mechanism choice. They do not require another backend to expose the same API or internal state.

This interpretation changes no accepted F3/F4/F5/D14.9 proposition. It removes backend-specific vocabulary from the meaning of the generic contract.

---

## 9. Host assembly selects the backend

Backend selection belongs to application/host assembly and product configuration, not Playback Session semantics.

Conceptually:

```text
host assembly
    chooses backend implementation
        ↓
constructs / injects stable Output Plugin
        ↓
Output Plugin provides AudioOutputCapability
        ↓
Playback Session consumes capability
```

The exact representation remains OPEN:

```text
compile-time cfg
factory function
constructor injection
feature-selected implementation
static or dynamic Rust dispatch
```

No representation is frozen by this ADR.

Runtime live switching between backends/devices is **not** authorized here. PBK-002's device-switch authority/replacement mechanism remains OPEN. A startup backend choice and a live device/backend replacement are different problems.

---

## 10. Authority firewall

A Host Render Backend is mechanism only.

It MUST NOT become semantic authority for:

```text
D11 Completed / Stopped / Failed
Pause / Resume intent
Seek acceptance or cutover commit
Open replacement commit
playlist/navigation truth
Position truth class
App desired volume
```

It may publish only the mechanism evidence already admitted by the existing contracts.

Likewise, backend identity MUST NOT become product truth. The player does not become semantically different because one machine uses WASAPI and another uses ALSA/PipeWire.

---

## 11. Current production differential

Current Windows production still fuses the stable Output Plugin role and the concrete WASAPI backend in the provider constructor/name:

```text
qianqian-output-wasapi::wasapi_output_plugin()
desired("output", "wasapi_output_plugin")
```

This is now an explicit **architecture-conformance differential**.

It does **not** invalidate the earned Windows transport semantics or physical evidence. It means only that the current representation exposes backend identity one layer too high.

Required corrective direction:

```text
current:
    WASAPI-specific Plugin constructor
        → AudioOutputCapability

corrective target:
    stable Output Plugin
        owns/injects
    WASAPI Host Render Backend
        implements/refines AudioOutput contract
```

The corrective MUST preserve the existing playback semantics and output behavior. It is not permission to redesign F3/F4/F5/F6/Volume.

Before claiming the output architecture backend-neutral — and before adding a Linux production backend — the Windows realization must pass a conformance slice demonstrating that separating Plugin identity from backend mechanism does not change the already-earned transport contract.

---

## 12. Required Windows conformance gate before Linux production

The separation corrective must re-run the relevant Windows evidence on the final corrected tree.

At minimum:

```text
natural playback / EOF
Stop
Pause / Resume + tail quiescence
Position publication
forward/backward Seek cutover
seek while paused
F5 device-failure-in-park regression
Open replacement
Navigation
Volume routing + replacement persistence
bounded teardown / resource cleanup
```

The purpose is semantic equivalence:

```text
plugin/backend identity split
    MUST NOT
change playback semantics
```

Only after this gate is green should Linux backend mechanism work begin.

---

## 13. Linux / future backend rule

A future Linux implementation plugs in below the stable Output Plugin boundary:

```text
Output Plugin
    ↓ owns
Linux Host Render Backend
    ↓
ALSA / PipeWire / CPAL-backed mechanism / other selected realization
```

Choosing among ALSA / PipeWire / CPAL is **not decided by this ADR**.

The choice must be earned by a narrow mechanism/reality gate against the backend-neutral obligations above, especially:

```text
real PCM output
tail/quiescence evidence
pause park behavior
seek cutover behavior
Position evidence
desired stream level realization
device/service failure
bounded teardown / RT behavior
```

A backend that can merely produce sound but cannot refine the frozen transport obligations is not production-conformant.

Do not weaken Windows-earned semantics merely to obtain a lowest-common-denominator cross-platform API.

---

## 14. D13 admission oracle extension

For future Plugin-boundary reviews, add this explicit negative candidate:

```text
candidate                     Plugin?
─────────────────────────────────────
concrete host-audio backend  NO, by default
```

Reason:

```text
platform specificity       != independent composition identity
replaceability             != independent composition identity
separate crate/API         != independent composition identity
mechanism complexity       != independent composition identity

stable Output Plugin can own the backend without losing
composition correctness or lifecycle/dependency ordering
→ D13 requires owned mechanism/resource
```

A proposal to make WASAPI / ALSA / PipeWire / CoreAudio itself a Plugin must therefore return to D13 with new evidence; it is not a representation choice.

---

## 15. Forbidden regressions

The following are architecture regressions unless a later authority decision explicitly reopens them:

1. make Playback Session depend on WASAPI/ALSA/PipeWire/CoreAudio identity;
2. encode backend brand as the stable desired-composition Output Plugin identity;
3. create one K0 Output Plugin per host backend merely for platform selection;
4. promote a backend to Plugin solely because it is replaceable/platform-specific/complex;
5. define generic `TailQuiesced` as `GetCurrentPadding() == 0` or any other single-platform API spelling;
6. require every backend to copy WASAPI's call sequence rather than refine the same neutral obligations;
7. silently weaken seek/pause/position/failure/volume semantics on a second platform because a convenience abstraction exposes less evidence;
8. move backend-specific device objects or counters into K0 kernel data;
9. introduce `BackendManager`, `BackendRegistry`, a generic audio runtime, or another layer without separate abstraction-earning evidence;
10. treat backend selection as playback semantic truth or a new Fact.

---

## 16. Formalization triage

This decision introduces no new independently legal semantic writer and no new temporal state machine.

The identified defect is an ownership/identity boundary and vocabulary/refinement leak:

```text
stable Plugin identity
vs
replaceable owned backend mechanism
```

Therefore:

```text
FORMALIZATION_NOT_EARNED
```

Use architecture/static boundary gates, regression tests, platform mechanism probes and physical evidence. If a future backend introduces a genuinely new concurrency/lifetime collision, formalization may be re-earned for that collision only.

---

## 17. Migration order

The authorized order is:

```text
1. this ADR / authority boundary
        ↓
2. Windows conformance refactor
   stable Output Plugin + owned WASAPI backend
        ↓
3. fresh Windows semantic-equivalence / dogfood gate
        ↓
4. Linux backend reality/mechanism gate
        ↓
5. Linux backend implementation
        ↓
6. cross-platform release / packaging work
```

Do not reverse this order by creating an `alsa_output_plugin` or `pipewire_output_plugin` first and trying to repair the architecture afterwards.

---

## 18. Summary

The durable rule is:

```text
Output Plugin
    = stable composition identity

AudioOutput
    = backend-neutral Host Render Contract

WASAPI / ALSA / PipeWire / CoreAudio / ...
    = replaceable owned mechanisms by default
```

Or, in one sentence:

> **K0 composes the Output Plugin; the Output Plugin owns a host backend; Playback Session consumes the capability; no playback semantic depends on which backend realizes it.**
