# F2 Authority Audit — Playback Semantic Truth

> **STATUS:** EVIDENCE / DESIGN INPUT. This report audits the current
> static playback architecture for candidate semantic truths and their
> authority candidates. It is not architecture authority; it promotes
> nothing. Any durable decisions earn their way into ADR corrective
> proposals only through explicit review.
>
> This report DOES NOT establish:
> - new architecture authority
> - new normative playback fact contract
> - new durable authority designation
>
> unless that durable decision is separately proposed through an ADR
> corrective.

```text
PHASE-F2-PLAYBACK-AUTHORITY-SEMANTICS-0

BASE_SHA:     8cf337e575ffa4f7cb805ebecee7c5692602b06b (main)
WORKTREE:     /home/hoo/Source/qianqian

AUTHORITY READ:
    PBK-001    (ACCEPTED — plane separation, Fact semantics, P1–P5)
    PBK-002    (ACCEPTED — canonical vocabulary, static composition)
    Issue #119  ROADMAP-CORRECTIVE-1 (F2–F8 ADR-conformant semantics)
    native-boundary-audit-0 ROUND-REPORT + REPORT
```

---

# 1. Production Reality Inventory

Every observable signal in the current playback path, annotated with
its runtime source, writer, readers, lifetime, current meaning, and
current consumers.

## 1.1 Signal inventory

| Signal | Runtime source | Writer | Readers | Lifetime | Current meaning | Current consumers |
|---|---|---|---|---|---|---|
| `SessionOutcome` | `CompletionState.outcome` | `resolve()` in completion.rs | `wait()` / `try_resolve_now()` | Once-resolved, memoized | How the episode ended | Headless CLI (print + exit code) |
| `SessionOutcome::Completed` | `resolve()` | candidate authority (see §4) | App | session-scoped | EOF + device drained | Headless exit code |
| `SessionOutcome::Stopped` | `resolve()` | candidate authority (see §4) | App | session-scoped | Stop intent observed before semantic resolution of aborted episode | Headless exit code |
| `SessionOutcome::Failed{stage}` | `resolve()` | candidate authority (see §4) | App | session-scoped | Episode failed; stage identifies failure class by resolver precedence | Headless exit code |
| `activation_failure` | `CompletionState.activation_failure` | `SessionCompletion::activation_failed()` | `activation_error()` | session-scoped | Why this activation attempt raised | Headless error message |
| `decode_failure` | `CompletionState.decode_failure` | `decode_worker` via `decode_failed()` | `resolve()` | session-scoped | Decode mechanism failed | Internal to resolve() |
| `worker_terminal` | `CompletionState.worker_terminal` | `worker_exited()` | `resolve()` | session-scoped | Edge terminal at worker exit | Internal to resolve() |
| `DrainVerdict` | `DrainInner.verdict` | Output mechanism `complete()` | `peek()` / `wait()` | session-scoped | How render leg terminated | resolve() + session completion |
| `DrainVerdict::Drained` | output mechanism | output mechanism | resolve() | session-scoped | EOF played out through device | resolve() |
| `DrainVerdict::Aborted` | output mechanism | output mechanism | resolve() | session-scoped | Render leg aborted (stop or failure) | resolve() |
| `stop_requested` | `CompletionState.stop_requested` | `request_stop()` | `stop_requested()` | session-scoped, command-state | Whether stop intent was recorded | F2 status (design) |
| `stop_target` | `CompletionState.stop_target` | `bind_stop_target()` | `request_stop()` | session-scoped | The edge to stop | session-internal |
| `source_format` | `CompletionState.source_format` | `set_source_format()` | `source_format()` | session-scoped, write-once | PCM format of the opened decode endpoint | Headless print |
| `EdgeTerminal` | `EdgeState.terminal` | `set_terminal()` (edge) | `terminal()` / `read_frames()` | edge-scoped | Data-plane terminal state | resolve(), worker, tests |
| `EdgeTerminal::Open` | edge | edge | consumers | edge-scoped | No terminal yet | edge-internal |
| `EdgeTerminal::Eof` | edge | edge | consumers | edge-scoped | Producer committed EOF | resolve() |
| `EdgeTerminal::Failed` | edge | edge | consumers | edge-scoped | Producer failed | resolve() |
| `EdgeTerminal::Stopped` | edge | edge | consumers | edge-scoped | Data plane stopped | resolve() |
| `buffered_frames` | `EdgeState.buffered / channels` | edge read/write | `buffered_frames()` | edge-scoped | Frames currently in ring | Tests (diagnostic only) |
| `FiberState::Active` | K0 kernel | K0 | App snapshot | composition-scoped | Component is live | Headless activation check |
| `FiberState::Failed` | K0 kernel | K0 | App snapshot | composition-scoped | Activation raised | Headless error path |
| `PcmFormat` | decode endpoint `format()` | decode service | session | session-scoped, write-once | Source PCM format | `set_source_format()` |
| `WASAPI open verdict` | output mechanism | output mechanism | session activation | session-scoped | Device opened or failed | activation error path |
| `render thread existence` | output mechanism | output mechanism | (internal) | session-scoped | Render leg is alive | mechanism-internal |
| `decode worker thread` | session activation | session spawn | join at teardown | session-scoped | Decode leg is alive | session-internal |

## 1.2 Signals explicitly NOT present in production

| Absent signal | Why it matters |
|---|---|
| `Playing` (semantic state) | No authority commits "playing has started" |
| `Starting` (semantic state) | No authority commits "episode is starting"; no bounded interval whose semantics mean "starting" |
| `Paused` | Not implemented |
| `Seeking` | Not implemented |
| `Stopping` (semantic state) | No separate authority; stop_requested is command-state |
| `current media path` (public) | Held by App as PathBuf, not published as fact by any session authority |
| `render position` / `playback position` | No render-position tracking |
| `elapsed time` | No time tracking |
| `buffer underrun count` | Not tracked |
| `device alive heartbeat` | Not tracked |
| `worker thread panic payload` | Caught, converted to string, not preserved |

---

# 2. Observation Classification

Every signal classified into exactly one primary category per PBK-001 §2.

## 2.1 COMMAND / INTENT

| Signal | Classification | Justification |
|---|---|---|
| `request_stop()` | COMMAND | Intent to stop; not a fact about the outcome. PBK-001 §2.2: "Command is intent, not fact." |
| `stop_requested` | COMMAND STATE | Records whether stop intent was recorded. The completion doc explicitly says: "Command state, not outcome truth." |
| `InteractiveCommand::Stop` | COMMAND | Parsed intent from stdin; not wired to any fact. |
| `InteractiveCommand::Pause` / `Resume` / `Seek` / etc. | COMMAND (unwired) | Recognized but not connected to any mechanism. |

## 2.2 MECHANISM EVIDENCE

| Signal | Classification | Justification |
|---|---|---|
| `DrainVerdict` | MECHANISM EVIDENCE | Produced by the output mechanism's render thread. It reports how the render leg terminated — a mechanism observation, not a semantic fact. The session's `resolve()` interprets it; the verdict alone does not determine the episode outcome. |
| `DrainVerdict::Drained` | MECHANISM EVIDENCE | "The device finished draining" — a mechanism observation. |
| `DrainVerdict::Aborted` | MECHANISM EVIDENCE | "The render leg aborted" — a mechanism observation. |
| `EdgeTerminal` | MECHANISM EVIDENCE | Data-plane terminal state observed by the decode worker. First-wins semantics are a mechanism property. |
| `EdgeTerminal::Eof` | MECHANISM EVIDENCE | "The decoder committed EOF on the edge" — mechanism. |
| `EdgeTerminal::Failed` | MECHANISM EVIDENCE | "The decoder failed" — mechanism. |
| `EdgeTerminal::Stopped` | MECHANISM EVIDENCE | "The data plane was stopped" — mechanism. Cannot distinguish user-stop from device-abort. |
| `worker_terminal` | MECHANISM EVIDENCE | The decode worker's exit terminal on the edge — mechanism observation. |
| `decode_failure` | MECHANISM EVIDENCE | Decode mechanism reported failure — mechanism observation. |
| `buffered_frames` | MECHANISM EVIDENCE | Ring occupancy diagnostic — mechanism observation. NOT semantic truth, NOT UI authority. |
| `source_format` | MECHANISM EVIDENCE | Decode endpoint's published format after decode open succeeds — mechanism observation. May exist even if later activation steps (output open, worker spawn) fail. |
| `activation_failure` | MECHANISM EVIDENCE | Kernel's diagnostic surface carried the FAILED verdict — mechanism evidence of why activation raised. |
| `WASAPI open verdict` | MECHANISM EVIDENCE | Device-open result — mechanism. |
| `render thread existence` | MECHANISM EVIDENCE | Whether the render thread is alive — mechanism. |

