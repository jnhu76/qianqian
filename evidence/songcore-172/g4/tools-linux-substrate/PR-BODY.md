## Why

G4 cannot be executed today: the two Linux entries either cannot be pointed at a commit or do not exist.

- `ops/build-linux.sh:41-42` (pre-PR) hardcoded `BRANCH=songcore-172/pr-a-build-fixes` and `PINNED_HEAD=bf23516b5851174dc63fb5058997fa5478751713`. No argv or environment variable could retarget the commit, and `bf23516` is not an ancestor of `main` (merge-base `05f5fef`; `main` carries five commits the pin lacks, including a `native/build/ffmpeg.lua` change). Two real runs at `69ba1ce` exited 0 with `head=bf23516…`, `clone=reuse`, `closure=reuse` and `build ok, spent 0.267s` / `0.373s` with zero compile lines — a cache no-op reported as PASS, with unchanged artifact hashes.
- The clone path could never succeed on a clean host: `:49` created the evidence directory under `$DST`, `:79 do_clone` ran `rm -rf "$DST"`, `:80` redirected the clone output into the now-missing path, and `:86 cp` failed. Measured twice: exit 1 at stage `clone`.
- No Linux packaging entry existed. The only §7/§8 Linux packager was the archived campaign script `evidence/songcore-172/linux/verify/package/package_linux.py`, pinned to `/home/hoo/qianqian-172-linux`, requiring the now-absent `/tmp/s3-v010` tarball and emitting a non-publishable `0.0.0-s3-verify`.

This PR closes exactly those three gaps and nothing else.

## What changed

**`ops/build-linux.sh` — parameterized (385 lines).**
- The source pin is explicit argv: `--source-commit` (full 40-hex; abbreviated refused), `--source-branch`, `--work-root`. Both stale constants and the `QIANQIAN_S3_SOURCE` / `QIANQIAN_S3_MAIN` defaults are deleted; a run without the three required arguments refuses to start.
- Fail-closed: a branch tip that is not `--source-commit` aborts instead of checking something else out; an existing work root that is not at the pin is never deleted without `--force-reclone`; `--evidence-root` must be outside the work root (a fresh clone deletes and re-creates it — the exact path the old code lost its evidence on).
- Build semantics, stages 2–5, the closure-identity revalidation and the summary schema are unchanged. The file becomes executable (mode only).

