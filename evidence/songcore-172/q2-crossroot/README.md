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
build-receipts/          per-root and per-arm build receipts: exact source
                         head/tree, toolchain, clone/closure provenance,
                         artifact sha256. Pairing:
                         - six winner roots: <target>-<root>.summary.json
                           (the four R1/R2 roots also archive their
                           winner-arm census as
                           <target>-<root>-w-r{1,2}-macromap.leak-census.txt;
                           the two L1-route roots archive no census)
                         - non-ctr arms (E0 / filemap / macromap / winner):
                           <target>-<root>-<stamp>.summary.json plus the
                           same-basename .leak-census.txt
                         - ctr arms: <target>-<root>-<stamp>.summary.json;
                           the paired census is
                           fix-witness/<target>-<root>-<stamp>.leak-census.txt
drivers/                 byte-identical copies of the three campaign scripts
                         (build, compare, comparator) that produced the
                         builds and comparisons; origin recorded in
                         "Driver provenance" below
gates/receipts/          six G1+G2 release-gate receipts (schema
                         songcore-release-gate-g1g2/1), one per build root
gates/six-root-gate-summary.{json,md}
                         aggregation generated from those six receipts
t4-verdict-and-L.md      the T4 single-candidate verdict and the L1/L2/L3
                         determination, grounded only in the files here
