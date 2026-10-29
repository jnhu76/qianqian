# Experiment matrix — one flag at a time (T4), both targets

Generated from the archived `verdict.json` of every measured variant
(`cmp/all-verdicts/<variant>.verdict.json`, copied byte-for-byte from the
campaign evidence). Each row is one independent build-and-compare run of the
two build roots for that target.

- variant `cmp-<target>-e0`: no prefix-map flag (negative control)
- `filemap` / `macromap`: `-ffile-prefix-map` / `-fmacro-prefix-map` mapping
  the compiled FFmpeg source root to `.`
- `filemap-ctr` / `macromap-ctr`: counterfactual controls (map NOT covering
  the actually compiled FFmpeg source root)
- `winner` / `winner-b`: the adopted single-flag fix
  (`-fmacro-prefix-map=<ffmpeg srcroot>=.`)

"static sha R1==R2" is full-archive sha256 equality across the two roots;
`differing occ` is static member-occurrence count that differs (of 282).
The `leak census R1` column is the embedded-path census summary string from
the verdict (occurrences of each prefix across all members of archive 1).

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
