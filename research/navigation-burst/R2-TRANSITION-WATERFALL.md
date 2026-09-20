# R2 — TRANSITION WATERFALL (one manual N vs one natural EOF)

Campaign: QIANQIAN-NAVIGATION-BURST-ROOT-CAUSE-AND-BOUNDARY-0
Base: `ad4e09dc9064077b97cc6f26dd9139d81dae6e99`

## Verdict class (§42)

```text
MANUAL N (single, warm):  DECODER_START + OUTPUT_OPEN dominant
                          (start phase ≈ 81 of 92 ms; old settlement 33 ms,
                          overlapping phases inside the synchronous sequence)
NATURAL EOF:              OLD-TAIL DRAIN inherent (Completed waits for the whole
                          device tail: drain_to_zero, by frozen D11/D14.4
                          semantics — "EOF must be audible")
BURST (NNNNN …):          CONTROL CHURN DOMINANT — R1's RC-A: N × (one full
                          replacement) serialized on the shell thread
```

## 1. Instruments

- **I2 — primitive waterfall (Linux, `crates/qianqian-playback/tests/
  navigation_waterfall.rs`):** the REAL session + real kernel + real
  edge + real legs on mechanism doubles. Medians, n=15 (structure-only
  assertions; timings are observations, not pins). Device-dependent
  numbers here are explicitly SIMULATED (instant or paced device).
- **Real-host decomposition (Windows):** single-N 85–92 ms total
  (solid; two scenarios) with one single-sample phase split the
  existing mechanism log provides (old-leg abort ≈ +33 ms, new device
  open ≈ +81 ms — ConPTY chunk granularity; treat as indicative, R1
  §5); prior physical evidence `experiments/f6-open-smoke/RESULTS.md`:
  replacement wall 52/51/51 ms (same seam, warm) and device-open→
  first-position publication ~0.5 s on that host.
- Evidence files: `evidence/waterfall-linux.txt`,
  `evidence/run1-B-manual-short.markers.txt`,
  `evidence/run*-B-eof-natural.*`.

## 2. Manual N waterfall (T0 = raw key, all numbers warm)

```text
T0   key consumed by shell loop
     probe (SourceFacts query; Linux synth: ~0.0002 ms/op; SongCore probe
       real-media cost is inside the phases below since the probesize cap)
T1   OLD stop requested (request_stop → edge.stop)
T3   old render leg observes PcmPull::Stopped, exits
       real host: ≈ +33 ms (single sample, chunk-granular);  Linux
       empty edge: 0.03 ms;
       Linux full edge (8192 frames buffered): 20.0 ms — bounded by the
       device leg's CURRENT read slice (the analog real bound is the
       device period / EVENT_TIMEOUT, wasapi.rs:91)
T6   old terminal Stopped committed synchronously at the publication
     boundary (completion.rs:1136) — no drain wait for a manual stop
T7   old root Discharged (K0 teardown; joins both legs)
       Linux: 0.02–0.06 ms; included in the real-host phase figures
T8   fresh root constructed; decode open_media (decoder startup)
T9   render stream open (device open + format negotiation / engine SRC)
       — T8+T9 measured together on the real host: ≈ +81 ms from key
       (single sample; the TOTAL 85–92 ms is the measured quantity)
T10  worker spawned; activation evidence published (source_format)
T11  first submission → the D14.6 commit is already observable here
       Linux first_submit after activate: 0.27 ms; real host: the
       commit render lands at +85–92 ms
T12  first device-consumed position sample
       real host: leg takes ~0.5 s from device open to first position
       publication (f6-open-smoke finding 2) — position PROJECTION
       latency, not audible-start latency; the device buffer is 970
       frames ≈ 22 ms
```

Total: **≈ 85–92 ms** per single manual transition on the real host
(warm). Phase shares: old settlement ~1/3 of the window but
overlapped-in-sequence; the **start phase (decoder open + device open +
activation) is the dominant single block (~81 ms of 92)**. NOT "PCM
queue": the edge tail and device padding are abandoned/dropped on a
manual stop, not waited on.

## 3. Natural EOF waterfall (Track A reaches its real end)

```text
T9'  decode worker hits Eof → edge.close_eof() (session.rs:460-462)
T10' render leg drains the REMAINING EDGE (buffered frames become audible)
T11' drain_to_zero (wasapi.rs:751): waits until GetCurrentPadding == 0 —
     the already-handed-off tail PLAYS OUT; only then Drained
T12' resolve commits Completed (D11); ONLY NOW does the App's
     poll_eof_policy see the Completed Fact and run the SAME
     replace_episode to B
```

Linux structural measurement (device paced ~4.9× real time;
`evidence/waterfall-linux.txt`): activate 0.22 ms | first_submit
0.28 ms | produce_to_eof 322 ms | **drain_to_Completed 4.9 ms** after
the last produced frame was submitted, with an instant-tail device —
i.e. the drain wait itself is cheap; on the real host the drain lasts
exactly the already-submitted tail (≤ device buffer ≈ 22 ms + engine
tail). Real-host witness: auto-next on the 5 s track committed at
T+5.14 s from spawn (≈ content length + startup + drain + one
replacement).

**EOF vs manual N: the current implementation treats them
DIFFERENTLY, and by design** — natural EOF drains the old tail to
audible completion before advancing (D11/D14.4 frozen semantics); a
manual N abandons the old tail immediately. The campaign's suggested
product model ("EOF drains, manual supersedes") is in fact what the
mechanism already does. No behavior change is indicated by this
measurement.

## 4. What would a manual "fast cut" buy? (§26/§30 — answered, not implemented)

- The old side of a manual replacement does NOT wait for PCM tail or
  device padding today (abandoned at edge.stop; client released). The
  candidate "discard old not-yet-audible queued data" optimization is
  ALREADY the de-facto mechanism for manual navigation.
- The frozen §29 PCM cutover invariant ("after the new committed
  episode begins contributing, no retired-episode PCM contributes
  afterwards") is guaranteed STRUCTURALLY by whole-root replacement:
  the new episode's edge, stream and device session are new objects;
  the old root is fully discharged (both legs joined) before the new
  root is constructed (D14.6 no-overlap). There is no shared buffer
  through which old PCM could outlive its episode.

## 5. Conclusion

- One genuine manual transition costs ≈ one device open + decoder open
  + K0 activation ≈ 60–92 ms warm. Probe and old-side settlement are
  minor. PCM/edge/padding are NOT the cost for manual navigation.
- A burst of K raw keys costs ≈ K × that, serialized — plus K−1
  audible intermediate episodes. This — not any single-transition
  inefficiency — is what the listener experiences as stalls/silence
  (RC-A).
- Any future latency work on single transitions should target the
  start phase (decode open + device open); it was NOT the object of
  this campaign and nothing here implements it.