## 2.3 AUTHORITY STATE

| Signal | Classification | Justification |
|---|---|---|
| `SessionOutcome` | AUTHORITY STATE | The resolved semantic truth of how the episode ended. Currently established by `resolve()` (candidate authority — see §4), consumed by the App. |
| `FiberState::Active` / `Failed` | AUTHORITY STATE | K0 kernel truth about composition lifecycle — but this is K0 authority, not playback authority. |
| `CompositionSnapshot` | AUTHORITY STATE | K0 truth about the running composition. Projection for the App (read-only visibility). |

## 2.4 SEMANTIC FACT / OUTCOME

| Signal | Classification | Justification |
|---|---|---|
| `SessionOutcome::Completed` | SEMANTIC FACT (see §4 for authority candidate) | "The episode completed: EOF + device drained." |
| `SessionOutcome::Stopped` | SEMANTIC FACT (see §4 for authority candidate) | "Stop intent was observed before semantic resolution of an aborted episode." |
| `SessionOutcome::Failed{stage}` | SEMANTIC FACT (see §4 for authority candidate) | "The episode failed; stage identifies the failure class selected by resolver precedence." |

## 2.5 PROJECTION / DIAGNOSTIC

| Signal | Classification | Justification |
|---|---|---|
| `snapshot.fibers.get("session").state` | PROJECTION | K0 composition snapshot consumed by the App to check activation — a read-side observation, not playback authority. |
| `completion.source_format()` | PROJECTION (derived visibility) | Derived from the write-once `source_format` field — convenience accessor, not a separate authority. |
| `buffered_frames()` | PROJECTION / DIAGNOSTIC | Explicitly documented as "diagnostic mechanism evidence only — NOT PlaybackState, NOT product semantic truth, NOT UI-facing authority." |

## 2.6 LIFECYCLE / COMPOSITION STATE

| Signal | Classification | Justification |
|---|---|---|
| `PcmEdge` existence | COMPOSITION | The edge exists as a K0-composed resource — lifecycle, not semantic. |
| `render thread spawned` | COMPOSITION | Thread existence is lifecycle, not playback semantic truth. |
| `decode worker spawned` | COMPOSITION | Thread existence is lifecycle, not playback semantic truth. |
| `CompositionKernel` state | COMPOSITION | K0's internal state — not playback authority. |

---

# 3. Semantic Subject Scope

The semantic subject for every playback fact is:

> **The current playback episode.**

One episode = one `SessionCompletion` lifetime = one `PcmEdge` lifetime
= one decode-worker lifetime = one render-stream lifetime.

PBK-002 §8 freezes this as "one Playback Session is the ownership unit
of one playback episode." The subject scope is the episode, not the
application, not the device, not the kernel.

Cross-episode behavior (previous episode's outcome visible after the
episode ends) is explicitly NOT in scope for F2. The `SessionCompletion`
is consume-once per episode (documented, unenforced — limitation carried
from the native audit).

---

# 4. Fact Authority Cards

## Vocabulary for authority status

This report distinguishes three levels of authority status:

```text
CURRENT IMPLEMENTATION WRITER:
    The code location that currently writes this fact.
    A production property, not an architecture designation.

CANDIDATE DESIGNATED AUTHORITY:
    The semantic role that could become the architecture-level
    designated authority, if an ADR corrective is accepted.

ARCHITECTURALLY DESIGNATED AUTHORITY:
    An explicit architecture-level designation through ADR corrective.
    NOT YET for any playback fact — requires separate review.
```

Per PBK-001 §2.3: "For each semantic fact kind and its semantic subject
scope, there is exactly one designated semantic authority at a time."
Designation is explicit architecture-level contract; it cannot be
inferred from "current code has one writer" or "tests are deterministic."

---

## F2-T1 — Current Source

```text
FACT AUTHORITY CARD

NAME:               Current Source
PROPOSITION:        "The media source for the current playback episode."

FACT KIND:          PlaybackSource (proposed)

SEMANTIC SUBJECT:   This Playback Session activation attempt / episode
SUBJECT SCOPE:      One PlaybackSession / SessionCompletion lifetime

CURRENT IMPLEMENTATION WRITER:
    Session activation (session.rs:activate_inner) receives the Path,
    opens the decode endpoint. The App holds the PathBuf as handle
    ownership.

CURRENT PRODUCTION PROPERTY:
    PathBuf is passed into activation and used to open_media().
    No session-owned semantic source fact is currently stored or
    published after activation.

CANDIDATE DESIGNATED AUTHORITY:
    Playback Session activation — it receives the path and opens
    the decode endpoint. But no authoritative source identity is
    currently stored on the session for later read-side use.

ARCHITECTURALLY DESIGNATED:
    NOT YET — requires ADR corrective if frozen.

MECHANISM EVIDENCE USED:
    decode endpoint's format() after open_media() succeeds

SEMANTIC COMMIT POINT:
    If one existed: activate_inner() successfully opening the decode
    endpoint. But currently no source identity fact is committed —
    only source_format is stored.

CAN IT CHANGE AFTER COMMIT?:
    N/A — no source identity fact currently committed.

CONFLICTING EVIDENCE RULE:
    N/A

PUBLICATION / READ-SIDE SHAPE:
    None for source identity. source_format() exists as mechanism
    evidence readback, but does not carry source identity.

PROJECTION CONSUMERS:
    Future UI may need "what is the player playing" — but this is
    NOT currently earned.

CONTROL MAY USE PROJECTION?:
    NO — control does not depend on knowing the source.

CURRENT PRODUCTION MAPPING:
    PathBuf held by App → passed to activation → open_media().
    The App's PathBuf is handle ownership, not semantic authority.
    No session-owned source identity fact exists after activation.

CURRENT GAP:
    "Current source" as a persistent semantic fact is NOT EARNED.
    The App holds PathBuf as input ownership; the session uses it
    to open the decode endpoint but stores no source identity.
    A future session-owned source identity fact would need its own
    authority designation.

VERDICT:
    NOT EARNED
    Source identity as a semantic fact is not currently established.
    The App's PathBuf is input/request ownership, not playback
    semantic authority. Do not fabricate a source fact because UI
    will need one.
```

---

## F2-T2 — Playing

```text
FACT AUTHORITY CARD

NAME:               Playing
PROPOSITION:        "The episode is currently producing audible audio."

FACT KIND:          PlaybackPlaying (proposed)

SEMANTIC SUBJECT:   Current playback episode

CURRENT IMPLEMENTATION WRITER:
    NO WRITER EARNED.

    The current architecture has NO single observation that establishes
    "audio is audible." Each candidate is mechanism evidence, not a
    semantic fact:

    - Fiber Active:          composition lifecycle, not playback
    - decode open succeeded: mechanism evidence (activation succeeded
                            does not mean audio is playing)
    - output open succeeded: mechanism evidence (device open ≠ playing)
    - render started:        mechanism evidence (thread spawned ≠ audio
                            submitted)
    - first PCM produced:    mechanism evidence (PCM in edge ≠ device
                            received)
    - first PCM consumed:    mechanism evidence (device read from edge
                            ≠ sample physically audible)
    - WASAPI started:        mechanism evidence (API call succeeded ≠
                            audible)
    - buffered_frames > 0:   mechanism evidence (ring occupancy ≠
                            audible)

    The gap: between "WASAPI Start succeeded" and "first sample
    physically audible" there are hardware buffering stages with no
    feedback mechanism. Qianqian cannot observe physical audibility.

    Even the "WASAPI started" evidence is not published to any
    semantic authority — it lives inside the output mechanism's
    internal state.

AUTHORITY STATE:    NONE

MECHANISM EVIDENCE USED:
    None sufficient.

SEMANTIC COMMIT POINT:
    NONE EARNED.

CAN IT CHANGE AFTER COMMIT?:
    N/A

CONFLICTING EVIDENCE RULE:
    N/A

PUBLICATION / READ-SIDE SHAPE:
    None.

PROJECTION CONSUMERS:
    None.

CONTROL MAY USE PROJECTION?:
    N/A

CURRENT PRODUCTION MAPPING:
    The headless CLI prints "playing ..." after activation succeeds —
    this is a derived projection, not a committed fact.

CURRENT GAP:
    No mechanism publishes "playing" as a semantic fact. The architecture
    lacks a designated authority for this proposition.

VERDICT:
    NOT_EARNED
    "Playing" is NOT a semantic fact the current architecture can
    truthfully commit. The headless "playing ..." line is projection.
    F2 must not fabricate this fact.
```

