# G4 / P0 — governance archive landings & songcore-v0.1.0 history integrity

Lane: `evidence/songcore-172/g4/p0-governance/`
Source SHA: `69ba1ce05a2772f5aa077c6ece3365c40f6e0500`
Run date: 2026-10-30 (UTC). Read-only: no tag, no release, no asset, no repo
file outside this directory was created or modified (`git status --porcelain
--untracked-files=no` → 0 lines after the run).

## 1. #330 differential dispositions in `docs/release/songcore-release-contract.md`

| Item | State | Where |
|---|---|---|
| D1 `song_open` "*out_handle is then untouched" vs code | `needsOwnerDecision` (recorded only) | `docs/release/songcore-release-contract.md:855` |
| D2 `files{}` vs `checksums.txt` set arithmetic | **fixed in place in the body** | §8 `:399-405`, V1 `:423-431`; §16 D2 `:883`, conclusion `:911-914` |
| D3 `song_read_pcm` produced-not-zeroed guard paths | `needsOwnerDecision` (recorded only) | `docs/release/songcore-release-contract.md:917` |

Cross-checked against the cited source lines (all read this run):

```text
native/include/songcore.h:218          "*out_handle is then untouched"         (D1 header claim)
native/src/songcore_ffmpeg.c:823-825   guard, then `*out_handle = NULL;`       (D1 code)
native/include/songcore.h:358          "other -> ... == 0 in that call"        (D3 header claim)
native/src/songcore_ffmpeg.c:1111-1114 !h -> return / !dst -> fail_arg / then zero (D3 code)
```

No code or header text was changed for D1/D3:

- `native/include/songcore.h` blob `3055ca5cdff7c4d40def9e99883cb3bead94c417`
  is identical at tag `songcore-v0.1.0` (`include/songcore.h`) and at HEAD
  (`native/include/songcore.h`) — `git diff` empty.
- Working-tree `sha256sum native/include/songcore.h` =
  `cf509a5c…78536` = the §3 frozen value.
- `git blame` of every D1/D3 cited line → `858f724` (2026-09-12), well
  before the #330 review.
- `git show --numstat 0b7c5e3` (#331) touches zero files under `native/`;
  `git show --numstat 69ba1ce` (#332) touches `native/build/ffmpeg.lua`
  only, not `native/include/` or `native/src/`.

## 2. songcore-v0.1.0 published release

Read via authenticated GitHub REST as `jnhu76` (`/user` → `jnhu76`,
id 5766215; repo `permissions.admin=true`), which is the same account and
the same data `gh release view` reads. **The `gh` CLI itself could not run**:
the WSL VM has no network interface (`ip addr` shows only `lo`;
`ping 223.5.5.5` → "Network is unreachable"; `gh release view` → exit 1
"error connecting to api.github.com"). See `audit_raw.log`.

```text
tag_name        songcore-v0.1.0
name            SongCore v0.1.0
published_at    2026-09-02T13:07:42Z      created_at 2026-09-02T13:07:05Z
draft/prerelease false / false
asset           songcore-v0.1.0-linux-x86_64.tar.gz
                size 2588376
                digest sha256:018e0cbdfd8d83b6967bbba8e899ae6dbcc2ccf690c5294bd48e2ef2699f6815
                created_at 2026-09-02T13:07:40Z  updated_at 2026-09-02T13:07:41Z
tag object      a1b6c996fcb5d6c7dfea965a11bf3f05b94943fb  (`git cat-file -t` → tag)
tag commit      522c16d283f024c01d1ef56fa96e587b286ca3b8  (2026-09-02 21:06:03 +0800)
```

Immutability signals: the asset's `updated_at` is one second after
`created_at` (never re-uploaded); the release's `updated_at` equals
`published_at`; the local historical tarball
`/home/hoo/s2pkg-v010/songcore-v0.1.0-linux-x86_64.tar.gz` is
2 588 376 bytes with sha256 `018e0cbd…6815` — byte-identical size and hash
to the published asset, and equal to the contract's frozen S-V010 value
(`docs/release/songcore-release-contract.md:67-68`).

## 3. songcore tag / release inventory

```text
git tag -l 'songcore-v*'                      → songcore-v0.1.0   (1)
remote refs/tags/songcore-v* (matching-refs)  → songcore-v0.1.0   (1), object a1b6c99…
releases total (authenticated, incl. drafts)  → 2
    qianqian-v0.1.0  (2026-10-06)  songcore-v0.1.0 (2026-09-02)
    drafts: none
songcore-v* releases                          → 1
page 2 of ?per_page=100                       → [] (list complete)
```

## 4. Issue/PR landings on main (`69ba1ce`)

```text
#326  43d5e96a8d685db142be63be1648f60f6355a529  fix(songcore): correct Windows static target builds… (#326)
#327  742abf87ef2a832b653b17760d8c2a55008cb1c1  build(songcore): establish Windows x86_64 RC lane (#327)
#328  b49b18cd68ae1a353cd3396efb3977b8590cc4d1  build(songcore): archive Linux S3 reference evidence (#328)
#330  (issue, not a PR: pulls/330 → 404) closed 2026-10-29T19:04:21Z, resolved by PR #331
#331  0b7c5e3b451a950554c886a9017ef50eeb66dd4e  docs(release): ratify S1 contract … (#331)
#332  69ba1ce05a2772f5aa077c6ece3365c40f6e0500  build(native): map ffmpeg macro roots … (#332)
```

Every commit is `git merge-base --is-ancestor` of HEAD, and each GitHub
`merge_commit_sha` equals the local commit id (see `pr-merge-check.log`).

## 5. Ratification landing points (#331)

```text
docs/release/songcore-release-contract.md:4      TRUTH CLASS OWNER-RATIFIED repository authority
docs/release/songcore-release-contract.md:13-16  "Ratified 2026-10-30 … by the explicit approval PR reviewed under Issue #330"
docs/release/songcore-release-contract.md:776-780 §14 pointer: #330 differentials are owner-reserved
docs/release/songcore-release-contract.md:842    §16 differential dispositions heading
docs/README.md:17                                routing row marked **OWNER-RATIFIED** …
docs/architecture/registry.yml:225-231           "Owner-ratified 2026-10-30 … reviewed under Issue #330"
docs/architecture/registry.yml:236-241           authority + decisions: issue 172, issue 330
```

## Not run / caveats

- `gh release view|list|api` in WSL: NOT_RUN — no network interface in the
  WSL VM. Facts obtained through the same GitHub API with the same account
  token (curl config file, never on argv; temp credential files deleted,
  `auth-read.log`). Authenticated and anonymous payloads are byte-identical
  (`anon vs auth release object identical: True`).
- Re-downloading the published tarball to re-hash it: NOT_RUN on purpose —
  the local historical copy already equals the published asset digest.
- Any fix for D1/D3: NOT_RUN by design (owner-reserved; no coding in this lane).
