# T4 verdict and L1/L2/L3 determination — SongCore #172 Q2 cross-root (#329)

Every claim below is grounded in the files in this directory (paths relative
to this file). This is campaign evidence, not release authority: #172 G4 and
the release contract govern any release decision. Definitions used are the
#329 owner authorization's: L1 = fresh clone AND freshly derived FFmpeg
closure per target; L2 = genuine G2 on R2 for both targets (including native
Windows PE runtime) alongside R1, with ABI/PCM/seek/status/metadata parity;
L3 = byte identity of all shipped artifacts across roots.

## T4 — single-candidate verdict

Defect (E0 negative controls, `cmp/controls/`): with no prefix-map flag, the
two roots' static archives differ on both targets —
`all_identical=false`, 273/282 member occurrences identical, 9 differing
(archive symbol-table occurrences plus FFmpeg object members; the differing
list is in the archived `*-e0-static-cmp.txt`) — and absolute build-root
literals are present in the archives (verdict census per archive of
extracted member bodies: 5 hits of the full R1-root prefix on linux, 1 on
windows, symmetric on R2; the build-receipt whole-artifact census records
9 occurrences of `<root>/src` in linux static, 3 in linux shared and 9 in
windows static — see README "Counterfactual arm semantics").

One flag at a time, on the actually replayed FFmpeg translation units
(matrix: `cmp/experiment-matrix.md`; full verdicts: `cmp/all-verdicts/`):

| arm | linux | windows | reading |
|---|---|---|---|
| E0: no map (control) | differs (9 occ) | differs (9 occ) | root literals are the cause of cross-root difference |
| `-ffile-prefix-map=<ffmpeg srcroot>=.` | identical | identical | wider flag also suffices; NOT adopted |
| counterfactual: each root maps its own compiled FFmpeg src root; R1 target `.` (retained `mapto="."` product) vs R2 target `src` (rebuilt) (`filemap-ctr` / `macromap-ctr`) | differs (9 occ; 0 root-path hits) | differs (9 occ; 0 root-path hits) | cross-root identity also requires the same replacement target string |
| `-fmacro-prefix-map=<ffmpeg srcroot>=.` | identical | identical | adopted single candidate |

The `*-ctr` row is not a symmetric two-root rebuild: R1 is the retained
`mapto="."` product (the filemap arm's archive for `filemap-ctr`, the winner
arm's for `macromap-ctr`; byte-identical) and only R2 is rebuilt
(`mapto="src"`). Its 9 differing occurrences and its zero root-path census
on both sides are measured; attributing the surviving bytes to the
replacement target string is inference, because the ctr R2 products were
overwritten by later arms before archiving and the differing literals were
not captured (README "Counterfactual arm semantics").

Flag delivery (witness census, `fix-witness/`): each of the six per-root
winning witnesses records `compile_commands_entries=281`,
`fmacro_prefix_map_entries=279`, `ffile_prefix_map_entries=0`,
`ffmpeg_src_entries=560` — the flag demonstrably reaches the replayed FFmpeg
compile units on every root, and the two no-map controls record 0/0. The
four ctr witnesses record the ctr flag on 279/281 entries
(`ffile_prefix_map_entries=279` for `fctr-r2`, `fmacro_prefix_map_entries=279`
for `mctr-r2`), so the counterfactual varied only the replacement target.

**T4 verdict: GO for the single flag `-fmacro-prefix-map=<ffmpeg srcroot>=.`
applied to the replayed FFmpeg translation units**, adopted as the one-line
change in `native/build/ffmpeg.lua` on this branch. Causality as measured:
no map → differs (9 member occurrences) with absolute build-root literals
present; each root mapping its own compiled FFmpeg src root to target `.` →
byte-identical on both targets; target `.` (retained R1 archive) vs `src`
(rebuilt R2) → differs (9 member occurrences) with zero absolute-root hits
on either side. The counterfactual therefore isolates the replacement target
string, not map coverage; its byte-level attribution is inference as noted
above. No `strip` / `-g0` was needed or used; no flag stacking (the wider
`-ffile-prefix-map` arm also measured identical and was not adopted).

Winner detail (`cmp/linux-winner/`, `cmp/windows-winner/`): 282/282 member
occurrences identical, member order identical, 9 comparator duplicate-name
entries equal by occurrence index (see README "Occurrence accounting and
comparator naming (v1)"), `HEADER_ANOMALIES=0`; full-archive sha256 equal
across roots — linux static `7b506922…` AND shared `085425e5…`; windows
static `d8c8e688…` (static-only form). Residual: one identical `/tmp/`
literal in `file_open.c.o` on both sides of both targets — a source-level
literal, not a build-root leak (see README Boundaries).

## L determination

| level | linux | windows |
|---|---|---|
| L1 (fresh clone + fresh closure route) | PASS | PASS (mingw-w64 cross route only) |
| L2 (G2 on R2 both targets, native Windows PE runtime, parity) | PASS | PASS within static-only scope |
| L3 (byte identity of shipped artifacts across roots) | PASS (static + shared) | PASS for the shipped static form; shared NOT RUN |

Details and explicit limits:

- **L1 linux — PASS.** `gates/receipts/linux-lin-l1-route/` verdict PASS;
  `build-receipts/linux-lin-l1-route.summary.json` records `clone=fresh`,
  `closure=fresh` at head `b49b18cd…` / tree `c868f35f…`.
- **L1 windows — PASS for the mingw-w64 cross route.**
  `gates/receipts/windows-win-l1-route/` verdict PASS;
  `build-receipts/windows-win-l1-route.summary.json` records
  `clone=fresh`, `closure=fresh`. NOT established: an MSVC /
  native-Windows-toolchain L1 route.
- **L2 linux — PASS.** G1+G2 PASS on R2 (`gates/receipts/linux-linux-root-two/`)
  and on R1 (`linux-lin-r1`), static and shared forms, real ELF runtime
  (`gates/six-root-gate-summary.md`); cross-root artifacts byte-identical
  (above).
- **L2 windows — PASS within this campaign's static-only scope.** G1+G2 PASS
  on R2 (`gates/receipts/windows-windows-root-two/`) and R1
  (`windows-win-r1`); the G2 consumer PE executed on the real Windows host via
  cmd.exe interop (each receipt's `g2_forms_note`;
  `fixture_transport.windows_path` under the real Windows `%TEMP%`). Shared
  form: NOT RUN — windows ships static-only this round (release contract §6);
  no static-only artifact is counted as Windows shared support. This is not a
  Windows SUPPORTED claim.
- **L3 — measured PASS for shipped forms.** Linux static `7b506922…` and
  shared `085425e5…`, windows static `d8c8e688…`: each sha256 equal across
  R1/R2 (`cmp/linux-winner/verdict.json`, `cmp/windows-winner/verdict.json`).
  No windows shared artifact exists in this campaign, so none is claimed.
  Release-level L3 ratification — packaging G4, docs-verify, license checks,
  a new RC digest — is NOT RUN and NOT claimed here.
- **Explicit NOT RUN / NOT ESTABLISHED:** windows shared form; MSVC/native
  toolchain routes; non-interop (CI-host) Windows execution; new RC
  publication. Legacy RC `7b4d0615…` remains immutable and unreplaced.

Overall: with this one-flag fix, both targets' R1↔R2 shipped artifacts are
byte-identical and all six roots pass G1+G2. The release itself stays
BLOCKED_UNTIL_G4 (issue #329 wording); nothing here merges, tags or publishes.