**Adversarial note (Q1–Q2):**

- Q1: If Fiber == Active but output open hasn't completed → `Playing`
  cannot成立. The session's activation succeeds only when BOTH legs
  (decode open + render stream open) succeed. But even after both open,
  "playing" as a product semantic is still not committed because we
  cannot observe physical audibility.

- Q2: If decoder has produced PCM but device hasn't submitted any audio
  → `Playing` still cannot成立 for the same reason. The architecture
  has no designated authority for this proposition.

**Design decision:** The product semantics Qianqian needs may be
satisfied by a coarser fact. See §7 (projection design) for what CAN
be truthfully shown.

## F2-T3 — Starting

```text
FACT AUTHORITY CARD

NAME:               Starting
PROPOSITION:        "The episode is in the process of starting."

FACT KIND:          PlaybackStarting (proposed)

CURRENT IMPLEMENTATION WRITER:
    NONE — no code commits this.

DESIGNATED AUTHORITY:
    NO DESIGNATED AUTHORITY EARNED.

    The previous report proposed deriving "Starting" from:
    - source selected (App holds PathBuf)
    - activation succeeded (FiberState::Active in K0 snapshot)
    - no terminal committed yet (SessionOutcome is None)

    This derivation is INVALID: FiberState::Active means K0
    activation completed. With no Playing semantic fact, the
    condition "Active + no terminal" remains true for the
    ENTIRE active playback episode — not just the beginning.
    It therefore labels normal ongoing playback as "Starting,"
    which is a false projection.

AUTHORITY STATE:    NONE

SEMANTIC COMMIT POINT:
    NONE EARNED. No bounded interval exists whose semantics
    mean "episode is starting."

VERDICT:
    NOT_EARNED
    "Starting" cannot be truthfully derived from current authority
    state. The predicate "Active + no terminal" is true for the
    whole active episode, not just the start. There is currently
    no authoritative transition or bounded interval whose semantics
    mean "episode is starting." F2 must not expose this.
```

---

## F2-T4 — Completed

```text
FACT AUTHORITY CARD

NAME:               Completed
PROPOSITION:        "The episode finished: EOF was produced, the output
                    mechanism reported its drain contract completed, and
                    the Playback Session resolver committed Completed."

FACT KIND:          SessionOutcome::Completed

SEMANTIC SUBJECT:   Current playback episode
SUBJECT SCOPE:      One SessionCompletion lifetime

CURRENT IMPLEMENTATION WRITER:
    The resolve() function in completion.rs — it is the single writer
    of CompletionState.outcome. It observes:
    1. decode_failure (mechanism evidence)
    2. worker_terminal (mechanism evidence)
    3. DrainVerdict (mechanism evidence)
    And applies the precedence rule:
      decode_failure → Failed{decode}
      worker_terminal == Failed → Failed{decode}
      DrainVerdict::Drained + worker_terminal == Eof → Completed
      DrainVerdict::Aborted + stop_requested → Stopped
      DrainVerdict::Aborted + !stop_requested → Failed{device}

CANDIDATE DESIGNATED AUTHORITY:
    Playback Session episode-outcome semantic authority.
    Current realization: SessionCompletion::resolve().
    Mechanism evidence may inform that decision, but does not
    itself establish playback outcome truth.

ARCHITECTURALLY DESIGNATED:
    NOT YET — requires ADR corrective if frozen.
    PBK-002 §14 leaves PlaybackFacts OPEN.

AUTHORITY STATE:
    SessionOutcome (memoized after first resolution)

MECHANISM EVIDENCE USED:
    DrainVerdict::Drained (output mechanism reports all frames played
    through its drain contract)
    + EdgeTerminal::Eof (decode worker confirms EOF on the edge)

SEMANTIC COMMIT POINT:
    resolve() first produces SessionOutcome::Completed. Once committed,
    it is memoized and never changes.

CAN IT CHANGE AFTER COMMIT?:
    No — resolve() is first-wins: if outcome is already Some, it
    returns immediately.

CONFLICTING EVIDENCE RULE:
    Decode failure is authoritative over everything (checked first in
    resolve()). A late stop after Completed does not rename the outcome.

PUBLICATION / READ-SIDE SHAPE:
    wait() / try_resolve_now() — blocking / non-blocking read on
    SessionCompletion.

PROJECTION CONSUMERS:
    Headless CLI (print + exit code), future UI.

CONTROL MAY USE PROJECTION?:
    The App reads the outcome to decide exit code — this IS control,
    but it queries the authority (resolve()), not a projection.

CURRENT PRODUCTION MAPPING:
    resolve() in completion.rs:259-318. Precedence: decode_failure >
    worker_terminal == Failed > (DrainVerdict::Drained + Eof) >
    (DrainVerdict::Aborted + stop_requested) >
    (DrainVerdict::Aborted + !stop_requested).

CURRENT GAP:
    Completed requires BOTH legs to reach their terminal: decode EOF
    AND output drain contract completed. Use contract-level wording:
    "the output mechanism reported its drain contract completed" —
    do NOT upgrade "device buffer empty" to "listener definitely
    heard final sample" unless actually guaranteed.

    The gap is that "device drained" is mechanism evidence
    (DrainVerdict::Drained) — the output mechanism publishes this
    after the drain loop completes. The resolve() function
    INTERPRETS this mechanism evidence as a semantic fact. This
    interpretation is the semantic commit — resolve() is the
    candidate authority.

VERDICT:
    CANDIDATE — implementation classification sound, candidate
    semantic fact earned by production evidence, architectural
    designation requires ADR corrective if frozen.
    Causal overclaim: none — Completed does not claim physical
    audibility, only contract-level drain completion.

    Adversarial Q3: If decoder EOF but WASAPI still has 300ms buffered
    → Completed CANNOT成立 yet. resolve() waits for DrainVerdict::Drained
    before committing Completed. This is correct: "completed" means
    the output drain contract completed, not just the decoder finished
    producing.
```

---

## F2-T5 — Stopped

