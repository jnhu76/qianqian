# REVIEW — independent review and dispositions

```text
TRUTH CLASS   review record (evidence about the evidence), not architecture authority
SUBJECT       #324 Windows native Tier-2 campaign
REVIEWER      independent fresh-context reviewer (did not implement the campaign),
              read-only: no rebuild of the artifact, no rerun of the cells
REVIEWED AT   evidence package + harness sources + product sources as executed
              (the twelve receipts re-verified by hash and by log inspection)
```

## Verdicts

```text
A  WINDOWS ARTIFACT   PASS
B  FAULT MECHANISM    PASS
C  ORACLE             PASS_WITH_EXPLICIT_GAP
                      (strict reading: FAIL — 4 of 12 REDs are bounded-liveness
                       assertions inside the child, not STATE reads)
D  CONTRACT           PASS
```

Review C's split is the one substantive disagreement and is **not** papered
over: see "D-1" below. Everything else was accepted with only documentation and
hygiene fixes.

## Review A — Windows artifact (PASS)

Independently verified by the reviewer with the WinLibs binutils and a direct
archive parse:

```text
objdump -f | grep -c "file format pe-x86-64"   → 280
objdump -f | grep -c "file format elf"         → 0
ar t | wc -l                                    → 280
sha256sum                                       → 90bde1ca… (matches ARTIFACT.md)
songcore_ffmpeg.c.obj                           → 5cbbad71…, 25,310 bytes
nm --defined-only | grep -c " T song"           → 15
manifest target                                 → windows-x86_64 / mingw32 /
                                                  cross_prefix x86_64-w64-mingw32-
closure                                         → 279 TUs, 0 generated
consumer PE                                     → PE32+ x86-64, imports
                                                  KERNEL32/bcrypt/msvcrt,
                                                  727 av* symbols, real source
                                                  strings, CONSUMER_SMOKE=PASS
```

The reviewer explicitly confirmed there is no fabricated hash, no Linux
artifact, and no Linux runtime evidence counted as Windows.

## Review B — fault mechanism (PASS)

Confirmed as real Windows/facility mechanisms, with no Linux premise smuggled
in:

```text
C4      CreatePipe → SetStdHandle → GetFileType==PIPE → CloseHandle(read);
        canary Err code 232 before producer work; NOT NUL; restore only after
        the drainer is quiescent and the exporter is disabled
C1      (ffmpeg/read) anonymous pipe, read end held and never read, 64 KiB raw
        write parked, independent 1-byte raw write parked, PeekNamedPipe state;
        parent never reads (never_read=true)
C1      (seek) real process-wide std::io::stderr() lock; probe proves it is
        cross-thread; explicitly distinguished from pipe backpressure
C2      drainer inside a real export call, entered over mpsc, never-released
        Condvar gate; drained/abandoned read from the facility's report
C3      producer's own class-α pool saturated; outstanding == 2; no hidden wait
```

No mutant was killed by the outer watchdog and no mutant failed to compile.

## Review C — oracle, and the one disagreement (D-1)

All twelve were verified as real: one fresh process each, healthy PASS, mutated
source hash changed, mutant executed, named assertion fired, exit 101, no PASS
marker, byte-restored source hash equal to base, restored GREEN. No zero-test
arm, no build-error-as-RED, no Linux receipt.

The reviewer's strict objection: four REDs are produced by an **in-test bounded
liveness deadline** rather than by reading an illegal state.

| Cell | Observed RED | Assertion |
|---|---|---|
| ffmpeg-C1 | `PRODUCER-NOT-BACKPRESSURED: … did not complete within 10s` (10.34 s) | the cell's own 10 s receipt wait |
| read-C1 | `C1-PRODUCER-PROGRESS: … Timeout` (10.33 s) | the cell's own 10 s receipt wait |
| read-C2 | `C2-TEARDOWN: shutdown did not return inside its bounded deadline` (3.14 s) | the cell's own 3 s report wait |
| seek-C1 | `finished_while_held=false … elapsed_ms=10007` | state read AFTER the 10 s fail-closed lock valve released |

### D-1 disposition — labelled, not hidden

