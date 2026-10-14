# Measurement oracle validity: instrument controls before adjudication

> **Status:** Development / test-measurement guide (NON-NORMATIVE for product behavior).
> **Origin:** the [#233](https://github.com/jnhu76/qianqian/issues/233) closeout
> (2026-10-14, PR [#276](https://github.com/jnhu76/qianqian/pull/276)), where a
> round-1 cadence measurement was refuted by controlled round-2 instruments.
>
> This document explains how to validate a measurement **instrument** before its
> readings are used to adjudicate an architecture or performance question. It
> defines no product semantics, no new runtime noun, and no new verification
> authority. The evidence classes (mechanism evidence vs semantic truth) remain
> owned by [ADR-PBK-001](../adr/ADR-PBK-001.md) §2 and the
> [AGENTS.md verification boundary](../../AGENTS.md#verification-authority-boundary).

## Scope and application

Use this guide when a measurement — a rate, cadence, latency, throughput, frame
change count, resource observation — is about to be used to accept, reject, or
change something: an issue verdict, a performance claim, an architecture
decision. Numbers are only as trustworthy as the instrument's demonstrated
ability to see the signal they claim to describe.

A measurement result that contradicts an implementation-level fact (a configured
tick, a known code path, a mechanical guarantee) is a signal to re-validate the
instrument **before** any architecture action. The contradiction itself does not
say which side is wrong.

## The rule

An instrument may adjudicate only after its validity has been established on the
same surface, in the same session, and recorded with the evidence:

| control | question it answers | example |
|---|---|---|
| **Positive** | Can it see a known-rate signal of the same kind, through the same path? | a 30 Hz frame writer measured at ~27.5 changes/s; a 1 Hz position line measured at 1.00/s |
| **Negative** | Does it stay quiet on static or known-slow content? | a static screen reads ~0 changes/s; the detector does not fabricate motion |
| **Limits** | Which surfaces does it demonstrably NOT see? | a desktop capture path that returns byte-static frames for a DirectComposition window while the window is updating |

Additional rules:

- A differential claim (A vs B) rests on **same-method pairs**: both sides
  measured by the same instrument and method. Two independent instruments
  agreeing strengthens the claim; a disagreement must be explained before the
  differential is used — the two numbers may be counting different frame
  streams or running under different conditions.
- "The instrument saw nothing" is evidence about the **instrument** until its
  controls pass — not about the measured system.
- Archive the instrument (code + raw output) with the evidence. An unarchived
  artifact cannot be re-examined when its figure is challenged later.
- Precision is not validity: a tight p50 from an unvalidated instrument is
  still not evidence.

## Case record: #233 Visualizer cadence — a Measurement Oracle Failure

**Round 1** concluded from on-screen capture that the 150 ms build and the
50 ms build showed the *same* ~1 Hz visible cadence on real content, and that
"the visible cadence is content/quantization-limited, not tick-limited"
(verdict: NOT_READY).

**Round 2** re-measured with validated instruments:

~~~text
instrument           validation                                        result
PrintWindow burst    positive: 30 Hz writer -> ~27.5 changes/s;        e881152: 16.06/s (noise),
                     negative: 1 Hz position line -> 1.00/s            16.72/s (ballad)
gdigrab desktop      control refuted it for Windows Terminal           secondary instrument only;
video                content: byte-static while the app updated        never the sole evidence
diag builds          direct loop decomposition: poll p50 62.5 ms,      draw p50 0.38 ms
                     draw p50 0.38 ms
~~~

Measured differential (same method, same environment): 16.06/s vs 6.35-6.45/s
(noise) and 16.72/s vs 7.43/s (ballad) — a 2.3-2.5x increase. The round-1
~1 Hz figure was not reproducible on the same content; its detection code was
session-local and is not archived, so the artifact's exact cause is
unidentified — but the figure is inconsistent with the content's measured
motion (11k-44k changed pixels per event).

**Why round 1 went wrong:** the coarse change detector was never validated
against a known-rate signal, its readings were used to adjudicate (a NOT_READY
verdict plus a "tick is not the bottleneck" conclusion), and its artifact was
not archived. Re-validation cost one measurement round and flipped the
conclusion.

## Review prompt

For any evidence record that uses measurement to adjudicate:

~~~text
Which instrument produced this number, and where are its positive/negative
controls and recorded limits?
Are both sides of a comparison measured by the same method?
Which surfaces is the instrument known to miss, and does any conclusion
rely on them?
Is the instrument (code + raw output) archived with the evidence?
Does the result contradict an implementation-level fact - and if so, why is
the instrument believed over the fact?
~~~

## Boundary

This is a test/measurement-authoring method. It does not promote any
measurement into semantic authority, does not define product behavior, and does
not change the verification authority boundary in
[AGENTS.md](../../AGENTS.md#verification-authority-boundary).