```text
FACT AUTHORITY CARD

NAME:               Stopped
PROPOSITION:        "At semantic resolution time, an aborted terminal
                    episode had recorded stop intent and no
                    higher-precedence failure was committed."

FACT KIND:          SessionOutcome::Stopped

SEMANTIC SUBJECT:   Current playback episode
SUBJECT SCOPE:      One SessionCompletion lifetime

CURRENT IMPLEMENTATION WRITER:
    The resolve() function in completion.rs (same writer as Completed).

CANDIDATE DESIGNATED AUTHORITY:
    Playback Session episode-outcome semantic authority.
    Current realization: SessionCompletion::resolve().

ARCHITECTURALLY DESIGNATED:
    NOT YET — requires ADR corrective if frozen.

AUTHORITY STATE:
    SessionOutcome (memoized after first resolution)

MECHANISM EVIDENCE USED:
    DrainVerdict::Aborted (render mechanism reports abort)
    + worker_terminal == EdgeTerminal::Stopped (decode worker observed
      edge stop)
    + stop_requested == true (command state — classifies the abort as
      having recorded stop intent)

SEMANTIC COMMIT POINT:
    resolve() first produces SessionOutcome::Stopped. Memoized.

CAN IT CHANGE AFTER COMMIT?:
    No — first-wins.

CONFLICTING EVIDENCE RULE:
    Decode failure is authoritative over Stopped (checked first).
    A late Completed does not override Stopped.

PUBLICATION / READ-SIDE SHAPE:
    wait() / try_resolve_now().

PROJECTION CONSUMERS:
    Headless CLI, future UI.

CONTROL MAY USE PROJECTION?:
    Queries authority (resolve()), not projection.

CURRENT PRODUCTION MAPPING:
    resolve() at completion.rs:292-306. The critical discriminator:
    when DrainVerdict::Aborted AND worker_terminal == Stopped, the
    function checks stop_requested. If true → Stopped. If false →
    Failed{device}.

CURRENT GAP — CAUSE-LOSS:
    The proposition is deliberately narrow: "stop intent was observed
    before semantic resolution of an aborted episode." This does NOT
    claim "the user's stop caused the termination."

    Known limitation: a device abort that races a user stop may still
    resolve Stopped because both paths land on the identical edge
    terminal. Stop intent recorded AFTER the abort resolves still
    produces Stopped, because request_stop() sets stop_requested
    unconditionally and resolve() checks it later. The discriminator
    is "was stop intent recorded at the time resolve() processes the
    Aborted+Stopped state?" not "was stop intent recorded before the
    abort?"

    This is a LIMITATION, not an architecture defect, because the
    narrowed proposition does not require causal truth. If a future
    product requirement demands "the user's stop caused termination,"
    that would require cause-carrying mechanism evidence — an
    AUTHORITY_GAP / MECHANISM_EVIDENCE_GAP at that point.

VERDICT:
    CANDIDATE — deterministic classification, narrow (non-causal)
    proposition, candidate semantic fact. Architectural designation
    requires ADR corrective if frozen.
    Cause-loss limitation: explicit, not an architecture defect for
    the current narrowed proposition.

    Adversarial Q4: If user stop THEN decode error → decode failure
    wins because decode_failure is checked first in resolve(). This
    is correct: the decode failure is the more informative truth about
    why the episode ended, and it was published before the edge was
    stopped.

    Adversarial Q6: If SessionCompletion already Completed and then
    late stop_requested=true → the outcome is already memoized as
    Completed. The late stop is a no-op. Projection should NOT show
    "Stopping" — the outcome is already committed.
```

---

## F2-T6 — Failed

```text
FACT AUTHORITY CARD

NAME:               Failed
PROPOSITION:        "The episode failed before completion."

FACT KIND:          SessionOutcome::Failed { stage: String }

SEMANTIC SUBJECT:   Current playback episode
SUBJECT SCOPE:      One SessionCompletion lifetime

CURRENT IMPLEMENTATION WRITER:
    The resolve() function in completion.rs (same writer as
    Completed/Stopped).

CANDIDATE DESIGNATED AUTHORITY:
    Playback Session episode-outcome semantic authority.
    Current realization: SessionCompletion::resolve().

ARCHITECTURALLY DESIGNATED:
    NOT YET — requires ADR corrective if frozen.

AUTHORITY STATE:
    SessionOutcome (memoized after first resolution)

MECHANISM EVIDENCE USED:
    Multiple causes, checked in precedence order:
    1. decode_failure string (decode mechanism failure)
    2. worker_terminal == Failed (decode worker observed edge failure
       without a message)
    3. DrainVerdict::Aborted + worker_terminal != Stopped ||
       !stop_requested (device abort without user stop)

SEMANTIC COMMIT POINT:
    resolve() first produces SessionOutcome::Failed{stage}. Memoized.

CAN IT CHANGE AFTER COMMIT?:
    No — first-wins.

CONFLICTING EVIDENCE RULE:
    Decode failure is authoritative over everything downstream.

PUBLICATION / READ-SIDE SHAPE:
    wait() / try_resolve_now().

PROJECTION CONSUMERS:
    Headless CLI, future UI.

CONTROL MAY USE PROJECTION?:
    Queries authority, not projection.

CURRENT PRODUCTION MAPPING:
    resolve() at completion.rs:265-312. Three failure paths:
    - decode_failure string → Failed{stage: "decode: {message}"}
    - worker_terminal == Failed → Failed{stage: "decode"}
    - DrainVerdict::Aborted + !stop_requested → Failed{stage: "device"}

CURRENT GAP:
    "Failed{stage}" is one fact kind with structured cause (the stage
    string). The stage identifies the failure class selected by the
    resolver's semantic precedence — it does NOT imply chronological
    first-failure. A device abort can occur first in wall-clock time,
    yet a published decode failure can still dominate semantic
    resolution.

    The stage string is mechanism-level detail ("decode: {message}" or
    "device") — it is NOT a separate (fact kind, subject scope). The
    semantic truth is "the episode failed"; the stage is diagnostic
    detail on that same fact.

    Adversarial Q5: If device dies and render path calls edge.stop(),
    worker sees Stopped, and abort path publishes DrainVerdict::Aborted.
    If stop_requested == false → Failed{device}. The authority is
    resolve(), interpreting mechanism evidence. The output mechanism
    does NOT directly publish Failed — it publishes DrainVerdict::Aborted
    (mechanism evidence), and resolve() interprets it.

VERDICT:
    CANDIDATE — implementation classification sound, candidate
    semantic fact. Stage identifies failure class by resolver
    precedence, not chronological first-failure.
    Architectural designation requires ADR corrective if frozen.
```

---

## Additional discovered facts

### source_format (within episode)

```text
FACT AUTHORITY CARD

NAME:               Source Format
PROPOSITION:        "The opened decode endpoint reports PCM format X."

FACT KIND:          PlaybackSourceFormat (proposed)

SEMANTIC SUBJECT:   Current playback episode / decode endpoint
SUBJECT SCOPE:      One SessionCompletion lifetime

CURRENT IMPLEMENTATION WRITER:
    Session activation (session.rs:activate_inner). It calls
    set_source_format() exactly once after successful decode open.

CURRENT PRODUCTION PROPERTY:
    Write-once after decode open succeeds. May exist even if later
    activation steps (output open, worker spawn) subsequently fail.
    This is a mechanism-derived write-once observation — not proof
    that the playback episode successfully activated.

ARCHITECTURALLY DESIGNATED:
    NOT_APPLICABLE — mechanism evidence is not a semantic-fact
    candidate and needs no semantic-authority designation
    (writer != semantic authority).

AUTHORITY STATE:
    source_format (write-once, memoized)

MECHANISM EVIDENCE USED:
    decode endpoint's format() after open_media()

SEMANTIC COMMIT POINT:
    set_source_format() in activate_inner()

CAN IT CHANGE AFTER COMMIT?:
    No — write-once.

CONFLICTING EVIDENCE RULE:
    If decode open itself fails, no format is published. But if
    decode open succeeds and later activation steps fail, the
    format IS committed — it is not rolled back.

PUBLICATION / READ-SIDE SHAPE:
    source_format() accessor on SessionCompletion.

VERDICT:
    MECHANISM EVIDENCE — mechanism-derived write-once observation.
    Narrow proposition: "the opened decode endpoint reports format X."
    NOT proof that the episode activated successfully.
    Do NOT mix with source identity. Not a semantic-fact candidate;
    no ADR authority designation required.
```

### activation_error

```text
FACT AUTHORITY CARD

NAME:               Activation Error
PROPOSITION:        "This Playback Session activation attempt failed;
                    here is why."

FACT KIND:          PlaybackActivationFailed (proposed)

SEMANTIC SUBJECT:   This Playback Session activation attempt
SUBJECT SCOPE:      One activation attempt (not a successfully
                    activated episode — activation failure occurs
                    before any episode exists)

CURRENT IMPLEMENTATION WRITER:
    Session activation (session.rs:activate). It calls
    activation_failed() when activate_inner() returns Err.

ARCHITECTURALLY DESIGNATED:
    NOT_APPLICABLE — mechanism evidence / write-once diagnostic is
    not a semantic-fact candidate and needs no semantic-authority
    designation (writer != semantic authority).

AUTHORITY STATE:
    activation_failure (write-once, memoized)

MECHANISM EVIDENCE USED:
    ActivationError from capability resolution, decode open, render
    stream open, or worker spawn failure.

SEMANTIC COMMIT POINT:
    activation_failed() in session.rs:59-62.

VERDICT:
    MECHANISM EVIDENCE — write-once diagnostic, conflict-free.
    Not a semantic-fact candidate; no ADR authority designation
    required.

    Subject scope note: The producing mechanism (session activation)
    is the current implementation writer of the stored value; that
    does NOT make it a designated semantic authority. The subject is
    "this activation attempt," NOT "current playback episode" —
    activation failure occurs before a successfully activated
    playback episode exists. Activation failure != episode terminal
    outcome.
```

---

# 5. Authority Candidates Comparison

