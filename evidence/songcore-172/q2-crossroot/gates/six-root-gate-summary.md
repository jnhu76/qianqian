# Six-root G1+G2 gate receipt summary

Generated from the six gate receipts archived under `receipts/` (copied
byte-for-byte from the campaign `gates/out2/` outputs; schema
`songcore-release-gate-g1g2/1`, gate `G1_LINK+G2_CORE_RUNTIME`).

- G1 static: archive member-format audit + 15/15 ABI symbol audit
- G1 shared: shared-library export audit (linux only)
- G2: per form, the full S-BIND 7.1 scenario + lifetime + truncation-variant
  suite for each corpus fixture (alac-16-44-stereo, alac-long,
  flac-16-44-stereo, misc, mp3-cbr-id3v23) vs the frozen reference
- `artifact sha`: receipt `header` frozen-vs-observed staged-artifact check
- Windows rows: consumer PE cross-compiled, executed on the real Windows host
  via `cmd.exe` interop (see each receipt's `g2_forms_note`); static form only
- `failures`: receipt `failures` list length

| root key | target | verdict | G1 static | G1 shared | G2 static | G2 shared | failures | artifact sha |
|---|---|---|---|---|---|---|---|---|
| `linux-lin-r1` | linux | PASS | PASS | PASS | PASS (5 fixtures) | PASS (5 fixtures) | 0 | match |
| `linux-linux-root-two` | linux | PASS | PASS | PASS | PASS (5 fixtures) | PASS (5 fixtures) | 0 | match |
| `linux-lin-l1-route` | linux | PASS | PASS | PASS | PASS (5 fixtures) | PASS (5 fixtures) | 0 | match |
| `windows-win-r1` | windows | PASS | PASS | n/a | PASS (5 fixtures) | n/a | 0 | match |
| `windows-windows-root-two` | windows | PASS | PASS | n/a | PASS (5 fixtures) | n/a | 0 | match |
| `windows-win-l1-route` | windows | PASS | PASS | n/a | PASS (5 fixtures) | n/a | 0 | match |
