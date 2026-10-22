# PR #311 production acceptance — real Windows host (burst matrix)

Round: Owner review of PR #311 (REVISE round 2 disclosed Windows native
acceptance as NOT_RUN; the Owner then pointed the round at the real
Windows host reachable from WSL). This note records what was actually
executed, on what binaries, with what oracles — and what remains
NOT_RUN. It is evidence, not authority; the coalescing semantics
authority remains R3 + the code/tests.

## Host, binaries, provenance

```text
executed on      real Windows 11 host (the Owner's desktop), driven from
                 WSL via the campaign ConPTY harness
                 (research/transport-dogfood tuidriver.exe, Start-Process
                 detached, QIANQIAN_AUDIO_LOG=1)
product          qianqian-headless.exe (x86_64-pc-windows-gnu, release,
                 --features playback), built from fix/nav-burst-coalesce
                 head 0884b55 — product sources identical to 02901f4
                 (the two later commits touch research/ only)
                 sha256 c0a82108c9e1b3f1a6d6c6496bc7bafa5f3eee365406fddd5499c882ee13f9a3
driver           tuidriver.exe sha256 55e2ccf75ac17a9649409d68b00648fe4732d920919467d2becca8af166bd1c2
                 (round-2 instrument; P1 ran the round-1 instrument,
                 recorded in ENV-BURST-RUNP1.txt)
SongCore         libsongcore.a sha256 408d7d6ea4c9d1707283104366725c3
                 deda47d5a4cebee5102c170fdf86d404b — member inspection
                 shows genuine x86-64 COFF objects (280 members, full
                 FFmpeg closure); the link and the real-device run
                 succeeded. NOTE: #288 LEDGER F5 called this archive
                 "not a real mingw archive"; that direction is
                 corrected here (both artifacts/ and artifacts-mingw/
                 hold this same COFF archive).
media            vtest01..08.mp3 = renamed copies of a locally
                 generated 45 s synthetic sine (ffmpeg lavfi), synth5
                 regenerated with the campaign recipe (same sha256
                 ca028a20… as the research campaign's staging)
audio endpoint   default render endpoint, shared mode, event-driven
                 (per the opened/render mechanism lines in each
                 transcript; the ENV device-name line is console-
                 codepage garbled, cosmetic)
```

## Rounds

P1 (round-1 instrument): 7/9 scenarios GREEN; B-pppp-10ms and
B-nnnnn-150ms RED. Diagnosis (kept on record, logs archived): both
were blind-cadence scripts that assumed per-key commits at 150 ms
spacing. Under the coalescing shell, a key that lands while a
replacement is mid-flight is busy-interval input and is drained —
B-pppp stalled at vtest03 and B-nnnnn-150ms at 4/8, the exact
drain + deadline-first signature, with the final commit never
refuted. Product defect: none found. Instrument defect: the scripts
assumed wall-clock cadence instead of synchronizing on shell state.

P2 (synchronized instrument — each spaced key followed by its own
committed-`Track:` witness; net-zero rows added): **9/9 GREEN**,
driver exit 0. Full matrix re-run, not a delta rerun.

## P2 results (the acceptance table, real host)

```text
scenario          final_track(exp) total_opens ceiling(1+exp) burst_read      verdict
B-single-N         2/8 (2/8)       2           2             1          GREEN (exact count)
B-nnnnn-10ms       6/8 (6/8)       2           2             1          GREEN (exact count)
B-nnnppnnp-10ms    3/8 (3/8)       2           2             1          GREEN (exact count)
B-np-10ms          1/8 (1/8)       1           1             0          GREEN (exact count)
B-npnpnp-10ms      1/8 (1/8)       1           1             0          GREEN (exact count)
B-pppp-10ms        1/8 (1/8)       5           6          <=5 (line lost)          GREEN (capture-limited)
B-nnnnn-150ms      6/8 (6/8)       6           6             5          GREEN (exact count)
B-eof-natural      2/2 (2/2)       1           2          <=1 (line lost)          GREEN (capture-limited)
B-manual-short     2/2 (2/2)       1           2          <=1 (line lost)          GREEN (capture-limited)
verdict: ALL_GREEN
```