| Candidate | Semantic ownership | Physical lifetime | Fact scope | Writers | Pause/Seek compat | UI access | Cross-episode | Stale risk | K0 leakage | Verdict |
|---|---|---|---|---|---|---|---|---|---|---|
| **Session (session.rs)** | One episode | Episode-scoped | episode-outcome, source_format, activation | resolve() is single writer | episode-scoped; pause/seek would need new authority | App reads completion | No | Low (consume-once) | No | **CANDIDATE** for episode outcome |
| **SessionCompletion** | One episode | Episode-scoped | same as Session | same | same | App holds handle | No | Low | No | **CANDIDATE** (it IS the session's application-facing seam) |
| **App** | None (PBK-002 D3) | Application | N/A | N/A | N/A | IS the consumer | N/A | N/A | No | **REJECTED** — App is bootstrap/shutdown, not playback authority |
| **K0 / Fiber state** | Composition | composition-scoped | lifecycle only | K0 kernel | No | snapshot read | No | Low | YES — would leak | **REJECTED** — K0 owns existence, not playback semantics |
| **Decode Plugin** | decode mechanism | provider-lifetime | decode only | decode mechanism | No | No | No | Low | No | Only for decode-specific facts (e.g., decode_failed) |
| **Output Plugin** | output mechanism | provider-lifetime | drain only | output mechanism | No | No | No | Low | No | Only for drain-specific facts (DrainVerdict) |

**Decision:** For the current one-episode playback topology, the Playback
Session is the candidate designated semantic authority for that episode's
terminal outcome. Current realization: `SessionCompletion::resolve()`.

It is:

1. **Single writer:** resolve() is the only function that writes to `CompletionState.outcome`.
2. **Episode-scoped:** one handle per episode, consume-once.
3. **Correct scope:** owns the decode worker, edge, and render stream relationships.
4. **No K0 leakage:** the completion is a session-owned seam, not a K0 primitive.
5. **No mechanism leakage:** resolve() interprets mechanism evidence but is not itself a mechanism.
6. **Testable:** unit-testable with mock DrainVerdict/EdgeTerminal inputs.

The current authority chain is:

```text
mechanism evidence (DrainVerdict, EdgeTerminal, decode_failure)
        ↓
resolve() — current implementation writer (candidate authority)
        ↓
semantic commit (memoized SessionOutcome)
        ↓
publication (wait() / try_resolve_now())
        ↓
projection / headless status / future UI
```

**This chain is production reality, not architecture designation.** To
freeze this as architecture authority, an ADR corrective is required.

---

# 6. Adversarial Questions — Answers

## Q1: Fiber Active but output open not complete → Playing?

**Answer:** `Playing` cannot be committed. The current architecture has
no designated authority for "Playing." Even if we wanted one, the
condition "output open succeeded" is mechanism evidence inside the
session's activation — not a published semantic fact. The session's
activation succeeds when both legs open, but activation success ≠
"playing."

## Q2: Decoder produced PCM but device hasn't submitted → Playing?

**Answer:** Same as Q1. `Playing` is NOT EARNED. The architecture
cannot truthfully commit this fact.

## Q3: Decoder EOF but WASAPI has 300ms buffered → Completed?

**Answer:** `Completed` CANNOT be committed yet. resolve() requires
DrainVerdict::Drained (output drain contract completed) AND
worker_terminal == Eof. The 300ms of buffered audio means the drain
contract is not yet satisfied. This is correct behavior.

## Q4: User stop then decode error → final truth?

**Answer:** `Failed{stage: "decode: ..."}`. decode_failure is checked
first in resolve() and is authoritative over Stopped. The decode
failure is the more specific truth about why the episode ended.

**Who decides?** resolve() — the candidate authority.

## Q5: Device dies → who owns Failed{device}?

**Answer:** resolve() owns the Failed{device} fact. The output
mechanism publishes DrainVerdict::Aborted (mechanism evidence). The
decode worker observes EdgeTerminal::Stopped. resolve() interprets
both: if !stop_requested → Failed{device}. The output mechanism does
NOT directly publish Failed.

## Q6: Completed then late stop_requested → projection shows Stopping?

**Answer:** Absolutely NOT. The outcome is already memoized as
Completed. A late stop_requested is a no-op with respect to the
outcome. The completion doc explicitly says: "Calling this after the
outcome is resolved is a no-op." Any projection showing "Stopping"
after Completed would be lying.

## Q7: Episode terminal but process alive → Idle/Completed/Stopped?

**Answer:** This exposes that "terminal outcome" and "current activity"
are different dimensions. The terminal outcome (Completed/Stopped/Failed)
is the candidate authority fact. "Idle" would be a projection: "no active
episode" — derivable from (no active PlaybackSession fiber in K0
composition). These are orthogonal:

- Terminal outcome = "how did the episode end?" (SessionCompletion candidate authority)
- Current activity = "is the player doing something?" (projection from K0 state)

## Q8: Future UI reconnect → snapshot or replay?

**Answer:** Current snapshot. The candidate authority (SessionCompletion)
holds the memoized outcome. No event log is needed. The App reads
try_resolve_now() and gets the committed truth. This is sufficient
because the current architecture is single-episode, single-outcome.

---

# 7. The Known Cause-Loss Hole

## 7.1 The problem

```text
USER STOP                          DEVICE ABORT
    ↓                                  ↓
command intent                   mechanism failure
    ↓                                  ↓
request_stop()                   render path calls edge.stop()
    ↓                                  ↓
edge.stop() → Stopped           edge.stop() → Stopped (same terminal)
    ↓                                  ↓
DrainVerdict::Aborted            DrainVerdict::Aborted (same verdict)
    ↓                                  ↓
resolve(): stop_requested=true   resolve(): stop_requested=false
    ↓                                  ↓
SessionOutcome::Stopped          SessionOutcome::Failed{device}
```

Both paths produce the identical mechanism evidence (EdgeTerminal::Stopped
+ DrainVerdict::Aborted). The only discriminator is `stop_requested`
— command state used as evidence.

## 7.2 Is this cause-carrying evidence?

**No.** `stop_requested` is NOT cause-carrying evidence. It is a
boolean flag: "was stop intent recorded?" It does NOT carry:

```text
WHY the abort happened
WHEN the abort happened relative to the stop
WHETHER the abort was caused by the stop or was independent
```

The current discriminator works because:
1. request_stop() publishes intent BEFORE releasing the edge.
2. Any stop-caused Stopped necessarily observes the intent.
3. A device abort without a stop request leaves stop_requested=false.

But: stop intent recorded AFTER the abort resolves still produces
Stopped, because request_stop() sets stop_requested unconditionally
and resolve() checks it later.

## 7.3 Classification of the limitation

The narrowed proposition for Stopped is:

> "At semantic resolution time, an aborted terminal episode had
> recorded stop intent and no higher-precedence failure was committed."

This does NOT require causal truth. Therefore the cause-loss is a
**LIMITATION** — the semantic proposition is honest about what the
current mechanism can determine. It is NOT an architecture defect for
the current narrowed proposition.

If a future product requirement demands "the user's stop caused
termination," that would require cause-carrying mechanism evidence
(e.g., `DrainVerdict::Aborted { cause: UserStop | DeviceFailure }`).
At that point the cause-loss becomes an **AUTHORITY_GAP /
MECHANISM_EVIDENCE_GAP**.

**Deferred to later phase:** A cause-carrying drain verdict would
improve diagnostic accuracy but is not required for F2 semantic
correctness under the current narrowed proposition.

---

# 8. Re-Admission Gate

## 8.1 buffered_frames()

**Verdict: RETIRE from PlaybackSnapshot (if one is ever created).**

Reasoning:
1. `buffered_frames()` is mechanism evidence (ring occupancy diagnostic).
2. No product semantic depends on exact edge occupancy.
3. No semantic control may legally depend on it (PBK-001: control must
   query authority state, not mechanism evidence).
4. The doc already states: "NOT PlaybackState, NOT product semantic
   truth, NOT UI-facing authority."
5. It stays `pub` ONLY because integration tests live outside the crate.

**If a PlaybackSnapshot is created:** `buffered_frames` must NOT appear
in it. It remains available as a test/verifier diagnostic seam only.

**Current status:** Correctly classified. No change needed.

## 8.2 stop_requested()

