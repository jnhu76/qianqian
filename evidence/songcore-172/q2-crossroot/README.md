# songcore-172 q2-crossroot — cross-root FFmpeg macro-path evidence (#329 Q2)

Campaign evidence for issue #329 (SongCore #172 Q2 release blocker: absolute
build-root strings baked into ordinary sections of the shipped SongCore
archives via FFmpeg `__FILE__`-family macro expansions). Archived under the
`evidence/` rules of `evidence/README.md`: non-authoritative evidence only;
it defines no architecture semantics and grants no release authority.

## Base and the fix under evidence

- Source base: `b49b18cd68ae1a353cd3396efb3977b8590cc4d1` (merge of #328).
  Every archived build receipt records that same `head` and tree
  `c868f35f8edf3eaac60c31e7d92fa518da30ed3d`; this branch's tree at the same
  commit is byte-identical (`git rev-parse HEAD^{tree}` = `c868f35f…`).
- Fix under evidence (T4 winner, single candidate): one forced flag inside the
  `qianqian_av` `on_load` in `native/build/ffmpeg.lua`, immediately after the
  generated include dirs:

  ```lua
  target:add("cflags", "-fmacro-prefix-map=" .. srcroot .. "=.", {force = true})
  ```

  It remaps macro-expansion literals (`__FILE__` family) of the replayed
  FFmpeg translation units from the absolute FFmpeg source root to `.`, so
  shipped archive members stop carrying the build root.

## Layout

```text
cmp/linux-winner/        R1↔R2 archive comparison for the linux target:
                         static-1.tsv / static-2.tsv (per-member occurrence
                         table), static-cmp.txt (comparison report),
                         verdict.json (machine verdict)
cmp/windows-winner/      same for the windows target (static-only form)
cmp/controls/            E0 no-map negative controls (both targets):
                         cross-root bytes DIFFER and absolute build-root
                         literals are present in the archives
cmp/all-verdicts/        verdict.json of all 12 measured variants
cmp/experiment-matrix.md matrix generated from those 12 verdicts
fix-witness/             compile_commands.json census per build root proving
                         which flags actually reached the replayed FFmpeg
                         compile units; includes two no-map negative controls.
                         The four counterfactual arms are archived as
                         <target>-<root>-<stamp>.txt plus
                         .fix-patch-lines.txt / .summary.json /
                         .leak-census.txt sidecars
build-receipts/          one build receipt per root: exact source head/tree,
                         toolchain, clone/closure provenance, artifact sha256.
                         Winner roots are <target>-<root>.summary.json;
                         per-arm receipts are
                         <target>-<root>-<stamp>.summary.json
drivers/                 byte-identical copies of the two campaign scripts
                         that produced the builds and comparisons; origin
                         recorded in "Driver provenance" below
gates/receipts/          six G1+G2 release-gate receipts (schema
                         songcore-release-gate-g1g2/1), one per build root
gates/six-root-gate-summary.{json,md}
                         aggregation generated from those six receipts
t4-verdict-and-L.md      the T4 single-candidate verdict and the L1/L2/L3
                         determination, grounded only in the files here
```

Per-arm receipt stamps (the `<stamp>` part of the archived filenames; the
trailing `.` of a build-root attempt stamp is dropped, and each JSON's
`attempt_stamp` field repeats the original):

| arm | linux R1 / R2 | windows R1 / R2 |
|---|---|---|
| E0 (no flag) | `a1-none-.` / `e0-r2-none-.` | `e0-r1-none-.` / `e0-r2-none-.` |
| `filemap` | `f-r1-filemap-.` / `f-r2-filemap-.` | `f-r1b-filemap-.` / `f-r2-filemap-.` |
| `macromap` | `m-r1-macromap-.` / `m-r2-macromap-.` | `m-r1-macromap-.` / `m-r2-macromap-.` |
| `filemap-ctr` | — (R1 retained) / `fctr-r2-filemap-src` | — (R1 retained) / `fctr-r2-filemap-src` |
| `macromap-ctr` | — (R1 retained) / `mctr-r2-macromap-src` | — (R1 retained) / `mctr-r2-macromap-src` |

Two mapping notes: the linux E0 R1 receipt is stamped `a1-none-.` and its
artifacts are the R1 archives compared by `cmp-linux-e0` (static sha256
`916d113c…`). The windows `filemap` R1 attempt `f-r1-filemap-.` left no
summary or artifact record — its build log records an xmake error generating
`__cpp_qtpalette.c.c` — and the arm was completed as `f-r1b-filemap-.`
(static sha256 `d8c8e688…`), whose summary is the one archived.

## Method

1. **Two independent absolute build roots per target** (R1/R2), plus one
   fresh-clone/fresh-closure route per target (L1 routes). The four
   counterfactual (`*-ctr`) arms are the explicit exception: they are
   asymmetric by design and only R2 is rebuilt — see "Counterfactual arm
   semantics".

   Roots:

   | root key (`gates/out2/` name) | target | build root | clone/closure |
   |---|---|---|---|
   | `linux-lin-r1` | linux (gcc 15.2.0) | `/home/hoo/qq329/lin-r1` | reuse |
   | `linux-linux-root-two` | linux | `/home/hoo/qq329/linux-root-two` | reuse |
   | `linux-lin-l1-route` | linux | `/home/hoo/qq329/lin-l1-route` | fresh/fresh |
   | `windows-win-r1` | windows (x86_64-w64-mingw32-gcc 13-win32) | `/home/hoo/qq329/win-r1` | reuse |
   | `windows-windows-root-two` | windows | `/home/hoo/qq329/windows-root-two` | reuse |
   | `windows-win-l1-route` | windows | `/home/hoo/qq329/win-l1-route` | fresh/fresh |

   Clone/closure provenance per root is recorded in `build-receipts/`. The
   root-key → build-root mapping is the campaign layout; each gate receipt's
   `artifact_root` is `<stage>/<root key>`.

2. **Member-accurate cross-root comparison ("cmp")**: the two roots' shipped
   `libsongcore.a` archives are compared member by member with occurrence
   indexing — duplicate member names (9 per archive) are compared by
   occurrence index, never collapsed by basename — including member order,
   per-occurrence sha256, an archive header-field audit (zeroed date/uid/gid)
   and an embedded-path leak census over all members of both archives.
   Results: `cmp/`.

3. **Fix witness**: after each build, the replayed compile database is
   censused — how many compile entries exist and how many carry
   `-ffile-prefix-map` / `-fmacro-prefix-map`. Results: `fix-witness/`.

4. **Gates**: the repository release-correctness gate G1 (ABI symbol audit +
   member-format audit; plus shared export audit on linux) and G2 (the full
   S-BIND §7.1 scenario/lifetime/truncation suite against the canonical
   frozen reference corpus) run against each root's staged artifacts. Linux
   consumers run on the WSL ELF runtime; the windows consumer PE is
   cross-compiled and executed on the real Windows host via
   `/mnt/c/Windows/System32/cmd.exe` interop, from a run directory under the
   real Windows `%TEMP%` (per receipt `g2_forms_note` and the
   `fixture_transport.windows_path` fields). Results: `gates/`.

5. **T4 verdict and L1/L2/L3 determination**: see `t4-verdict-and-L.md`.

## Counterfactual arm semantics

The `filemap-ctr` / `macromap-ctr` rows of the matrix are **asymmetric by
design**, and they do not test whether the prefix map covers the compiled
FFmpeg source root. An earlier description of these arms as a map-coverage
counterexample is withdrawn: it named a different experiment than the one
that was run.

### Design as executed

| arm | R1 | R2 |
|---|---|---|
| `filemap-ctr` | not rebuilt — the comparison reuses the winner arm's `-ffile-prefix-map=<R1 ffmpeg srcroot>=.` archive | rebuilt with `-ffile-prefix-map=<R2 ffmpeg srcroot>=src` |
| `macromap-ctr` | not rebuilt — reuses the winner arm's `-fmacro-prefix-map=<R1 ffmpeg srcroot>=.` archive | rebuilt with `-fmacro-prefix-map=<R2 ffmpeg srcroot>=src` |

- Each root maps its own compiled FFmpeg source root
  (`…/src/native/build/ffmpeg-src`) as the *from* prefix. Coverage of the
  compiled root is therefore not the parameter under test.
- The varied parameter is the replacement target string: `.` in the retained
  R1 archive vs `src` in the rebuilt R2 archive.
- Only the root-two receipts exist for these arms
  (`fctr-r2-filemap-src`, `mctr-r2-macromap-src` under `linux-root-two` /
  `windows-root-two`); neither `lin-r1` nor `win-r1` has a ctr receipt.
- Flag delivery is witnessed in `fix-witness/*-fctr-r2-*` and
  `*-mctr-r2-*`: `compile_commands_entries=281`,
  `ffile_prefix_map_entries=279` (fctr) / `fmacro_prefix_map_entries=279`
  (mctr), `ffmpeg_src_entries=560`; the added line is line 144 of the
  build-root `native/build/ffmpeg.lua` (`.fix-patch-lines.txt` sidecars).

### Measured values

| item | linux | windows |
|---|---|---|
| compared R1 archive (retained winner-arm product) | static `7b506922…`, shared `085425e5…` | static `d8c8e688…` |
| rebuilt R2 archive (`mapto=src`) | static `ebc799f9…`, shared `57529d23…` | static `29c29943…` |
| R2 leak census (`<root>/src`, `/home/hoo`, `C:\`, `/mnt/`) | 0 on static and shared | 0 on static |
| member occurrences identical / differing (of 282) | 273 / 9 | 273 / 9 |
| `filemap-ctr` vs `macromap-ctr` R2 sha256 | equal (both `ebc799f9…` static) | equal (both `29c29943…` static) |

For contrast, the E0 controls (`cmp/controls/`; both roots built without any
flag) record, in the receipt leak census, 9 occurrences of `<root>/src` in
the static archive and 3 in the linux shared archive, with 9 differing
member occurrences; the winner arms record 0 and 0.

### Reading: measured vs inferred

Measured: no flag → the two roots' archives differ (9 member occurrences)
and carry absolute build-root strings; each root maps its own compiled
FFmpeg source root to target `.` → byte-identical archives; target `.`
(retained R1) vs `src` (rebuilt R2) → 9 differing member occurrences while
both archives carry **zero** absolute-root hits.

Inference (not directly measured): the 9 differences of the ctr arms can
only come from the replacement text itself — the members whose
macro-expanded paths were rewritten now read `.` in R1 and `src` in R2.
The rebuilds write into the same root-two build roots; the artifacts
currently on disk there are the later winner products
(`7b506922…` / `d8c8e688…`), so the ctr R2 bytes were overwritten and the
differing literals were not captured. `fix-witness/` plus the archived
receipts are the only surviving per-arm evidence.

### Boundary

- A ctr row measures the effect of the **replacement target string** on
  cross-root byte identity; it is not evidence about map coverage and must
  not be cited as "a map that does not cover the compiled root fails".
- A ctr row is not an independent two-root rebuild. Only the E0, `filemap`,
  `macromap` and `winner` arms rebuild both roots.
- The ctr result does not weaken the winner claim: the winner arm varies
  neither the root nor the target and still produces byte-identical
  archives on both targets.
- The per-byte attribution of the ctr differences is inference as stated
  above; only the counts (273 identical / 9 differing, order identical,
  9 duplicate-name members) and the zero leak census are measured.

## Driver provenance

`drivers/b329-build.sh` and `drivers/b329-cmp.sh` are byte-identical copies
of the campaign scripts, taken from
`/mnt/c/Users/fred1/AppData/Local/Temp/qq172wf/329/` on the campaign host
(sha256 `58222872e316db92…` and `1d8a4bb2ce7b2c79…` respectively).
`b329-cmp.sh` invokes the comparison tool `b329-arcmp.py` from the same
directory; that tool is not archived here. The scripts are evidence of the
executed procedure, not repository tooling; they are not run by any check.

## Boundaries (read before citing)

- Everything here was produced by the #329 Q2 campaign on one WSL2 host
  (kernel `6.18.54.1-microsoft-standard-WSL2`) and is archived as-is. While
  archiving, only the repository's fast local checks were re-run (vocabulary
  check, commit-convention check); no build, gate, cmp or witness was
  re-executed.
- **Linux**: gcc (Ubuntu 15.2.0) x86_64; static AND shared forms; G2 on the
  real ELF runtime.
- **Windows**: mingw-w64 (x86_64-w64-mingw32-gcc 13-win32) cross builds;
  STATIC form only (release contract §6: windows ships static-only this
  round; no shared form is gated, built or claimed). The G2 consumer PE ran
  on the real Windows host via cmd.exe interop. This is **not** a Windows
  SUPPORTED claim: MSVC/native toolchains, installers and the full release
  matrix are out of scope and remain governed by #172 G4.
- The legacy RC ZIP
  `7b4d0615d25af464739373a0a3565f77f3208ce9c263f59e578d443801536415` is
  immutable historical evidence; nothing here republishes or replaces it, and
  no new RC, tag or release is created by this archive.
- Residual census note: exactly one `/tmp/` literal in `file_open.c.o`
  appears identically in BOTH roots on BOTH targets (see the winner
  `static-cmp.txt` leak census). It is a source-level literal, not a
  build-root leak; unchanged by this fix and out of its scope.
- The `-fmacro-prefix-map` line is measured here on the campaign's replayed
  FFmpeg closure (the pinned FFmpeg import recorded by the campaign build
  scripts). Other build configurations that do not replay FFmpeg through this
  module are not exercised by this evidence.
