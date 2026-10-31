# SongCore #172 — campaign result record

```text
TRUTH CLASS   campaign result record (non-authoritative evidence).
              It does not define architecture semantics and grants no
              release authority; the release authority is
              docs/release/songcore-release-contract.md.
SCOPE         issue #172 (multi-target SongCore release substrate), plus the
              Q2/#329 reproducibility blocker and the G4 release gate.
RESULT        RELEASED. songcore-v0.1.1 published 2026-10-31T17:58:58Z from
              the single frozen source commit fae5173f30fb24cda97394b8a1e10dbad0b5a7c6.
```

## 1. Question

Can SongCore ship one ABI v1 component release for `windows-x86_64` (static)
and `linux-x86_64` (static + shared) from a single reproducible source
commit, with per-target build, C-suite, binding-parity, packaging,
reproducibility and external-consumer evidence, under the release contract?

## 2. Source and environment identity

```text
ABI                    v1, frozen: exactly 15 song* exports
header (v0.1.1)        native/include/songcore.h sha256
                       d43a9d47b767fe9671b9691011feb1d5201f4080609010be4d6da96f16c78d2a
window build host      Windows 11 10.0.26300 x86_64, rustc/cargo 1.96.0,
                       x86_64-pc-windows-gnu, WinLibs GCC 16.2.0,
                       xmake v2.9.7, FFmpeg n9.0.1 static closure
linux build host       WSL2 Ubuntu (kernel 6.18.54.1), gcc 15.2.0,
                       rustc/cargo 1.97.1, xmake v2.9.7, same pin
FFmpeg pin             n9.0.1 @ bf1b838f2ab88b4f8fd83443325c782ea0e0f7fa,
                       source zip sha256 d16837ddbb0963753aa2739971a09b07fe14fe855a6e84f253f851127f99c746
campaign worktree      C:\Users\fred1\source\qianqian-172 (branch lineage
                       songcore-172/pr-a-build-fixes, pr-b-windows-release-lane)
```

## 3. Results

### 3.1 S2 Windows build channel — PASS

```text
artifact        native/build/artifacts/libsongcore.a
sha256          1f5f8cf8b3f3a6688a034dd35b06b2be8fa546e58b72d962e0b3b2af758fd402
members         280/280 pe-x86-64, 0 ELF
symbols         15/15 ABI v1 song* defined
negative        songcore_shared build fails closed by recipe (windows shared
                form is not built; artifacts.shared=false)
entry           ops/build-rc.ps1, single entry, fail-closed end to end
```

### 3.2 S2 C suite (G2 CORE_RUNTIME) — PASS

12/12 channel-level cases, **109/109 sub-cases, 0 FAIL**, two independent
replays (stamps `20261029-051408`, `20261029-051431`), executed on the real
Windows host against the frozen artifact above. Case 1 is a 223-line
byte-identical ABI-layout diff against the frozen C probe output
(`experiments/songcore-call-comparison/results/layout-c.txt`).

### 3.3 S2 binding parity (G3) — PASS

C consumer vs Rust binding on the same artifact and media: **21/21 channel
cases, 214/214 sub-cases, 0 FAIL** (stamp `20261028-215619`), 35/35 field-level
machine comparisons against the C-suite receipts, and the four parity fixtures'
INFO/META observation lines byte-identical to the C receipts.

### 3.4 S2 external consumer acceptance — PASS

An independent consumer built only from the package (`include/`, `lib/`, the
README link line) linked once with `-lsongcore -lbcrypt`, ran
open→probe→decode→seek(50%)→EOF→close and exited 0. Import table limited to
`bcrypt.dll`, `KERNEL32.dll`, `msvcrt.dll`. Package container sha256
`7b4d0615d25af464739373a0a3565f77f3208ce9c263f59e578d443801536415`
(1,448,965 bytes), double-computed by two toolchains.

### 3.5 S2 packaging — PASS with a declared reproducibility limit

All contract checks V1–V7, O1/O2, B4, R3/R4/R5 pass on the RC container.
Two-root reproducibility, contract §12 T1–T3:

```text
recipe            RECIPE_RECONSTRUCTIBLE
semantic ABI      SEMANTIC_ABI_VERIFIED
byte              NOT_ESTABLISHED — the two roots' static archives differ
                  (R1 1f5f8cf8…, R2 059eb84e…; 3,309,710 vs 3,309,822 bytes).
                  Root cause: absolute build-root literals inside fused
                  FFmpeg object bodies. Fixed later by Q2/#329 (see 3.8).
```

### 3.6 S3 Linux reference rebuild — PASS

