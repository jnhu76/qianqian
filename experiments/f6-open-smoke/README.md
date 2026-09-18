# F6-OPEN-SMOKE — physical Open/replacement evidence (Stage C)

Evidence crate for the F6 Open implementation gate (Issue #119
checkpoint; authority: `docs/adr/ADR-PBK-002.md` §20 D14.6, the
F6-AUTHORITY-PROMOTION-1 amendment). Workspace-excluded executable
mechanism evidence — nothing here may be imported by any `qianqian-*`
crate, and the production behavior this slice ships is exactly the
D14.6 implementation, no more.

## What is exercised

The PRODUCTION reference player
(`apps/headless/src/player.rs` `ReferencePlayerApp`) with the REAL
provider wiring — SongCore decode (`probe_media` + the plugin) and the
WASAPI output — over real media on the real Windows host. The wiring
mirrors the headless binary's `RealEpisodeSource` (which is
main.rs-private; the duplication is the harness's, never the
product's).

## Scenarios

```text
O1  valid A → Open valid B        replaced: A settles Stopped, B established
O2  valid A → Open INVALID        REFUSED before any destructive step; A
                                  keeps consuming; a later valid Open commits
O3  paused A → Open B             D14.7 pause establishment first, then
                                  stop-from-paused ⇒ Stopped; B established
O4  seeked A → Open B             D14.5 cut to ~40 s, then replacement;
                                  A settles Stopped; B established
O5  repeated A→B→C→D              every old Stopped, every new established,
                                  quit reports Stopped + Discharged
```

Mechanical witness for "live/continues": the render leg's own position
publication (D14.8) advancing at source rate. The audible continuity
property stays a human-ear item (F5/F6 precedent, recorded
UNAVAILABLE in RESULTS.md) — a failed ear check reopens the verdict,
same posture as the S-PROBE conditional green.

Env-gated differential aids (used while isolating shapes; not part of
the oracle): `QN_OSMOKE_SKIP_PROBE=1` skips the real probe query,
`QN_OSMOKE_TRACE=1` long-prints the observation instead of running O1's
oracle.

## Physical runs (real Windows host)

Cross-build (from WSL; mingw COFF SongCore archive staged so
`build.rs` finds `include/songcore.h` + `build/artifacts/libsongcore.a`
for the windows target):

```bash
cd experiments/f6-open-smoke
mkdir -p /tmp/qn-osmoke-stage/native/build/artifacts
ln -sfn "$REPO/native/include" /tmp/qn-osmoke-stage/native/include
cp "$REPO/native/build/artifacts-mingw/"*.a /tmp/qn-osmoke-stage/native/build/artifacts/
QIANQIAN_NATIVE_DIR=/tmp/qn-osmoke-stage/native \
    cargo build --release --target x86_64-pc-windows-gnu
```

Run the matrix (3 independent runs; the runner stages exe + media and
generates the invalid candidate itself):

```bash
tools/run-windows.sh 1   # then 2, 3
```

One JSON verdict per scenario lands in `evidence/logs/` with the full
stderr trail (endpoint-open line included); environment identities
land in `evidence/ENV-RUN<N>.txt`. Exit 0 per scenario iff GREEN; a
wedged scenario hits the 120 s evidence watchdog (exit 42).
