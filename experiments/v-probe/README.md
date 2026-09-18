# V-PROBE — physical IAudioStreamVolume evidence (Stage E)

Evidence crate for the D14.9 pending item (ADR-PBK-002,
F6-AUTHORITY-PROMOTION-1; Issue #119 checkpoint). Workspace-excluded
executable mechanism evidence — nothing here may be imported by any
`qianqian-*` crate, and production behavior in this slice is ZERO.

## What is exercised

The WASAPI mechanism DIRECTLY (same windows-rs 0.62 bindings and the
same open shape as the production output crate: default endpoint,
shared mode, event-driven, float32; all threads in one MTA). Seven
scenarios probe the physical facts of the `IAudioStreamVolume`
candidate before the volume implementation may freeze its apply
placement:

```text
V1a  same-process stream isolation       failure ⇒ mechanism re-decision
V1b  other-process isolation             (child re-exec polls its own factor)
V2a  player→mixer factor independence    failure ⇒ mechanism re-decision
V2b  mixer→stream factor independence    failure ⇒ mechanism re-decision
V3   lifecycle persistence (Stop/Start)  context for the placement
V4   apply-placement perturbation        finding ⇒ apply-point/ownership
                                         reconsideration ONLY
V5   failure-signal existence            typed, distinguishable errors
```

One process = one scenario = one JSON verdict on stdout (`verdict=`
lines on stderr; exit 0 iff GREEN; 120 s evidence watchdog).

## Physical runs (real Windows host)

```bash
cd experiments/v-probe
cargo build --release --target x86_64-pc-windows-gnu
tools/run-windows.sh 1   # then 2, 3
```
