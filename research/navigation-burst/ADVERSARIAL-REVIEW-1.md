# ADVERSARIAL-REVIEW-1 — fresh-context review (campaign §51)

Reviewer: an independent fresh-context agent (no shared conversation
state with the campaign author), instructed to attack the deliverables
and verify against source and evidence rather than trust the reports.
It re-ran I1 and I2 locally and re-derived the count columns.

## Twenty-question verdicts (§51)

All twenty questions PASS. Highlights of the verified evidence:

1. NNNNN replacement count proven at both layers (I1: 4 probes/4
   starts/4 retirements with the 5th key inert; I3: 5 measured = 5
   expected at 10/50/150/250 ms, intermediate committed frames
   present in the transcripts).
2. OS/ConPTY input queue (Q1) explicitly distinguished from
   Qianqian-owned pending navigation (Q2: DOES NOT EXIST).
3. Q1–Q7 inventoried separately; each verified against source
   (edge 8192 frames ≈185 ms; device buffer 970 frames ≈22 ms;
   zero-entries Q2/Q5 part of the inventory; Q7 NOT OBSERVABLE).
4. One transition decomposed (Linux n=15 medians reproduced by the
   reviewer; real-host single-sample phase split disclosed).
5. Natural EOF vs manual N compared (Linux waterfall + matched
   B-eof-natural / B-manual-short pair; drain_to_zero semantics
   verified against wasapi.rs).
6. PCM blamed only where measured (edge.rs abandon-on-stop verified;
   drain waited on for EOF only).
7–8. Coalescer UI-neutral (TUI debounce rejected); future GUI reuses
   the intent layer with the explicit-target bypass.
9. Shuffle traversal preserved THROUGH the playlist pure-preview
   requirement; prototype's local replication is Sequential-only and
   DISCLOSED (implementation review must exercise Shuffle).
10. NP zero-Open outcome achieved (w > gap strictly; boundary noted).
11. Manual-pending vs EOF precedence specified; grounded in the
    existing exactly-once eof_consumed discipline; D11 untouched.
12. Pending state kept out of playback Facts and kept distinct from
    both committed `playing` and presentation-only `selected`; any UI
    indication of a pending target is a separate projection/affordance,
    not a reinterpretation of the existing `>` selection marker.
13–14. D13 applied per-candidate (existing owner / lifecycle /
    correctness-lost questions); NO Plugin earned.
15–16. No async/cancellation machinery; the App thread stays the
    single serializer; intermediate targets stay control-plane.
17–19. Encoded source-byte cache primary with size math; networking
    NOT implemented; cache/plugin admission deferred and reasoned
    from lifecycle ownership.
20. Minimal delta: no D14.6 amendment; PCM_CUTOVER_CHANGE NOT_EARNED
    (verified structural under whole-root replacement); production
    diff strictly test-only (+168 lines inside the test module).

## Findings (pre-fix)

```text
REQUIRED  ADVERSARIAL-REVIEW-1.md referenced by README/R3 but absent
          (fixed: this file IS that deliverable)
MAJOR     R3 "NP with any w ≥ gap" overclaimed the boundary (artifact
          shows w == gap does NOT collapse); corrected to w > gap
MAJOR     "≈42–63 ms serialized period" presented without its basis
          (n=3 frame-timestamp periods, ConPTY-quantized) and
          unreconciled with the 85–92 ms single total; corrected in
          R1 §5 / README (TOTALs are the measured quantities; ≈63 ms
          is the burst AVERAGE implied by 317/5; per-period figures
          marked quantized)
MAJOR     Real-host +33/+81/+92 phase split is a single sample at
          chunk granularity; now disclosed in R1 §5 / R2
MAJOR     R1 §2 Wall-ms column did not match its archived artifact;
          table regenerated verbatim from burst-matrix-linux.txt with
          a run-noise caveat
MAJOR     R4 "~30–100×" PCM-vs-encoded ratio failed arithmetic
          against its own premises; corrected to ~15× (192 kbit/s) …
          ~175× (16 kbit/s)
```

Not classified (minor, no action forced): RC-B's real-host probe share
is an estimate (RC-F discloses the 81 ms lump); R2's probe figure is
synthetic-provider open/close, labeled as such; the prototype's
Shuffle gap is disclosed in its own header; R0 line citations off by
≤2 lines.

## Post-fix status

```text
Critical = 0
Required = 0
Major    = 0
```

(every finding above is addressed in the report set; the reviewer's
full reply is preserved verbatim in the campaign session record)
