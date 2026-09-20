# R1 — BURST EVIDENCE

Campaign: QIANQIAN-NAVIGATION-BURST-ROOT-CAUSE-AND-BOUNDARY-0
Base: `ad4e09dc9064077b97cc6f26dd9139d81dae6e99`

## Verdict (required, §41)

```text
BURST_FULL_REPLACEMENT_CONFIRMED
```

Every raw, non-inert N/P key event executes exactly one complete D14.6
replacement (probe → old-episode retirement → fresh episode with a real
decoder open and a real device open), on both the App layer and the real
Windows product, at every input cadence tested. There is no coalescing,
no pending state, and no zero-Open collapse (`NP` costs two full
replacements even though its net target is the track already playing).

## 1. Reproducer protocol

Two independent instruments, two hosts, one code path:

- **I1 — App-layer count matrix (Linux, deterministic).** A test-only
  matrix (`apps/headless/src/player.rs` tests,
  `burst_matrix_one_full_replacement_per_raw_manual_step`) drives the
  REAL `ReferencePlayerApp` (real K0 kernel, real Playback Session,
  real bounded edge, real worker/render threads; mechanism doubles at
  the decode/output ports) through R1–R12 with one
  `next_track()/previous_track()` call per raw key — the exact function
  the shell's `handle_key` invokes. Counts come from the kernel-mediated
  event log (probe / activate / teardown lines), not from assumptions.
  Evidence: `evidence/burst-matrix-linux.txt`.
- **I3 — Real-product burst matrix (Windows host, ConPTY).** The
  existing transport-dogfood harness, extended additively with burst
  scenarios (B-*) and timestamped marker capture (key injections +
  the EXISTING off-by-default `QIANQIAN_AUDIO_LOG=1` per-episode
  `[qianqian-wasapi] opened:` mechanism lines). Eight 45 s synthetic
  tracks (vtest01–08); the product binary is the current tree
  cross-built `--features playback` (sha256 in
  `evidence/ENV-BURST-RUN*.txt`). Run 1 = timing round (audio log on);
  run 2 = clean count round (audio log off — its stderr interleaving
  pollutes grid rows; see observer-overhead note below).
  Evidence: `evidence/run*-B-*.{json,txt,markers.txt,raw.txt}`,
  `evidence/burst-counts-windows.txt`.

Input cadences (§8): 0–20 ms band ≈ 10 ms (KeysEach gap 10), plus 50,
150, 250 ms. No human input in the count experiment.

## 2. Count table — App layer (I1, Linux; every sequence starts at the
committed middle of an 8-track Sequential / Repeat-Off playlist;
figure transcribed verbatim from `evidence/burst-matrix-linux.txt`,
the archived run)

| Input | Raw keys | Opened | Probe | Episode starts | Old retirements | Final track | Wall ms |
|---|---:|---:|---:|---:|---:|---|---:|
| N | 1 | 1 | 1 | 1 | 1 | 5/8 | 0.3 |
| NN | 2 | 2 | 2 | 2 | 2 | 6/8 | 0.6 |
| NNN | 3 | 3 | 3 | 3 | 3 | 7/8 | 0.7 |
| NNNNN | 5 | 4 | 4 | 4 | 4 | 8/8 (5th inert at end) | 0.9 |
| P | 1 | 1 | 1 | 1 | 1 | 3/8 | 0.3 |
| PPPP | 4 | 3 | 3 | 3 | 3 | 1/8 (4th inert at start) | 0.6 |
| NNPP | 4 | 4 | 4 | 4 | 4 | 4/8 | 0.9 |
| NNNPPNNP | 8 | 8 | 8 | 8 | 8 | 6/8 | 1.8 |
| NPNPNP | 6 | 6 | 6 | 6 | 6 | 4/8 | 1.3 |
| NP | 2 | 2 | 2 | 2 | 2 | 4/8 (= start; still 2 replacements) | 0.4 |
| NNP | 3 | 3 | 3 | 3 | 3 | 5/8 | 0.7 |
| NNPPNNPP | 8 | 8 | 8 | 8 | 8 | 4/8 | 2.4 |

