# Experiment matrix — one flag at a time (T4), both targets

Generated from the archived `verdict.json` of every measured variant
(`cmp/all-verdicts/<variant>.verdict.json`, copied byte-for-byte from the
campaign evidence). Each row is one build-and-compare run of the two build
roots for that target. The two `*-ctr` rows are **asymmetric by design**:
R1 is not rebuilt there — the comparison reuses the retained `mapto="."`
product (the filemap arm's archive for `filemap-ctr`, the winner arm's for
`macromap-ctr`; byte-identical) — and only R2 is rebuilt, with `mapto="src"`
(see the README section "Counterfactual arm semantics").

Variant labels:

- `e0`: no prefix-map flag on either root (negative control)
- `filemap` / `macromap`: `-ffile-prefix-map` / `-fmacro-prefix-map` with
  each root's own compiled FFmpeg source root as the `from` prefix and `.`
  as the target, on both roots
- `filemap-ctr` / `macromap-ctr`: counterfactual controls — each root's own
  compiled FFmpeg source root as the `from` prefix, target `.` in the
  retained R1 `mapto="."` product vs `src` in the rebuilt R2 archive
- `winner` / `winner-b`: the adopted single-flag fix
  (`-fmacro-prefix-map=<ffmpeg srcroot>=.`), both roots

Variant names are archived verbatim: the linux `filemap-b` / `macromap-b` /
`winner-b` labels carry a `-b` suffix that the windows counterparts do not.

"static sha R1==R2" is full-archive sha256 equality across the two roots;
`differing occ` is static member-occurrence count that differs (of 282).
The `leak census R1` column is the embedded-path census summary string from
the verdict (occurrences of each prefix across all members of archive 1);
on a `*-ctr` row, R1 is the retained `mapto="."` product (the filemap arm's
for `filemap-ctr`, the winner arm's for `macromap-ctr`; these products are
byte-identical). Occurrence and duplicate-name accounting (282 rows =
280 members + 2 archive metadata entries; the `DUPLICATE_NAMES` metric) is
documented in the README section "Occurrence accounting and comparator
naming (v1)".

| variant | target | all_identical | differing occ | static sha R1==R2 | shared identical | leak census R1 |
|---|---|---|---|---|---|---|
| `cmp-linux-e0` | linux | False | 9 | false | false | `{/home/hoo/qq329/lin-r1: 5, /home/: 5, C:\: 0, /mnt/: 0, /tmp/: 1}` |
| `cmp-linux-filemap-b` | linux | True | 0 | true | true | `{/home/hoo/qq329/lin-r1: 0, /home/: 0, C:\: 0, /mnt/: 0, /tmp/: 1}` |
| `cmp-linux-filemap-ctr` | linux | False | 9 | false | false | `{/home/hoo/qq329/lin-r1: 0, /home/: 0, C:\: 0, /mnt/: 0, /tmp/: 1}` |
| `cmp-linux-macromap-b` | linux | True | 0 | true | true | `{/home/hoo/qq329/lin-r1: 0, /home/: 0, C:\: 0, /mnt/: 0, /tmp/: 1}` |
| `cmp-linux-macromap-ctr` | linux | False | 9 | false | false | `{/home/hoo/qq329/lin-r1: 0, /home/: 0, C:\: 0, /mnt/: 0, /tmp/: 1}` |
| `cmp-linux-winner-b` | linux | True | 0 | true | true | `{/home/hoo/qq329/lin-r1: 0, /home/: 0, C:\: 0, /mnt/: 0, /tmp/: 1}` |
| `cmp-windows-e0` | windows | False | 9 | false | n/a (static-only) | `{/home/hoo/qq329/win-r1: 1, /home/: 1, C:\: 0, /mnt/: 0, /tmp/: 1}` |
| `cmp-windows-filemap-ctr` | windows | False | 9 | false | n/a (static-only) | `{/home/hoo/qq329/win-r1: 0, /home/: 0, C:\: 0, /mnt/: 0, /tmp/: 1}` |
| `cmp-windows-filemap` | windows | True | 0 | true | n/a (static-only) | `{/home/hoo/qq329/win-r1: 0, /home/: 0, C:\: 0, /mnt/: 0, /tmp/: 1}` |
| `cmp-windows-macromap-ctr` | windows | False | 9 | false | n/a (static-only) | `{/home/hoo/qq329/win-r1: 0, /home/: 0, C:\: 0, /mnt/: 0, /tmp/: 1}` |
| `cmp-windows-macromap` | windows | True | 0 | true | n/a (static-only) | `{/home/hoo/qq329/win-r1: 0, /home/: 0, C:\: 0, /mnt/: 0, /tmp/: 1}` |
| `cmp-windows-winner` | windows | True | 0 | true | n/a (static-only) | `{/home/hoo/qq329/win-r1: 0, /home/: 0, C:\: 0, /mnt/: 0, /tmp/: 1}` |

Per-row reading:

- `e0` rows: cross-root bytes differ (9 of 282 occurrences) and the archives
  carry absolute build-root literals. Two censuses with different scopes
  agree:
  - verdict census (`cmp/controls/<target>-e0-verdict.json`; comparator
    occurrence count over extracted member bodies): 5 hits of the full R1
    root prefix in linux static, 1 in windows static.
  - build-receipt census
    (`build-receipts/<target>-<root>-<stamp>.leak-census.txt`;
    whole-artifact scan): 9 occurrences of `<root>/src` in linux static,
    3 in linux shared, 9 in windows static.
  The winner `w-r*` build-receipt censuses are all 0.
- `filemap` / `macromap` / `winner` rows: 282/282 occurrences identical,
  member order identical, zero root-path hits on both sides — each root maps
  its own compiled FFmpeg source root to the same target `.`.
- `*-ctr` rows: 273/282 identical and 9 differing while the leak census is
  **0 on both sides**. The R1 archive is a retained `mapto="."` product (the
  filemap arm's for `filemap-ctr`, the winner arm's for `macromap-ctr`), not
  an independent rebuild; the surviving difference is attributable to the
  different replacement target string (`.` vs `src`) by inference — the
  differing literals were not archived because the ctr R2 products were
  overwritten by later arms (README "Counterfactual arm semantics").