From `songcore-172/pr-a-build-fixes` @ `bf23516b5851174dc63fb5058997fa5478751713`,
tree `3247b280f26efe6bc4174307758d791100c12b0d`, clean `git fsck`; the same
`git archive HEAD | sha256sum` value `f8ce88a55f674bc288652f870e30f32ca2249bda8733fef27e9b98e8e9cfc0b6`
from both the Windows worktree and the WSL clone. Artifacts `libsongcore.a`
`409ef3da…`, `libsongcore.so` `5e9c5697…`, `libqianqian_av.a` `ff6cacec…`,
closure manifest `d037e8b4…`.

### 3.7 S3 verify (Linux) — PASS for the executed scope

Canonical corpus gates green: exactly 15 exports, zero `av_`/`ff_`/`swr_`
leaked exports or header symbols, shared `NEEDED ⊆ {libc, libm}`, 280 archive
members, correctness 4/4 fixtures bit-exact (frames + `pcm_sha256` + terminal)
against the frozen `reference.json`; contract-shaped packaging V1–V7; C
consumer suite, Rust binding parity, two-root T1–T3 and an external unpack
consumer. NOT_RUN items are listed in `verdicts/s3-verify.md` §7.

### 3.8 Q2 / #329 cross-root reproducibility — FIXED and demonstrated

Negative control (E0, no prefix-map flag): the two roots' static archives
**differ** on both targets — 273/282 member occurrences identical, 9 differing;
9 absolute `<root>/src` occurrences per root in linux static (3 in linux
shared, 9 in windows static).

One flag at a time, both targets (`q2-crossroot/cmp/experiment-matrix.md`,
machine verdicts in `q2-crossroot/cmp/all-verdicts/`):

| arm | linux | windows |
|---|---|---|
| `-ffile-prefix-map=<srcroot>=.` | identical | identical |
| counterfactual (R1 `.` vs rebuilt R2 `src`) | differs (9 occ, 0 build-root hits) | differs (9 occ, 0 build-root hits) |
| `-fmacro-prefix-map=<srcroot>=.` (adopted) | **identical** | **identical** |

Flag delivery proven per root: 279/281 replayed FFmpeg compile entries carry
the flag; the two no-map controls record 0. Six-root G1+G2 gate: 6/6 roots
PASS (linux static+shared, windows static), 5/5 corpus fixtures each, 0
failures, artifact sha match. Adopted as one line in
`native/build/ffmpeg.lua`; shipped by #332 (`69ba1ce`).

### 3.9 D2 link contract

```text
windows-x86_64 static   -lsongcore -lbcrypt        (nothing else)
linux-x86_64 static     -lsongcore -lm -lpthread
linux-x86_64 shared     ELF; 15 exported song* symbols; NEEDED ⊆ {libm, libc}
windows-x86_64 shared   not built (recipe artifacts.shared=false, fails closed)
```

`BCrypt*` is referenced only by `random_seed.c.obj` (FFmpeg
`libavutil/random_seed.c`); a *minimal* consumer that only calls
`songcore_abi_version()` still fails without `-lbcrypt`, so the flag is
unconditional, not usage-dependent.

### 3.10 D1 build-provenance digest defect — FIXED

Pre-fix `crates/qianqian-songcore-sys/build.rs` shelled out to coreutils
`sha256sum` and took the first whitespace token. On Windows every path
contains backslashes, so coreutils printed `\<digest>  <escaped-name>` and the
baked constants were 65 chars with a leading `\`; with no `sha256sum` on PATH
the build panicked (`exit 101`). Post-fix the digests are computed in-process
(std-only FIPS 180-4 SHA-256 with a fail-closed NIST self-test).

```text
counterfactual   Windows red 35/35, green 47/47 PASS; WSL red 35/35,
                 green 47/47 PASS (each phase exits 0, 0 failed assertions)
cross-host       the fixed implementation yields identical digests from the
                 msvc host, the WSL host and the real crate
```

Shipped by #326 (`43d5e96`).

### 3.11 G4 release gate — RELEASED

```text
preflight at 69ba1ce    ENVIRONMENT_BLOCKED (docs-verify VitePress half could
                        not complete on the host; E01-E18 PASS in WSL;
                        NEW_RC / TAG / GITHUB_RELEASE = NOT_CREATED)
substrate               #336 (59a99d7) fail closed on unattested artifacts
identity + D1/D3        #337 (fae5173) approved 0.1.1 identity, D1 header
                        text correction, D3 out_frames_produced clearing