```text
CLASSIFICATION  LIVENESS (4 cells) vs STATE (8 cells), recorded in MATRIX.md
                and EXECUTION_DAGS.md, and in every receipt.
TASK WORDING    "失败于编译、0 tests、环境前提未成立或仅外层 timeout 均不算有效
                RED" — the prohibition is the OUTER watchdog, not a cell's own
                bounded progress predicate. run_cell.ps1's 180 s watchdog did
                not fire in any arm (timed_out=False everywhere); every one of
                the four exited 101 with its named assertion and
                "test result: FAILED".
PRECEDENT       #323's accepted Linux package used the same shape
                (C1 PRODUCER-NOT-BACKPRESSURED, C2 bounded-deadline assertion).
LIMIT           A liveness property cannot be asserted without a bound: the
                unbounded form is a hang, which the task forbids as evidence.
                The bound is inside the child and is part of the cell's
                predicate; it is not the outer cleanup watchdog.
OWNER DECISION  The redesign question was analysed on the recorded receipts and
                mutation shapes (ORACLE-CLASS.md, v2 after an adversarial
                delta review). No in-process product-visible state oracle
                exists for these four mutations — the difference is
                non-completion, and every in-process substitute is temporal or
                over-constrained. An out-of-process thread-context oracle
                exists in principle for three of the four (read-C1 and seek-C1
                without mutation awareness; ffmpeg-C1 mutation-aware only) and
                for none of read-C2; it is not adopted, not executed, and would
                be a new mechanism/scope with a fresh run. The decision left to
                the owner is therefore class acceptance (is a named, bounded,
                in-child progress assertion a valid PRECISE_RED for C1/C2?) —
                or authorizing that new harness. Verdicts above are unchanged.
```

seek-C1's valve is now documented rather than implied: the helper waits 10 s for
`producer_done` and then releases the lock fail-closed; on the mutant that
release is what lets the process terminate and observe
`finished_while_held=false` (the illegal state), and on the healthy path the
release happens after the producer-side reads (`finished_while_held=true`).

seek-C4's counterfactual also fires the drain-boundary identity
(`C4: ACCOUNTING-ORACLE at quiescence`, left 6 / right 5) rather than the
`export_failed` assertion, because the disabled-exporter path still counts
later losses. The RED is real and is recorded with the signature that actually
fired.

## Review D — contract (PASS)

- No new system authority: the package is labelled `executed verification
  evidence / NOT architecture authority`; no D11 terminal Fact or Playback
  Session is claimed from adapter-level runs; `ShutdownReport` and the
  adapter's reliable results are the only terminal-ish claims.
- No production behavior change: `git diff HEAD` is empty; the only added Rust
  files are the Windows-only harnesses, which the PR commits under `evidence/`
  (not in the product test tree). `tools/check_architecture_vocabulary.py`
  passes.
- The two out-of-scope build findings (`build.rs` `sha256sum` path escaping;
  `songcore_shared` BCrypt link failure) are recorded and left to #172; neither
  is fixed or hidden here.
- No Plugin/K0/Logging-Manager/EventBus/scheduler/lifecycle concept is
  introduced; the harness seams are test-only `Exporter` implementations and
  Win32 calls.

## Findings ledger and dispositions

