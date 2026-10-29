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
                         compile units; includes two no-map negative controls
build-receipts/          one build receipt per root: exact source head/tree,
                         toolchain, clone/closure provenance, artifact sha256
gates/receipts/          six G1+G2 release-gate receipts (schema
                         songcore-release-gate-g1g2/1), one per build root
gates/six-root-gate-summary.{json,md}
                         aggregation generated from those six receipts
t4-verdict-and-L.md      the T4 single-candidate verdict and the L1/L2/L3
                         determination, grounded only in the files here
```

## Method

1. **Two independent absolute build roots per target** (R1/R2), plus one
   fresh-clone/fresh-closure route per target (L1 routes):

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