## Oracles, honestly scoped

1. PRIMARY — the driver's own premise-synchronized needles: the final
   committed `Track:` value after each burst, the `navigation: opened`
   feedback witness, the ABSENCE of any `navigation: opened` feedback
   in the net-zero rows (proven non-vacuous by forced repaint), clean
   exit 0 + quiet teardown on every row.
2. COUNT — distinct `ts_us` device-open markers (`[qianqian-wasapi]
   … opened:`), deduped (replayed lines recur across frames). A line
   is lost when no reconstructed frame carries it (the grid does not
   change when the startup open emits), so totals are lower bounds;
   the sound mechanical oracle is the CEILING `total ≤ 1 + expected
   burst opens` — the defect class policed here is EXTRA replacements
   (the #170 defect: NNNNN cost 5 device opens on this same host;
   NNNPPNNP cost 8), never a lost line. Rows where the total equals
   the ceiling have exact counts; the three capture-limited rows have
   exact counts in P1 (B-eof-natural 2 total → burst 1; B-manual-short
   2 → 1) or are bounded and needle-witnessed (B-pppp).
3. OLD vs NEW on this host: the #170 baseline measured NNNNN@10ms = 5
   replacements and NNNPPNNP@10ms = 8; P2 measures 1 and 1 with exact
   device-open counts. NP/NPNPNP (net-zero collapse, 2 and 6 opens in
   the per-key model) are pinned at ZERO opens with absence witnesses.

NOT claimed here: audibility. The acoustic witness remains a human
listening item. Windows TUI rendering/interaction IS exercised end to
end through ConPTY (real terminal stream, real key injection, real
WASAPI device opens).

## Reproduce

```bash
# from the WSL worktree (fix/nav-burst-coalesce):
QIANQIAN_NATIVE_DIR=<staging: include/songcore.h + build/artifacts/libsongcore.a> \
  cargo build --release --target x86_64-pc-windows-gnu --features playback \
  -p qianqian-headless --bin qianqian-headless
(cd research/transport-dogfood && cargo build --release --target x86_64-pc-windows-gnu)
PATH="$PATH:/mnt/c/Windows/System32:/mnt/c/Windows/System32/WindowsPowerShell/v1.0" \
  bash research/navigation-burst/tools/run-burst.sh P2 \
  B-single-N B-nnnnn-10ms B-nnnppnnp-10ms B-np-10ms B-npnpnp-10ms \
  B-pppp-10ms B-nnnnn-150ms B-eof-natural B-manual-short
python3 research/navigation-burst/tools/analyze_production_acceptance.py P2
```

Evidence files: `runP1-*` / `runP2-*` (json/txt/markers.txt/raw.txt per
scenario), `burst-runP1.summary` / `burst-runP2.summary`,
`ENV-BURST-RUNP1.txt` / `ENV-BURST-RUNP2.txt`,
`win-msvc-cargo-test.log`, plus
`../qianqian-navburst-logs/win-accept/` (build logs, run logs, analysis
outputs, P1 diagnosis) outside the repository.

## Windows-native (MSVC) test suite

Beyond the GNU product binary's device lanes, the branch's test suite
was compiled and run NATIVELY on the same host with the Windows MSVC
toolchain (cargo 1.96.0, fresh clone `C:\Users\fred1\source\qianqian-accept`
at 0d36643 — product sources identical to 02901f4; the user's own
checkout was not touched):

```text
cargo test -p qianqian-headless
401 lib + 0 bin + 12 grammar + 2 quickstart + 17 status
  = 432 passed; 0 failed            (win-msvc-cargo-test.log)
```

(The per-platform lib totals differ from Linux's 429 — cfg-partitioned
tests — and both platforms are 0-failed.)

## Still NOT_RUN

Human listening witness (audibility on real speakers/headphones) — an
owner-side item by nature; nothing in this round claims it.