**Verdict: COMMAND STATE, not playback semantic fact.**

Reasoning:
1. `stop_requested` records whether stop intent was recorded — command
   state per PBK-001 §2.2.
2. It is NOT a semantic fact about the episode's outcome.
3. It IS used as evidence in the semantic decision (resolve() checks
   it to classify an aborted episode as having recorded stop intent).
4. "Stopping..." would be a projection derived from (stop_requested AND
   no terminal committed yet). It should NOT be an independent semantic
   authority.

**If a PlaybackSnapshot is created:** `stop_requested` could appear as
a labeled derived projection field, explicitly marked as command-state,
not outcome. But it must never be used as a correctness basis.

**Current status:** Correctly classified. No change needed.

---

# 9. Projection Design

## 9.1 What CAN be truthfully shown today

Given the authority audit, a headless status or future UI projection
may truthfully display:

```text
OUTCOME:
    SessionOutcome (authority state from resolve()):
    - Completed: EOF + device drain contract completed
    - Stopped: stop intent observed before resolution of aborted episode
    - Failed{stage}: episode failed (stage = failure class by precedence)

SOURCE FORMAT:
    source_format (mechanism evidence, write-once, within episode).
    Not source identity — just the PCM format of the opened endpoint.

ACTIVATION:
    Whether activation succeeded (FiberState::Active in K0 snapshot —
    this is K0 authority, used as read-side visibility).
    activation_failure (mechanism evidence, within this activation
    attempt).

COMMAND STATE:
    stop_requested (command state, labeled as such).

DIAGNOSTIC (not for product display):
    buffered_frames (mechanism evidence).
```

## 9.2 What MUST NOT be shown yet

```text
Playing          NOT EARNED — no designated authority
Starting         NOT EARNED — derivation is semantically false
                   (Active + no terminal is true for whole episode)
Paused           NOT IMPLEMENTED
Seeking          NOT IMPLEMENTED
Stopping         NOT EARNED — derivable from (stop_requested + no terminal)
                   but not independently earned
Position         NOT TRACKED
Elapsed time     NOT TRACKED
Buffer health    NOT A SEMANTIC FACT
Device status    NOT TRACKED
Source path      NOT EARNED as semantic fact
```

## 9.3 Projection contract

If a PlaybackSnapshot or similar projection is created:

```text
It is derived visibility.
It is NOT designated semantic authority.
It may be stale according to its contract.
Observers may render it.
Control correctness must query authority state,
    not use the projection as the correctness basis.
```

Forbidden:
```text
UI reads snapshot → decides architecture-critical teardown legality
```

## 9.4 Snapshot composition ownership

A `SessionCompletion::snapshot()` method can only honestly expose
fields that `SessionCompletion` actually owns or stores:

```text
SessionCompletion owns/stores:
    outcome (authority state)
    source_format (mechanism evidence, write-once)
    activation_failure (mechanism evidence, write-once)
    stop_requested (command state)

SessionCompletion does NOT own:
    source path (held by App as input ownership)
    K0 Fiber state (K0 authority)
    K0 composition snapshot (K0 authority)
    activation success/failure from K0 perspective
```

Therefore a `SessionCompletion::snapshot()` can only provide:

```text
outcome: Option<SessionOutcome>
source_format: Option<PcmFormat>
activation_error: Option<String>
stop_requested: bool
```

It cannot synthesize `source: Option<PathBuf>` (App input ownership,
not session-owned) or `activation: succeeded/failed` from K0 state
(K0 authority, not session-owned).

If a future UI needs a combined view that merges App input + K0 state
+ session authority, that must be assembled at the App level as a
projection — NOT inside `SessionCompletion::snapshot()`. That
App-level assembly is:

```text
projection assembly ownership (App)
separate from
semantic authority ownership (SessionCompletion)
```

---

# 10. Headless Status Design

## 10.1 What `status` can truthfully print today

```text
outcome: Completed | Stopped | Failed{stage} | (pending)
format: {sample_rate} Hz, {channels} channels, mask {channel_mask}
stop_requested: true | false
activation: succeeded | failed ({message})
```

## 10.2 What must NOT be shown

```text
state: Playing / Paused / Stopped (as a playback state)
position: 1:23 / 3:45
buffered: 1234 frames
source: {path}  (App input, not session authority)
```

## 10.3 Design principle

If the current architecture can only truthfully answer "how did the
episode end?" and "what is the source format?" — then F2 implementation
only implements those. Do not fabricate status to look complete.

---

# 11. Command/Fact Histories

## H1 — Play → activation success → EOF → completed

```text
COMMAND:           play <file>
MECHANISM EVIDENCE: decode open ok, render stream open ok,
                   DrainVerdict::Drained, EdgeTerminal::Eof
SEMANTIC DECISION:  resolve() — Drained + Eof → Completed
FACT COMMIT:        SessionOutcome::Completed (memoized)
PROJECTION UPDATE:  headless prints "EOF: played out completely"
```

**Single writer:** resolve(). No MAJOR.

## H2 — Play → stop → stopped

```text
COMMAND:           play <file>, then stop
MECHANISM EVIDENCE: request_stop() → edge.stop() → DrainVerdict::Aborted,
                   worker_terminal == Stopped
SEMANTIC DECISION:  resolve() — Aborted + Stopped + stop_requested → Stopped
FACT COMMIT:        SessionOutcome::Stopped (memoized)
PROJECTION UPDATE:  headless prints "stopped before completion"
```

**Single writer:** resolve(). No MAJOR.

## H3 — Stop before binding → activation → stopped

```text
COMMAND:           stop before activation binds edge
MECHANISM EVIDENCE: stop_requested = true (recorded before edge exists)
                   bind_stop_target() applies the stop immediately
                   DrainVerdict::Aborted, worker_terminal == Stopped
SEMANTIC DECISION:  resolve() — same as H2
FACT COMMIT:        SessionOutcome::Stopped
PROJECTION UPDATE:  headless prints "stopped before completion"
```

**Single writer:** resolve(). No MAJOR.

## H4 — Play → decode failure → Failed{decode}

```text
COMMAND:           play <file>
MECHANISM EVIDENCE: decode_failed("...") called, edge.fail()
SEMANTIC DECISION:  resolve() — decode_failure checked first →
                   Failed{stage: "decode: ..."}
FACT COMMIT:        SessionOutcome::Failed{stage: "decode: ..."}
PROJECTION UPDATE:  headless prints error
```

**Single writer:** resolve(). No MAJOR.

## H5 — Play → device abort → Failed{device}

```text
COMMAND:           play <file>
MECHANISM EVIDENCE: DrainVerdict::Aborted, worker_terminal != Stopped
                   OR worker_terminal == Stopped + !stop_requested
SEMANTIC DECISION:  resolve() — Aborted + no stop intent → Failed{device}
FACT COMMIT:        SessionOutcome::Failed{stage: "device"}
PROJECTION UPDATE:  headless prints error
```

**Single writer:** resolve(). No MAJOR.

**Note (MINOR-2 corrective):** If a device abort happens BEFORE a
decode error arrives, the outcome is still `Failed{decode}` — not
`Failed{device}` — because decode_failure is checked first in
resolve(), regardless of temporal ordering. The stage identifies the
failure class selected by the resolver's semantic precedence, not the
chronological first failure.

## H6 — Play → device abort × stop collision

```text
COMMAND:           play <file>, then stop (racing device abort)
MECHANISM EVIDENCE: Both paths land on identical edge terminal.
                   resolve() checks stop_requested as discriminator.
SEMANTIC DECISION:  resolve() — Aborted + Stopped + stop_requested → Stopped
                   (stop intent after the abort still produces Stopped —
                   see §7 for the full cause-loss hole)
FACT COMMIT:        SessionOutcome::Stopped
PROJECTION UPDATE:  headless prints "stopped before completion"
```

**Single writer:** resolve(). No MAJOR — but this is the cause-loss
case: if the device abort was independent of the stop, the outcome is
still Stopped because stop_requested was true. The narrowed proposition
("stop intent observed before resolution") does not claim causality,
so this is a LIMITATION, not an architecture defect.

## H7 — Play → activation failure