owner decision record   docs/release/songcore-0.1.1-approval.json
                        (Q1 0.1.1 project-coupled; Q3 windows static-only;
                        Q4 measured GLIBC_2.43 floor, no older-compat claim;
                        Q5 manual no-overwrite publication bound to source SHA;
                        D1 documentation correction; D3 clear before error)
published               tag songcore-v0.1.1, 2026-10-31T17:58:58Z
assets                  songcore-v0.1.1-linux-x86_64.tar.gz
                        2,575,553 B  sha256 97a37172e437b16d46a410e84ae515c937e49b178f566e7beb6c84ff96215256
                        songcore-v0.1.1-windows-x86_64.zip
                        1,449,435 B  sha256 f7d9ffa391b05cf47e1a23b18cd9f90d679abe951843f3dad17db23f6e61226f
                        (+ a .sha256 sidecar per asset)
```

## 4. Verdict

```text
S2 windows build / G2 CORE_RUNTIME / G3 BINDING_RUNTIME /
S2 external acceptance / S2 packaging / S3 linux build /
S3 verify / D1 / D2 / Q2          PASS (each in its stated scope)
Q2 byte reproducibility           FIXED and demonstrated identical on both trees
G3 binding-level runtime (per-target release acceptance)   NOT_RUN as a G4 gate
G4                                RELEASED as songcore-v0.1.1
```

## 5. Limits and what is *not* established

- **Windows shared / arm64 / MSVC / DLL targets:** out of scope and not
  claimed. Only `windows-x86_64` static and `linux-x86_64` static+shared exist.
- **glibc floor:** `GLIBC_2.43` is measured, not engineered. No compatibility
  claim for older distributions.
- **Recipe-level reconstruction** is claimed; **byte reproducibility** of the
  shipped archives across independent roots is claimed only *for the adopted
  prefix-map configuration* (Q2), not for arbitrary toolchain versions.
- **#296 / #324 harness evidence** is producer-thread/plugin-level, not a full
  Playback Session / D11 terminal-Fact run; see those topics' own limits.
- **G4 preflight** itself concluded `ENVIRONMENT_BLOCKED`; the release was
  reached through #336/#337 plus the owner decision record. The docs-verify
  VitePress failure was a host/file-handle condition, not a docs-content
  error, and is not reported as a pass on that host.
- **Evidence grade:** this campaign's per-run receipts and build transcripts
  were removed from the working tree on 2026-11-01 (see §7). The numbers above
  remain the recorded result; they are no longer independently re-checkable
  from committed receipts.

## 6. Fixes, decisions and entry points

```text
#326  43d5e96  correct Windows static target builds and provenance hashing (D1/D2)
#327  742abf8  Windows x86_64 release-candidate lane
#328  b49b18c  Linux S3 reference evidence
#331  0b7c5e3  ratify the S1 release contract (#330 governance lanes)
#332  69ba1ce  -fmacro-prefix-map for FFmpeg macro roots (Q2 / #329)
#336  59a99d7  fail closed on unattested release artifacts
#337  fae5173  approved 0.1.1 identity with D1/D3 corrections
```

Entry points still in the tree: `ops/build-rc.ps1`, `ops/package-rc.ps1`,
`ops/run-csuite.ps1`, `ops/run-binding-gate.ps1`, `ops/verify-external.ps1`,
`ops/build-linux.sh`, `ops/package-linux.sh`.

## 7. Retention note (2026-11-01 evidence diet)

Removed from the tree: `build/**` (per-run logs and summaries),
`binding-parity/**` (4 timestamped runs + runner logs), `suite-results/**`,
`external-accept/**`, the xmake semantics probe tree (its captured output
survives in `semantics/`), `pr-*.log` shipping-gate transcripts,
`package/{stage,songcore-v0.2.0-rc.1-windows-x86_64.zip,stage-audit.*}`,
`linux/build/2026*`, `linux/verify/{repro,rust,external}`, and the 351 KB /
339 KB `audit/*-undefined-by-member.txt` dumps.

Kept because a tracked consumer needs them: `suite/csuite_consumer.c`,
`linux/verify/package/external-consumer-example.c`,
`linux/verify/suite/{suite_ext.c,run_suite.py}` (the only copy — the
`ops/verify-suite/` path named in the S3 verdicts does not exist in the tree),
`linux/verify/package/package_linux.py`, the four small S3 receipts, and
`package/repro/**` (the receipt `ops/package-rc.ps1` writes into the shipped
manifest).

Frozen receipts still contain campaign-local paths that no longer resolve
(`package/repro/identity.txt`, `package/repro/run-compare.sh`); they were left
byte-unchanged rather than rewritten.