**`ops/package-linux.sh` — new Linux packaging entry (1168 lines).**
- Contract §7 tree → `songcore-v<VER>-linux-x86_64.tar.gz`: `include/songcore.h`, `lib/libsongcore.a`, `lib/shared/libsongcore.so`, `LICENSES/{COPYING.GPLv2,COPYING.GPLv3,COPYING.LGPLv2.1,COPYING.LGPLv3,THIRD_PARTY_NOTICES.txt,LICENSE-MIT,LICENSE-APACHE}`, `README.md`, `metadata/{manifest.json,symbols.txt,checksums.txt,external-consumer-example.c}`.
- §8 schema-2 `metadata/manifest.json`: carried v0.1.0 fields (including `artifact_shared` as the shared artifact **path**, which is the v0.1.0 meaning) plus `target_identity`, `tools`, `sdk_ndk`, `source`, `pin`, `closure`, `files`, `runtime_dependencies`, `license`, `reproducibility`.
- `--version` is **required with no default**; `0.1.0` is refused as immutable history; the §2 form is enforced including "no leading zeros". No tag, no GitHub Release, no publication path.
- Issue #330 D2 set arithmetic: `checksums.txt` covers every packaged file except itself (14 entries, including `manifest.json`); `files{}` is that set minus `manifest.json` (13 entries); nothing hashes itself.
- V1–V7 + O1/O2/B3/B4 machine checks run on the **written** files before any container exists; any failure exits nonzero and leaves no container. There is no SKIP path in the tool.
- Licenses: the four FFmpeg COPYING texts are extracted from the pinned source zip after re-verifying its sha256 against `pin.json` (O1); `LICENSE-MIT`/`LICENSE-APACHE` are byte-identical to the repo-root texts (B4); `THIRD_PARTY_NOTICES.txt` uses the Issue #330 P3 wording (identity header for this package, FFmpeg facts bound to this run's verified pin/closure/profile, dual-license component paragraph).
- Deterministic container: sorted members, mtime fixed to `SOURCE_DATE_EPOCH` or the source-commit time, uid/gid 0, empty uname/gname, modes 0644/0755, the versioned root as an explicit member, no absolute path, no symlink, no pax header. The container is then re-opened and entry set, order, modes, ownership, mtime and every payload hash are re-verified; it is extracted and `sha256sum -c metadata/checksums.txt` is run.
- Artifact attribution (the audit's `artifact_attribution` finding): the optional `--build-receipt <ops/build-linux.sh summary.json>` re-verifies the receipt's `head` and **both** artifact sha256 values against the packaged bytes. Without it the package still builds, but the manifest notes and `package-summary.json` state that provenance is unattested.

## Measured here (every exit code below was produced in this run)

**Build** (WSL, work root `/home/hoo/g4-172/linux-root`, branch `main` @ `69ba1ce`):

```
bash ops/build-linux.sh --source-commit 69ba1ce05a2772f5aa077c6ece3365c40f6e0500 \
  --source-branch main --work-root /home/hoo/g4-172/linux-root \
  --evidence-root /home/hoo/g4-172/evidence
```

- run 1, empty work root: **exit 1** — the clone path now works (`stage1: cloned 69ba1ce…`), then stage 2 fails closed because this WSL VM has no network interface for `scripts/fetch-ffmpeg`.
- runs 2 and 3: **exit 255** — the Xmake replay compiles the FFmpeg sources, so a work root also needs `native/build/ffmpeg-src`; the first attempt copied it with the wrong `cp` target.
- run 4, closure + `ffmpeg-src` seeded from the already-imported Linux closure root (no re-download), **exit 0**: `verdict: PASS`, head `69ba1ce…`, static `0d38f4b5a8bd812ba262ca2dc26c32f2d1a596511222d930e685db25d7361308` (4,066,980 B), shared `085425e590b0d6d810192404162cef6f1f1f880f769e4afd12e4af5306078e98` (2,288,536 B), 280/280 archive members `elf64-x86-64`, 15/15 ABI symbols in both forms, NEEDED `libm.so.6, libc.so.6`.

**Package** (same work root, `SOURCE_DATE_EPOCH=1793000000`):

```
bash ops/package-linux.sh --version 0.2.0-rc.1 \
  --source-commit 69ba1ce05a2772f5aa077c6ece3365c40f6e0500 \
  --work-root /home/hoo/g4-172/linux-root --out-dir /home/hoo/g4-172/packages/runA \
  --build-receipt /home/hoo/g4-172/evidence/build-linux/20261030T125315Z/summary.json
```

- **exit 0**; `songcore-v0.2.0-rc.1-linux-x86_64.tar.gz` sha256 `3dc50510dde1b0e99969e0027f1c3b2bc11a2b62d90af499dce99cb755681131`, 2,575,417 B.
- 15 packaged files / 14 `checksums.txt` entries / 13 `files{}` entries; V-checks **15/15 PASS**; container re-verified (21 entries = 15 files + 6 directories); `sha256sum -c` 14/14 OK inside the extracted container.
- A second run into a different `--out-dir` with the same `SOURCE_DATE_EPOCH` produced a **byte-identical** sha256 (determinism).
- `0.2.0-rc.1` is a caller-supplied **test value**: this PR adds no version, and the entry has no default.

**Guards** (12 cases, all as expected): missing `--version` → 1; `0.1.0` → 1; `banana` → 1; `1.2` → 1; `01.2.3` → 1; unknown argument → 2; wrong `--source-commit` → 1; abbreviated commit → 1; missing receipt → 1; receipt that does not bind the packaged bytes → 1; a stale artifact paired with a real receipt → 1; dirty work root → 1. `--help` → 0.

## Gates

`lefthook run pre-push --all-files` in the WSL worktree, on the commit in this PR:

```
✔ commit-convention (0.08s)   ✔ vocabulary (0.94s)   ✔ plugin-boundary (7.97s)
✔ rust-fmt (0.53s)   ✔ rust-clippy (2.64s)   ✔ rust-test (140.13s)   ✔ release-policy (2.47s)
✗ docs-verify — NOT_RUN
```

`docs-verify` did **not** execute: `sh: 2: node: not found` / `docs:build prerequisite missing: website dependencies not installed. Run once: cd website, then npm ci` (exit status 1). This WSL VM has no node/npm and no network for `npm ci`, so the check is **NOT_RUN, never PASS**, and `lefthook` therefore exited 1. Its command globs are `docs/**` and `website/**`; this PR touches only `ops/**`, so a real push does not select it. Everything else in the gate really ran — including `release-policy`, which `ops/**` is **not** a glob for and which `--all-files` forced to run.

## NOT_RUN

- `docs-verify` (node/npm + `website/node_modules` unavailable in WSL, no network for `npm ci`).
- Runtime/device smoke of the packaged consumer example — no media input in this lane; `package-summary.json.not_run` records it.
- Two-root reproducibility rebuild — every manifest level is `NOT_ESTABLISHED`; nothing claims `RECIPE_RECONSTRUCTIBLE`, `SEMANTIC_ABI_VERIFIED` or `BYTE_REPRODUCIBLE`.
- A fresh FFmpeg closure derivation (`scripts/fetch-ffmpeg` + `ffmpeg_import`) — no network in this WSL VM; the already-imported closure was reused with its identity revalidated against the pin, recipe and profile.
- CI: no workflow invokes `ops/**`.

## Scope

- **This PR does not change the ABI.** `native/include/songcore.h` is untouched (sha256 `cf509a5c044735e4813642ac6683691242146e642342a8ac356e98fbd8e78536`), and check `V2_header_sha` asserts packaged == source-commit blob == contract §3.
- **This PR does not choose a version.** `--version` has no default, no version string is added to any file, and `0.1.0` is refused.
- **This PR grants no release authority.** No tag, no GitHub Release, no asset upload, no publish path; `docs/release/songcore-release-contract.md`, `native/xmake.lua` and all product code are untouched.
- `evidence/` is deliberately not part of this PR; the run receipts are archived separately.

## Follow-ups (out of scope here)

- `ops/**` is outside the `lefthook` release-policy globs and no CI job invokes it, so these entries still have no automated exercise.
- `ops/package-rc.ps1` still has no artifact-to-commit binding (a 2026-10-06 artifact packages under `git_sha=69ba1ce`); `ops/package-linux.sh` now supports `--build-receipt`, the Windows entry does not.
- `ops/build-rc.ps1:344` references `ops/verify-build.ps1`, which does not exist; `ops/run-csuite.ps1:52` and `ops/run-binding-gate.ps1:49` pin an artifact sha no build produces; `ops/verify-external.ps1` defaults point into the archived worktree.