```text
COMMAND:           play <file>
MECHANISM EVIDENCE: activate_inner() returns Err, activation_failed()
                   called, FiberState::Failed in K0 snapshot
SEMANTIC DECISION:  No SessionOutcome committed (activation failed
                   before any episode started)
FACT COMMIT:        activation_failure (mechanism evidence, write-once)
                   subject: this activation attempt, not
                   "current playback episode" (no episode exists yet)
PROJECTION UPDATE:  headless prints error
```

**Single writer:** activation function. No MAJOR.

## H8 — Terminal committed → late stop

```text
COMMAND:           play <file>, outcome resolved, then stop
MECHANISM EVIDENCE: request_stop() — but outcome is already memoized
SEMANTIC DECISION:  resolve() — outcome already Some, returns immediately
FACT COMMIT:        No change (memoized)
PROJECTION UPDATE:  No change
```

**Single writer:** resolve(). No MAJOR.

---

# 12. Formalization Triage

**Question:** Are there two or more independent legal writers/events
whose interleaving could produce conflicting truth for the same
semantic fact?

**Analysis:**

The resolve() function in completion.rs is the SINGLE writer of
`CompletionState.outcome`. All other writes to `CompletionState` are
to distinct fields:

- `decode_failure` — written by decode_worker only (first wins)
- `worker_terminal` — written by worker_exited only
- `stop_requested` — written by request_stop only
- `stop_target` — written by bind_stop_target only
- `source_format` — written by set_source_format only
- `activation_failure` — written by activation_failed only

The critical question: can two writers produce conflicting evidence
that resolve() interprets differently depending on interleaving?

**Stop × EOF:** If EOF commits on the edge before stop arrives, the
worker exits with Eof terminal. resolve() sees DrainVerdict::Drained
+ Eof → Completed. The late stop is a no-op. If stop arrives before
EOF commits, the edge terminal is Stopped (first-wins), the worker
exits with Stopped, and resolve() sees Stopped. Both interleavings
produce correct outcomes.

**Stop × decode failure:** decode_failure is checked first in resolve().
If decode failure is published before stop resolves, Failed wins. If
stop resolves first but decode failure was already published (the
decode worker calls decode_failed() before edge.fail()), Failed still
wins because decode_failure is checked first.

**Stop × device abort:** Both land on identical mechanism evidence.
The discriminator (stop_requested) resolves correctly because
request_stop() publishes intent before releasing the edge.

**Conclusion:** No independently-legal interleaving produces conflicting
truth for the same (fact kind, subject scope). resolve() is the single
authority with deterministic precedence.

**FORMALIZATION = NOT EARNED.**

The existing loom suite (`loom_edge.rs`) already covers the edge-level
interleavings. No new Loom/TLA target is needed for F2.

---

# 13. Plugin Decision

```text
NEW PLUGIN = NO
WHY:
    Playback Session is a Component / episode ownership unit (PBK-002
    §8). It does not earn Plugin status because:
    1. It has no independent long-lived capability outside one episode.
    2. It does not provide/require stable capabilities across episodes.
    3. It does not need independent replacement/withdrawal.
    4. Its lifetime is exactly one episode.

    The Decode Plugin and Output Plugin remain the only Plugins.
    SessionCompletion is not a Plugin — it is a session-owned seam.
```

---

# 14. New Capability

```text
NO / EARNED
WHY:
    No new capability is needed for F2. The existing PcmDecodeCapability
    and AudioOutputCapability cover the mechanism seams. The playback
    semantic authority (SessionCompletion/resolve()) does not need its
    own Capability — it is a component-owned seam, not a cross-
    composition dependency.
```

---

# 15. Episode ID / Generation

```text
NOT EARNED
WHY:
    The current architecture is single-episode. One SessionCompletion
    per episode, consume-once. There is no cross-episode ambiguity:
    the old episode's completion is gone before a new one starts.

    F2 does not need EpisodeId or Generation because:
    1. No stale observation can be mistaken for current episode truth.
    2. The App creates a new SessionCompletion for each episode.
    3. K0's composition snapshot shows FiberState per fiber name, which
       disambiguates at the composition level.

    If multi-session topology is ever needed (seek, next, gapless),
    EpisodeId/Generation may be re-evaluated. Not now.
```

---

# 16. ADR Amendment

```text
ADR CORRECTIVE REQUIRED

Reasoning:
    The report proposes that the Playback Session be the designated
    semantic authority for episode terminal outcome (Completed/Stopped/
    Failed). PBK-002 §14 leaves PlaybackFacts OPEN. Freezing a
    designated authority for (episode outcome, episode scope) is a
    durable playback authority decision that requires explicit
    ADR-level designation.

    The candidate designation:
    "For the current one-episode playback topology, the Playback
    Session is the designated semantic authority for that episode's
    terminal outcome."

    Current realization: SessionCompletion::resolve().

    Mechanism evidence may inform that decision, but does not itself
    establish playback outcome truth.

    This report does NOT directly edit any accepted ADR. It proposes
    the minimum amendment for later review.

SCOPE OF PROPOSED AMENDMENT:
    - Playback Session designated semantic authority for episode
      terminal outcome (one-episode topology only)
    - Current outcomes: Completed, Stopped, Failed
    - Explicitly OPEN: Playing, Starting, Paused, Position, Seek,
      cross-episode source truth, playlist, multi-session,
      EpisodeId/Generation, PlaybackControl topology, PlaybackSnapshot
      representation

If this ADR corrective is not accepted in this PR:
    F2 implementation remains blocked on the authority designation.
```

---

# 17. Majors / Minors / FYI

```text
MAJORS:  (none — all 4 MAJORs from reviewer are corrected in this
          version)

MINORS:  (none — all 4 MINORs from reviewer are corrected in this
          version)

FYI:
    1. The cause-loss hole (§7) is a known LIMITATION, not a defect,
       for the current narrowed Stopped proposition. Becomes an
       AUTHORITY_GAP only if future product requires causal truth.
    2. SessionCompletion consume-once is documented but unenforced.
       Latent for future retry/restart.
    3. The zero-frame decode branch loops hot (pre-existing; the
       edge-terminal check keeps it stoppable).
    4. A wedged device call has no Host-side escape (no timeout,
       no signal handling).
```

---

# 18. Next Implementable F2 Slice

After the corrected authority analysis, the next F2 slice must be
re-derived. The previous proposal (PlaybackSnapshot on SessionCompletion
with source field + K0 activation field) does not survive review because
SessionCompletion cannot honestly synthesize fields it does not own.

**Recommended next slice:**

1. **ADR corrective proposal** — designate Playback Session as semantic
   authority for episode terminal outcome (Completed/Stopped/Failed).
   This is the gate that unblocks F2 implementation.

2. **Terminal outcome read-side projection only** — if ADR corrective
   is accepted, the smallest implementable slice is:
   - `SessionOutcome` read-side accessor (already exists as
     `try_resolve_now()` / `wait()`)
   - Headless `status` command printing outcome + source_format +
     stop_requested + activation_error
   - No PlaybackSnapshot type yet (projection assembly across
     App/K0/session boundaries needs clearer ownership)

3. **No Playing/Starting/current-source claims** — these are NOT EARNED
   and must not be fabricated.

**What is NOT authorized yet:**

```text
PlaybackSnapshot type (cross-boundary assembly unclear)
Playing / Starting / Paused / Stopped-as-state projections
Source path as semantic fact
status command as control-correctness authority
New Plugin / K0 primitive / capability
```

---

# 19. Final Verdict