```

Per-arm receipt stamps (the `<stamp>` part of the archived filenames; a
trailing `-.` of a build-root attempt stamp is dropped, and each JSON's
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
   indexing — duplicate member names are compared by occurrence index, never
   collapsed by basename (resolved-name count: 9 duplicate names with 13
   extra occurrences per archive; the comparator's `DUPLICATE_NAMES=9` is a
   different metric — see "Occurrence accounting and comparator naming
   (v1)") — including member order,
   per-occurrence sha256, an archive header-field audit (zeroed date/uid/gid)
   and an embedded-path leak census over the comparator's content-bearing
   rows (224/282 on linux, 185/282 on windows — folded long-name members and
   the two metadata entries carry no member content; see "Occurrence
   accounting and comparator naming (v1)").
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
| `filemap-ctr` | not rebuilt — reuses the retained `-ffile-prefix-map=<R1 ffmpeg srcroot>=.` product (the filemap arm's archive, byte-identical to the winner `-fmacro-prefix-map` archive) | rebuilt with `-ffile-prefix-map=<R2 ffmpeg srcroot>=src` |
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
| compared R1 archive (retained `mapto="."` product; see Design) | static `7b506922…`, shared `085425e5…` | static `d8c8e688…` |
| rebuilt R2 archive (`mapto=src`) | static `ebc799f9…`, shared `57529d23…` | static `29c29943…` |
| R2 build-receipt leak census (`<root>/src`, `/home/hoo`, `C:\`, `/mnt/`) | 0 on static and shared | 0 on static |
| member occurrences identical / differing (of 282) | 273 / 9 | 273 / 9 |
| `filemap-ctr` vs `macromap-ctr` R2 sha256 | equal (both `ebc799f9…` static) | equal (both `29c29943…` static) |

For contrast, the E0 controls (both roots built without any flag) record
9 differing member occurrences and absolute-root hits under both censuses.
The two censuses are not interchangeable: the verdict census cannot see
folded long-name members because they carry no member content, so it covers
224/282 rows on linux and 185/282 on windows.

- **build-receipt census**
  (`build-receipts/linux-lin-r1-a1-none.leak-census.txt`,
  `build-receipts/linux-linux-root-two-e0-r2-none.leak-census.txt`,
  `build-receipts/windows-win-r1-e0-r1-none.leak-census.txt`,
  `build-receipts/windows-windows-root-two-e0-r2-none.leak-census.txt`;
  `b329-build.sh`'s whole-artifact scan, patterns `<root>/src`,
  `/home/hoo`, `C:\`, `/mnt/`; covers all 280 members): E0 linux static
  9 occurrences of `<root>/src` per root, linux shared 3 per root, windows
  static 9 per root.
- **verdict census** (`cmp/controls/<target>-e0-verdict.json`;
  `b329-arcmp.py` `leak_scan`, patterns full root label, `/home/`, `C:\`,
  `/mnt/`, `/tmp/`; content-bearing rows only): E0 linux static 5
  full-root-prefix hits per side, windows static 1 per side; each side also
  records 1 `/tmp/` hit (`file_open.c.o`, a source-level literal unrelated
  to this fix).

The count gap (linux 4, windows 8) is folded-member territory: the E0
static comparison lists 4 `<symbol-table-ext>` DIFF rows on linux and 7 on
windows (`grep '^DIFF <symbol-table-ext>' cmp/controls/<target>-e0-static-cmp.txt`),
and the remaining gap is path literals inside folded member bodies, which
the verdict census never scans.

The winner arms record 0 build-root hits under both censuses on both targets
(verdict `leak_r1`/`leak_r2`; `build-receipts/*-w-r*.leak-census.txt`); the
verdict census still records the 1 `/tmp/` source literal per side.

### Reading: measured vs inferred

Measured: no flag → the two roots' archives differ (9 member occurrences)
and carry absolute build-root strings; each root maps its own compiled
FFmpeg source root to target `.` → byte-identical archives; target `.`
(retained R1) vs `src` (rebuilt R2) → 9 differing member occurrences while both archives carry
**zero** build-root hits under the available censuses (R1: winner verdict
census; R2: build-receipt census).

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
  the root while holding the flag and its replacement target `.` fixed, and
  still produces byte-identical archives on both targets.
- The per-byte attribution of the ctr differences is inference as stated
  above; only the counts (273 identical / 9 differing, order identical,
  9 comparator duplicate-name entries — see "Occurrence accounting and
  comparator naming (v1)") and the zero build-root verdict census are
  measured.

## Occurrence accounting and comparator naming (v1)

The occurrence counts used throughout this archive (282 comparison rows,
`DUPLICATE_NAMES=9`, `DIFFERING_OCCURRENCES`) are accounted for as follows.
Every item below was re-checked against the winner archives during this
archive revision.

1. **282 comparison rows = 280 real ar members + 2 archive metadata
   entries.** The two extra rows are `<symbol-table>` (the GNU symbol-table
   member, raw header name `/`) and `<longnames>` (the GNU long-name table,
   raw header name `//`). No object member is missing: `ar t` lists 280
   members for each winner archive.
2. **Names longer than 15 characters are folded into one synthetic label by
   the v1 comparator.** 56 of the 280 members carry names longer than 15
   characters on linux (95 on windows; check:
   `ar t <archive> | awk 'length($0)>15' | wc -l`; examples
   `songcore_ffmpeg.c.o`, `swresample_frame.c.o`). They appear in the v1 TSV
   as `<symbol-table-ext>`. The cause is in `drivers/b329-arcmp.py`: the
   `/`-initial-name branch (lines 62–67) runs before GNU long-name offset
   resolution (lines 78–83), so a `/offset` reference never reaches the
   long-name table. Their bytes are still compared occurrence by occurrence
   (the synthetic name is occurrence-indexed too) and `ORDER_IDENTICAL=yes`
   guarantees the pairing, so no verdict or digest depends on the label.
   The defect has two consequences: the names are unreadable, and the
   verdict census cannot scan these members at all (no content field), so
   its coverage is 224/282 rows on linux and 185/282 on windows. The
   archived `static-cmp.txt` files repeat the v1 tool's own section header
   `leak census (all members, both archives)`; that header is inaccurate for
   the same reason — `leak_scan` skips every member without a content field
   (the archived cmp files are left unmodified).
3. **Duplicate-name accounting.** By resolved name the archives contain
   9 duplicate names and 13 extra occurrences: `version.c.o` ×4,
   `utils.c.o` ×3, `options.c.o` ×3, and `pcm`, `mpegaudiodsp`, `iamf`,
   `flacdec`, `cpu`, `aacdec` each ×2 (windows uses `.obj`). The older
   audit's "13 duplicate-name members" is this 13-extra-occurrence count.
   The comparator's `DUPLICATE_NAMES=9` is a different metric: 8 real
   duplicate names visible as names in the v1 TSV plus the one synthetic
   long-name group. `mpegaudiodsp.c.o` (16 characters) contributes its
   occurrences to the synthetic group, so the archived v1 TSV shows only the
   folded group. This name-level conclusion comes from a campaign-host
   live-archive re-check (`ar t <archive> | sort | uniq -c | awk '$1>1'`),
   not from the archived TSV. Under either metric every occurrence is
   compared and no object is skipped.
4. **v1 label defect status.** The branch ordering above is recorded as a
   known defect of the campaign comparator with two consequences: unreadable
   names and incomplete verdict-census coverage (224/282 linux, 185/282
   windows). Verdicts and digests are unaffected because folded members are
   still compared occurrence by occurrence (step 2). Fixing the label
   ordering is left to a later change; it changes no boundary in this
   archive.

## Driver provenance

`drivers/b329-build.sh`, `drivers/b329-cmp.sh` and `drivers/b329-arcmp.py`
are byte-identical copies of the campaign scripts, taken from
`/mnt/c/Users/fred1/AppData/Local/Temp/qq172wf/329/` on the campaign host:

| file | sha256 |
|---|---|
| `b329-build.sh` | `58222872e316db9239cf6aa04b46bbe7e872c6299b833471aedec1afd3fba951` |
| `b329-cmp.sh` | `1d8a4bb2ce7b2c7998437cd88902bbb4bc7321d560d0925557d606c35c2629df` |
| `b329-arcmp.py` | `be919210bf85a8613fdfe4d98427bff6ea08e5ebb46605f569090601b24bb7ba` |

`b329-cmp.sh` invokes `b329-arcmp.py` by campaign-host absolute path; the
archived copy preserves that invocation verbatim. The scripts are evidence of
the executed procedure, not repository tooling; they are not run by any
check.

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