(The Wall column is this stack's device-free run-to-run noise at the
0.1–0.3 ms scale; the COUNT columns are the finding and are asserted
executably in the instrument.)

Per-key event-log shape (every replacement, frozen order):
`probe <next>` → `teardown <old> decode/output` → `activate <new>
decode/output`. Decoder activations = episode starts (the decode
endpoint opens inside session activation); output opens = episode
starts (the render stream opens inside the same activation).

## 3. Count table — real Windows product (I3 run 2, clean grid)

Committed replacements counted as Track-row value changes in the
timestamped reconstructed frames (immune to ConPTY repaint duplication;
mid-write polluted frames skip, never invent). Expectations = the
traversal policy applied to the scripted keys.

| Scenario | Keys (cadence) | Replacements measured | Expected | Final track | First key → final commit |
|---|---|---:|---:|---|---:|
| B-single-N | 1 N | 1 | 1 | 2/8 | **85 ms** |
| B-nnnnn-10ms | 5 N @10 ms | **5** | 5 | 6/8 | **317 ms** |
| B-nnnnn-50ms | 5 N @50 ms | **5** | 5 | 6/8 | 328 ms |
| B-nnnnn-150ms | 5 N @150 ms | **5** | 5 | 6/8 | 661 ms |
| B-nnnnn-250ms | 5 N @250 ms | **5** | 5 | 6/8 | 1066 ms |
| B-pppp-10ms | 4 setup N + 4 P @10 ms | **8** | 8 | 1/8 (4th P inert) | 247 ms (burst part) |
| B-nnnppnnp-10ms | 8 @10 ms | **8** | 8 | 3/8 | **137 ms** |
| B-eof-natural | (natural EOF) | 1 | 1 | 2/2 | commit at T+5.14 s |
| B-manual-short | 1 N | 1 | 1 | 2/2 | **92 ms** |

All 9 scenarios GREEN (`burst-run*.summary`); replacement counts match
the traversal policy exactly — **one full replacement per raw key at
every cadence**. Intermediate episodes are not control-only: each one
reaches probe + decoder open + PCM + device open (the per-episode
`[qianqian-wasapi] opened:` marker; run 1) and commits (`Track: n/8`
frames, e.g. run2 B-nnnnn-10ms shows committed intermediate frames
2/8 → 3/8 → 4/8 → 5/8 → 6/8).

`NNNPPNNP` at 10 ms: 8 keys, net displacement +2, cost **8 full
episode replacements, all committed within 137 ms of the first key** —
the user hears up to 7 intermediate-track blips/silence gaps inside
that window, then the final target. This is the reported field
defect, mechanically.

## 4. Key-queue hypothesis (§10)

- **Queue owner (Q1):** the OS-side Windows console / ConPTY input
  buffer. With the shell thread inside a replacement (~60–90 ms warm),
  keys injected at 10 ms accumulate there; the evidence that they are
  all preserved and replayed is the exact replacement count: 5 keys
  injected within 42 ms produced 5 committed replacements (and 5
  per-episode device opens, run 1 markers). crossterm consumes ONE
  event per loop iteration (runtime.rs:78-88); each consumed key runs
  `handle_key` → the whole replacement synchronously.
- **Qianqian-owned pending navigation (Q2): DOES NOT EXIST.** There is
  no pending target, no quiet window, no debounce in the path. The
  queuing observed is exclusively OS/terminal queuing + App-thread
  serialization (D14.6 "Repeated Open is App-thread-serialized").
- These are different things and the experiment distinguishes them:
  the OS queue preserves events (proven above); the App's replay of
  them one-per-key is Qianqian's OWN choice of shell wiring, not an
  OS artifact.

## 5. Timing highlights (run 2 unless noted; precision bounds stated)

