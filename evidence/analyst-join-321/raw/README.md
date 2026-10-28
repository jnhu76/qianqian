# Raw receipts — provenance and hashes

These are the original closeout-3 receipts, copied byte-for-byte into version
control. They were produced on a Linux WSL2 host (rustc 1.97.1) from the
closeout-3 worktrees; the source tree was `main@98ff343` for the baseline and the
fix branch `3af2f48` for the fix/falsifier runs. They are evidence, not authority.

```text
sha256                                                            file
8a8437276c3d603e65a6122a64e23016618c034b1dc24be1e925b310a025a5bc  01-healthy-control.txt
189da4583f54a60cd3809036e0228cfdc85147bf5b0306d9e7d62619ec793389  02-broken-stderr-panic.txt
d2f973e22b5248eebdfab20d5c34e48f168170bd4f33b455ccfcfd3c20b14281  03-blocked-wedge-oracle.txt
01a5f19f13fbb67fda50c20eac6607c99850294b42a12b18fc27d56b7ba8d9d6  03c-blocked-no-panic-control.txt
00492c7075df92e2bfd94bed8f627d47dd49706c63fc04ab6bc23f4d0ed2247d  04-falsifier-alpha-fix-oracle.txt
bc5562d3b2a5f106f1cd10eccba422f364adb0f987daa3e4552820a2db684d91  07-regression-negative-control.txt
50182b453a77ef7418c32e10527b8dd3bf39d5cc56e7ebea786cca2e348f55fb  08-manual-wedge-gdb-stacks.txt
fa85eb485e03497c1b9b40d672064db817149fa8c66cee720718d06bc6d0cd84  09-regression-main-red.txt
4bff0c4995eb4e76c9672453ec7e61befd5ac4cc078937a9893bea117bfae55b  10-regression-fix-green.txt
```

| File | Original record | What it shows |
|---|---|---|
| `01-healthy-control.txt` | closeout-3 `receipts/01` | real session, no panic: `Discharged`, analyst gone |
| `02-broken-stderr-panic.txt` | closeout-3 `receipts/02` | EBADF stderr + real panic: previous hook returns, `Discharged` |
| `03-blocked-wedge-oracle.txt` | closeout-3 `receipts/03` | the subject: markers, `/proc` wchan, pipe occupancy, no dispose verdict |
| `03c-blocked-no-panic-control.txt` | closeout-3 `receipts/03c` | saturated sink without panic: `Discharged` |
| `04-falsifier-alpha-fix-oracle.txt` | closeout-3 `receipts/04` | α registration flips the same oracle to `Completed` |
| `07-regression-negative-control.txt` | closeout-3 `receipts/07` | permanent unit oracle RED without the fix |
| `08-manual-wedge-gdb-stacks.txt` | closeout-3 verify `v02` | independent gdb stacks: default hook in fd-2 write, decode in join, dispose in worker join |
| `09-regression-main-red.txt` | closeout-3 verify `v06` | unit oracle RED on unfixed main |
| `10-regression-fix-green.txt` | closeout-3 verify `v07` | unit oracle GREEN on the fix |

The closeout-3 experiment injected the panic immediately before `catch_unwind`.
The versioned overlay now injects it **inside** the protected region; the replay
script re-derives the same baseline/fix outcomes with that stricter shape. The
outcomes in these receipts are unchanged and are not presented as if produced by
the overlay.

## Final replay receipts (`replay-final/`)

Produced by `replay.sh origin/main HEAD` after the rebase onto
`main@61d834a`, with the fix at `c25d133`. `ENVIRONMENT.txt` records the
revisions, rustc and host.

```text
01-baseline-wedge.log             base: ORACLE observed=Wedged, previous hook
                                  entered, unread=65536, no DISPOSE-RETURNED
02-fix-complete.log               fix: ORACLE observed=Completed,
                                  DISPOSE-RETURNED verdict=Discharged,
                                  CRITICAL-PANICS=1, previous hook absent
03-healthy.log                    Discharged
04-broken-stderr-panic.log        previous hook returns, Discharged
05-blocked-sink-no-panic.log      Discharged, CRITICAL-PANICS=0
06-negative-control-wedge.log     registration after the protected region:
                                  ORACLE observed=Wedged again
```

These are generated evidence, not hand-edited. The script's RUN lines are the
authority for the sequence; the logs are the raw child output.