| # | Finding | Class | Disposition |
|---|---|---|---|
| 1 | Four C1/C2 REDs are bounded-liveness, not STATE | BLOCKER (strict) | **labelled** LIVENESS + documented here; `ORACLE-CLASS.md` answers the redesign question (no in-process product-visible state oracle; a thread-context option is documented but not adopted); owner class-acceptance decision remains (D-1) |
| 2 | `COMMANDS.md` reproduced from the base commit, which lacks the evidence/tests | BLOCKER | **fixed**: §0 now checks out the PR head; §2 copies the harnesses into the test tree |
| 3 | `REVIEW.md` / `RECEIPTS.sha256` advertised but absent | BLOCKER | **fixed**: this file + `RECEIPTS.sha256` added |
| 4 | seek-C1's 10 s auto-release undisclosed | SHOULD_FIX | **fixed**: documented in MATRIX/EXECUTION_DAGS/REVIEW |
| 5 | seek-C4's primary named assertion does not fire | SHOULD_FIX | **documented** with the signature that did fire |
| 6 | C2/C3 do not use the real `StderrExporter`; comment overclaimed | SHOULD_FIX | **fixed**: comment and README now state the test-owned exporter seam |
| 7 | Windows-only harnesses not platform-gated | SHOULD_FIX | **fixed**: `#![cfg(windows)]` added to all five |
| 8 | `Q324_ONLY` overwrote the 12-cell JSON | SHOULD_FIX | **fixed**: single-cell runs write `*-only-<cell>.json` |
| 9 | `COMMANDS.md` not portable (`/home/hoo`, main-tree zip copy, Store-stub `python3`) | SHOULD_FIX | **fixed**: `$WSLROOT`, `fetch-ffmpeg`, real-python note, shared-target note |
| 10 | No artifact build transcript; archiver/linker versions uncaptured | SHOULD_FIX | **fixed**: `raw/smoke/artifact-toolchain.txt` (GNU ar/ld 2.45.90) and corrected ARTIFACT.md |
| 11 | Sink-primitive probe source missing | SHOULD_FIX | **fixed**: `harness/probes/{probe,pipeinfo,grow}.rs` + COMMANDS §7 |
| 12 | No test-quality six-question record / CI-coverage disclosure | SHOULD_FIX | **fixed**: `TEST-QUALITY.md` (M3 + R-H, `CI COVERED? NO`) |
| 13 | `ar p \| sha256sum` is not binary-safe | NOTE | **documented** in ARTIFACT.md (`ar x` + hash) |
| 14 | `healthy.json` lacked source hashes | NOTE | **fixed**: source sha256 map added per healthy run |
| 15 | `environment.txt` snapshot predates the evidence dir | NOTE | **clarified** in ENVIRONMENT.md |
| 16 | Read receipts lacked `pid` | NOTE | **fixed**: each read cell prints `<CELL>-ENV=… pid=…` |
| 17 | Consumer `.exe` is a build product, not committed | NOTE | **documented**: binary removed from the package; hash + source + command recorded |
| 18 | Manifest `toolchain.system=linux` is the build host | NOTE | **documented** (target block is the identity gate) |
| 19 | `ORACLE-CLASS.md` v1 overclaimed "no program-state oracle exists" (contradicting its own §4) | BLOCKER (delta review) | **fixed** in `ORACLE-CLASS.md` v2 and the derivative pointers; the thread-context construction is documented per cell and marked unexecuted |

## Delta review of ORACLE-CLASS (fresh-context, adversarial)

After the original review, `ORACLE-CLASS.md` (commit `890d0af`) was checked by a
second fresh-context reviewer whose explicit task was to falsify it. It did not
write the document and ran read-only.

```text
VERDICT  PARTIALLY_OVERCLAIMED
```

Confirmed unchanged: the four mutations, the base/mutant/restored hashes, every
log quote, the 12/12 RED/GREEN records, 36/36 `timed_out=false`, and that no
verdict was silently changed by `890d0af`.

Corrected in v2: v1's absolute "no program-state oracle exists" contradicted its
own §4. The review supplied a concrete out-of-process construction (child under
`DEBUG_ONLY_THIS_PROCESS`, hardware execute breakpoint filtered to the producer
thread, debug events instead of timers) that decides read-C1 and seek-C1
without touching the mutation, a mutation-aware variant for ffmpeg-C1, and no
construction for read-C2. It also corrected five factual statements: the
ffmpeg-C1 park state and the two-wait duration, where read-C1 actually parks
(std stderr mutex held by the blocked drainer, before the pipe), the inverted
read-C2 direction, and the §3 wait example. `ORACLE-CLASS.md` v2 incorporates
all corrections and labels the thread-context construction as an unexecuted
option.

Residual: the construction is a design sketch, not evidence; adopting it would
be a new harness/observation mechanism and a fresh run. The campaign's RED
class and every verdict are unchanged.

## Reproducibility answer (post-fix)

With the PR head checked out, a Windows host with WinLibs GCC 16.2.0, rustc
1.96.0 + `x86_64-pc-windows-gnu`, WSL2 with xmake 2.9.7 and mingw-w64 GCC 13,
a real Python 3, and an artifact rebuilt at any path (updating the three
`EXPECTED_ARTIFACT_SHA256` constants), the twelve cells can be re-executed
**in kind**. The raw receipts are not byte-reproducible (PIDs, timings, PE link
hashes, archive path-dependence); the package's value is the hash-verified
mutation/restore chain and the real Windows premises, not binary determinism.

## What the reviewer did not verify

No rebuild and no rerun of the twelve cells (read-only review). The review is
based on the on-disk archive, manifest, test executables, all 36 logs, both
JSON records, all harness/product sources, and independent hashes/format
parsing. No device/audio/arm64/MSVC claim was checked because none is made.