- Single manual N, real host, TOTAL (solid, two scenarios):
  **85–92 ms** key → final commit.
- Phase decomposition of that total comes from run-1 stderr markers
  and is a SINGLE SAMPLE at ConPTY chunk granularity (the opened line
  and the abort line can land in one read chunk, as in B-single-N):
  old render leg abort (`data plane stopped`) ≈ **+33 ms**, new
  episode device open completes ≈ **+81 ms**, commit render ≈ +92 ms
  (B-manual-short). Treat the split as indicative only; the TOTAL is
  the measured quantity.
- Burst totals (solid): 5×N@10ms → 317 ms; @50ms → 328; @150ms → 661
  (key-rate-limited); @250ms → 1066 (key-rate-limited); PPPP burst
  part → 247 ms; NNNPPNNP → 137 ms. The implied AVERAGE per-
  replacement cost inside a fast burst is 317/5 ≈ 63 ms — lower than
  the single 85–92 ms total; the difference (first-replacement
  overhead, draw-cadence amortization) is NOT separately measured,
  and per-period figures derived from Track-row frame timestamps are
  ConPTY-quantized (n=3 periods per scenario, spread e.g. 13–114 ms
  within one scenario) — do not read them as device-accurate periods.
- Natural EOF on a 5 s first track: auto-next committed at T+5.14 s
  from spawn — the old episode drains its full tail before `Completed`
  and the auto-next Open (see R2); the manual N on the same corpus is
  92 ms. The two operations do NOT behave identically today (§14) —
  correct per the drain semantics, and material to the design.

## 6. Observer overhead (§7)

- Zero production instrumentation was added for the counts. The
  Windows leg reuses the EXISTING off-by-default mechanism log
  (`QIANQIAN_AUDIO_LOG`; wasapi.rs:100). The harness-side marker scan
  runs in the DRIVER (outside the product): a per-chunk line scan for
  the needle with a bounded (4 KB) line buffer; the audio path is
  untouched.
- Known harness artifact, recorded: with the audio log ON, stderr
  lines interleave with VT frames on the shared pseudoconsole and
  ConPTY repaints duplicate them — raw byte counts are inflated and
  grid rows can be mid-write polluted. Counts for the verdict
  therefore come from the CLEAN round (audio log off) via Track-row
  value changes; the polluted stream is used only for corroboration
  timestamps.
- The experiment did not create the latency it measured: key
  injection, capture, and analysis all live in the driver process.

## 7. Root-cause classification (§15) — burst defect

```text
RC-A INPUT BURST REPLAY   CONFIRMED, DOMINANT (the burst defect IS this)
RC-B PROBE COST           present (~sub-10 ms share of ~60–90 ms; SongCore probe
                          was the 2 s-class defect BEFORE the probesize cap,
                          field round 1 — now bounded)
RC-C OLD EPISODE SETTLEMENT  small on manual stop (real host: 33 ms in the
                          single separable sample, R1 §5; Linux empty-edge
                          0.04 ms, full-edge bounded by the leg's current
                          device wait)
RC-D PCM EDGE TAIL        not a latency factor for manual stop (buffered frames
                          abandoned, edge.rs:232-237)
RC-E OUTPUT PADDING       not waited on for manual stop (client released);
                          DOMINANT for natural EOF only (drain_to_zero)
RC-F DECODER STARTUP      material share of the ~60–90 ms (part of the start
                          phase; not separately instrumented on Windows —
                          probe+open+activate measured together at 81 ms)
RC-G OUTPUT OPEN / SRC    material share of the same 81 ms (same phase)
RC-H UI PRESENTATION      the visible "stall" is the shell thread blocked in
                          the serialized replacement — a CONSEQUENCE of RC-A,
                          not an independent cause
```

Field reproduction (§49) on the user's real corpus with human
listening: **NOT PERFORMED** in this campaign (no human available);
the acoustic witness is recorded UNAVAILABLE as in prior campaigns.
The automated evidence above does not depend on it.
