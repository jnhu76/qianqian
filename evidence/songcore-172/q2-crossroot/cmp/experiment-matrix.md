# Experiment matrix — one flag at a time (T4), both targets

Generated from the archived `verdict.json` of every measured variant
(`cmp/all-verdicts/<variant>.verdict.json`, copied byte-for-byte from the
campaign evidence). Each row is one build-and-compare run of the two build
roots for that target. The two `*-ctr` rows are **asymmetric by design**:
R1 is not rebuilt there — the comparison reuses the winner arm's `mapto="."`
archive — and only R2 is rebuilt, with `mapto="src"` (see the README section
"Counterfactual arm semantics").

Variant labels:

- `e0`: no prefix-map flag on either root (negative control)
- `filemap` / `macromap`: `-ffile-prefix-map` / `-fmacro-prefix-map` with
  each root's own compiled FFmpeg source root as the `from` prefix and `.`
  as the target, on both roots
- `filemap-ctr` / `macromap-ctr`: counterfactual controls — each root's own
  compiled FFmpeg source root as the `from` prefix, target `.` in the
  retained R1 winner-arm archive vs `src` in the rebuilt R2 archive
- `winner` / `winner-b`: the adopted single-flag fix
  (`-fmacro-prefix-map=<ffmpeg srcroot>=.`), both roots

Variant names are archived verbatim: the linux `filemap-b` / `macromap-b` /
`winner-b` labels carry a `-b` suffix that the windows counterparts do not.

"static sha R1==R2" is full-archive sha256 equality across the two roots;
`differing occ` is static member-occurrence count that differs (of 282).
The `leak census R1` column is the embedded-path census summary string from
the verdict (occurrences of each prefix across all members of archive 1);
on a `*-ctr` row, R1 is the retained winner-arm archive.

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
  carry absolute build-root literals. The verdict census counts 5 hits of
  the full R1 root prefix on linux static and 1 on windows static; the
  receipt leak census counts 9 occurrences of `<root>/src` in the static
  archive and 3 in the linux shared archive.
- `filemap` / `macromap` / `winner` rows: 282/282 occurrences identical,
  member order identical, zero root-path hits on both sides — each root maps
  its own compiled FFmpeg source root to the same target `.`.
- `*-ctr` rows: 273/282 identical and 9 differing while the leak census is
  **0 on both sides**. The R1 archive is the winner product, not an
  independent rebuild; the surviving difference is attributable to the
  different replacement target string (`.` vs `src`) by inference — the
  differing literals were not archived because the ctr R2 products were
  overwritten by later arms (README "Counterfactual arm semantics").