```text
F2-AUTHORITY-AUDIT-CORRECTIVE-1

BASE:         8cf337e (main)
OLD_HEAD:     a36b9c3
BRANCH:       specs/f2-authority-audit-0

VERDICT:      ADR_CORRECTIVE_REQUIRED

SEMANTIC FACT CANDIDATES (ADR designation required):
    SessionOutcome (Completed/Stopped/Failed) — candidate semantic
    fact, current implementation writer = resolve() in completion.rs,
    episode-scoped, memoized, single-writer, conflict-free.
    Architecturally designated: NOT YET (ADR corrective required).

MECHANISM / DIAGNOSTIC EVIDENCE (not semantic-fact candidates;
no authority designation required — writer != semantic authority):
    SourceFormat — mechanism-derived write-once observation, within
    episode. May exist even if later activation steps fail.

    ActivationError — mechanism evidence, write-once. Subject: this
    activation attempt, not "current playback episode."

NOT EARNED:
    Playing — no designated authority, no mechanism publishes this.
    Starting — derivation is semantically false (Active + no terminal
        is true for whole episode, not just the start).
    Paused/Seeking — not implemented.
    Stopping — not an independent fact, derivable projection.
    Source identity — NOT EARNED; App holds PathBuf as input ownership.
    EpisodeId/Generation — not needed for single-episode architecture.
    Cross-episode "current source" — not earned.

AUTHORITY MODEL:
    current implementation writer:
        resolve() in completion.rs (episode outcome)
        session activation (source_format, activation_failure)
    candidate designated authority:
        Playback Session for episode terminal outcome
    architecturally designated authority:
        NOT YET for any playback fact

SOURCE VERDICT:
    NOT EARNED — no session-owned source identity fact exists.
    App holds PathBuf as input/request ownership.

SOURCE_FORMAT VERDICT:
    MECHANISM EVIDENCE — mechanism-derived write-once observation.
    May exist even if activation later fails.
    Do NOT conflate with source identity. NOT_APPLICABLE to ADR
    authority designation.

STARTING VERDICT:
    NOT EARNED — derivation is semantically false.
    Active + no terminal is true for the whole active episode.

STOPPED PROPOSITION:
    Narrow classification: "stop intent observed before semantic
    resolution of aborted episode."
    NOT causal: does not claim "user's stop caused termination."

CAUSE-LOSS STATUS:
    LIMITATION — the narrowed proposition does not require causal
    truth. Becomes AUTHORITY_GAP only if future product demands
    causal semantics.

FAILED PRECEDENCE:
    Stage identifies failure class by resolver semantic precedence,
    NOT chronological first-failure. Device abort can occur first
    in wall-clock time, yet published decode failure still dominates.

COMPLETED PROPOSITION:
    EOF + output drain contract completed + resolver committed.
    NOT physical audibility. Contract-level wording.

ACTIVATION FAILURE SUBJECT:
    This activation attempt (not "current playback episode").
    Activation failure occurs before a successfully activated
    episode exists.

PROJECTION ASSEMBLY:
    SessionCompletion can only expose fields it owns: outcome,
    source_format, activation_failure, stop_requested.
    Source path and K0 state must be assembled at App level.
    Separate projection assembly ownership from authority ownership.

ADR:
    REQUIRED
    Scope: designate Playback Session as semantic authority for
    episode terminal outcome (one-episode topology).
    Explicitly OPEN: Playing, Starting, Paused, Position, Seek,
    source truth, playlist, multi-session, PlaybackSnapshot.

FORMALIZATION:
    NOT EARNED — no independently-legal interleaving produces
    conflicting truth.

NEXT IMPLEMENTABLE F2 SLICE:
    1. ADR corrective proposal (authority designation gate)
    2. Terminal outcome read-side projection + headless status
    3. No Playing/Starting/source claims
    4. No PlaybackSnapshot cross-boundary synthesis

PRODUCTION CODE CHANGED:
    NO

UNRESOLVED MAJORS:
    (none)

WORKTREE:
    clean
```

---

# Appendix A — Current resolve() Precedence Observed in Production

> This is production reality / evidence. It becomes normative only if
> separately accepted through architecture authority.

The following precedence rule is the current implementation's decision
function, observed in `resolve()` in completion.rs:

```text
1. if outcome already committed → return it (memoized, first-wins)
2. if decode_failure exists → Failed{stage: "decode: {message}"}
3. if worker_terminal == Failed → Failed{stage: "decode"}
4. if DrainVerdict::Drained AND worker_terminal == Eof → Completed
5. if DrainVerdict::Aborted:
   5a. if worker_terminal is None → wait (not yet decidable)
   5b. if worker_terminal == Stopped AND stop_requested → Stopped
   5c. otherwise → Failed{stage: "device"}
6. otherwise → not yet decidable (wait)
```

This rule is the current implementation property. It is a one-shot
resolution function that observes mechanism evidence and commits a
semantic fact.

Key properties (current implementation, not frozen architecture):
- **Decode failure dominates** everything downstream (checked first).
- **Precedence, not chronological first-failure**: a device abort can
  occur first in wall-clock time, yet a published decode failure still
  dominates semantic resolution.
- **First-wins memoization**: once committed, the outcome never changes.
- **stop_requested is command state** used as a discriminator, not
  cause-carrying evidence.

If an ADR corrective accepts the Playback Session as the designated
semantic authority for episode outcome, this precedence rule would be
the current realization of that authority's decision function. It
would then become normative through the ADR, not through this report.

---

# Appendix B — Vocabulary for F2

All terms below are design input, not frozen vocabulary:

| Term | Class | Meaning |
|---|---|---|
| Episode | Subject scope | One playback lifetime: activation → terminal |
| Episode outcome | Semantic fact (candidate) | How the episode ended (Completed/Stopped/Failed) |
| Mechanism evidence | Input to authority | Raw observations from decode/output mechanisms |
| Semantic commit | Authority action | Designated authority establishes truth |
| Derived projection | Read-side visibility | Snapshot of current authority state for display |
| Command state | Intent | Whether an intent was recorded (stop_requested) |
| Current implementation writer | Production property | The code that currently writes a fact |
| Candidate designated authority | Architecture proposal | The semantic role that could become designated authority |
| Architecturally designated authority | Architecture decision | Explicit ADR-level designation (NOT YET for any playback fact) |
| Designated authority | Architecture role | Single writer for a (fact kind, subject scope) — requires ADR |
| Resolve | Implementation function | The current function that commits semantic truth — candidate authority realization |

---

# Appendix C — Fact Authority Matrix (Corrected)

Columns: PROPOSITION | SUBJECT SCOPE | CURRENT PRODUCER/WRITER | CLASS | CANDIDATE ARCHITECTURAL AUTHORITY | ARCHITECTURALLY DESIGNATED? | COMMIT POINT | POST-COMMIT MUTABILITY | KNOWN AMBIGUITY | VERDICT

| Proposition | Subject scope | Current writer | Class | Candidate authority | Designated? | Commit point | Post-commit | Ambiguity | Verdict |
|---|---|---|---|---|---|---|---|---|---|
| Episode completed (EOF + drain) | Episode | resolve() | semantic fact (candidate) | Playback Session | ADR_REQUIRED | resolve() first produces Completed | immutable (memoized) | none | CANDIDATE |
| Episode stopped (abort + stop intent) | Episode | resolve() | semantic fact (candidate) | Playback Session | ADR_REQUIRED | resolve() first produces Stopped | immutable (memoized) | cause-loss: stop intent after abort still produces Stopped | CANDIDATE (LIMITATION) |
| Episode failed | Episode | resolve() | semantic fact (candidate) | Playback Session | ADR_REQUIRED | resolve() first produces Failed | immutable (memoized) | stage = precedence class, not chronological first | CANDIDATE |
| Source format of opened endpoint | Episode | set_source_format() | mechanism evidence (write-once) | N/A | NOT_APPLICABLE | set_source_format() in activate_inner | immutable (write-once) | may exist if later activation fails | MECHANISM EVIDENCE |
| Activation failure | Activation attempt | activation_failed() | mechanism evidence (write-once) | N/A | NOT_APPLICABLE | activation_failed() | immutable (write-once) | subject = activation attempt, not episode | MECHANISM EVIDENCE |
| Source identity | Episode | NONE | NOT EARNED | NONE | NOT EARNED | N/A | N/A | App holds PathBuf, not session authority | NOT EARNED |
| Playing | Episode | NONE | NOT EARNED | NONE | NOT EARNED | N/A | N/A | no mechanism publishes this | NOT EARNED |
| Starting | Episode | NONE | NOT EARNED | NONE | NOT EARNED | N/A | N/A | derivation is semantically false | NOT EARNED |
| Stopping | Episode | NONE | NOT EARNED (projection) | NONE | NOT EARNED | N/A | N/A | derivable from stop_requested + no terminal | NOT EARNED |
| Paused | Episode | NONE | NOT EARNED | NONE | NOT EARNED | N/A | N/A | not implemented | NOT EARNED |
| Buffered frames | Edge | edge read/write | mechanism evidence (diagnostic) | NONE | NOT EARNED | N/A | mutable | ring occupancy only | RETIRED from snapshot |
| Stop requested | Episode | request_stop() | command state | NONE | NOT EARNED | request_stop() | mutable (set once) | command, not outcome | COMMAND STATE |
