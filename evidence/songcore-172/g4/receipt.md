-----BEGIN-RECEIPT-----
BEGIN_SONGCORE_172_G4_PREFLIGHT

BASE_SHA=69ba1ce05a2772f5aa077c6ece3365c40f6e0500
CURRENT_MAIN_SHA=69ba1ce05a2772f5aa077c6ece3365c40f6e0500
CONTRACT_AUTHORITY=docs/release/songcore-release-contract.md @ 69ba1ce05a2772f5aa077c6ece3365c40f6e0500（#331 ratified）
TARGETS_IN_SCOPE=linux-x86_64(static+shared), windows-x86_64(static)

WINDOWS_ARTIFACT_READINESS=PASS
LINUX_ARTIFACT_READINESS=PASS
PACKAGE_LAYOUT=FIX_REQUIRED
MANIFEST_V1_V7=v1_set_arithmetic=PASS v2_header_sha=PASS v3_pin_closure=PASS v4_symbols=PASS v5_license=PASS v6_repro_preflight=PASS v7_tools_target_identity=PASS version_identity_s2=FIX_REQUIRED
LICENSE_O1_O4=O1 PASS — both packages ship COPYING.GPLv2 8177f975…, COPYING.GPLv3 8ceb4b9e…, COPYING.LGPLv2.1 246041b6…, COPYING.LGPLv3 da7eabb7…, each byte-identical to the member extracted from the pinned FFmpeg-n9.0.1.zip (sha256 d16837dd… = native/ffmpeg/pin.json source_sha256) and recorded in manifest.license.license_files (4/4, 0 mismatch). O2 PASS — THIRD_PARTY_NOTICES.txt present in both (linux 5da8e9f8…, windows 94777594…), listed in metadata/checksums.txt and manifest files{}, and `sha256sum -c metadata/checksums.txt` returns OK for all 14 (linux) / 13 (windows) entries. O3 PASS at the contract's literal level — `ar t lib/libsongcore.a` = 280 members on both targets, 100% .o/.obj incl. FFmpeg source-named objects and songcore_ffmpeg.c.o, and README.md:71-79 (linux) / :59-67 (windows) state the relink path — but the consumer-actionable relink procedure is absent, so the ask's stricter bar is FIX_REQUIRED (see o3_relink_executable_instructions). O4 PASS — 37 (linux) / 39 (windows) configure_args with 0 matches for ^--enable-(gpl|version3|nonfree|lib), gpl/version3/nonfree=false, external_libs=[].
LICENSE_B1_B4=B1 PASS — no license text missing and no sha256 differs from the pinned-source extraction (4/4 texts, both targets). B2 PASS — V5 inputs clean (no gpl/version3/nonfree/lib* arg in either manifest). B3 PASS — THIRD_PARTY_NOTICES.txt present and hashed in checksums.txt in both packages. B4 PASS — LICENSES/LICENSE-MIT e31ce84d… and LICENSES/LICENSE-APACHE a60eea81… equal `git show 69ba1ce05a2772f5aa077c6ece3365c40f6e0500:LICENSE-MIT` / `:LICENSE-APACHE` exactly (the repo-root LICENSE 3d44f3d4… is the dual-license pointer that names those two files).
RUNTIME_DEPENDENCIES=linux-x86_64 SHARED (libsongcore.so, sha256 085425e590b0d6d810192404162cef6f1f1f880f769e4afd12e4af5306078e98 = task input): ELF64 x86-64 DYN. DT_NEEDED = libm.so.6 + libc.so.6 ONLY (readelf -d); ldd in an isolated dir => linux-vdso.so.1, libm.so.6, libc.so.6, /lib64/ld-linux-x86-64.so.2. No libpthread/libdl/libgcc_s/av* NEEDED; NO FFmpeg dynamic dependency (no libav*/libsw*/libpostproc in NEEDED, no av*/sw* undefined dynamic symbols). Exactly 15 song* dynamic exports (nsong_audio_stream_count..songcore_abi_version), no other defined dynamic symbols; no SONAME, no RPATH/RUNPATH. Raw: evidence/songcore-172/g4/lane-e-runtime-deps/raw/{readelf-dynamic-so.txt,needed-libs.txt,ldd-isolated-so.txt,nm-D-defined-so.txt,nm-D-undefined-so.txt}.
linux-x86_64 STATIC (libsongcore.a, sha256 0d38f4b5a8bd812ba262ca2dc26c32f2d1a596511222d930e685db25d7361308 = task input): 280 ar members measured (ar t); the 280/280 ELF64 member census is as supplied by the build lane and was not independently re-run (the archive was deleted after measurement). Measured link line: `gcc -O0 -I<inc> src/abi-complete-consumer.c -L<art> -lsongcore -lm -lpthread -o static/abi-O0-libs` => exit 0. Without -lm: exit 1 (undefined cos/exp/atan from vorbisdec.c.o); with -lpthread only: exit 1; with -lm only: exit 0 (pthread_* resolve from libc.so.6 on this host; libpthread.so.0 measured as placeholder-only: 24 defined dynsyms = 12 version ABS + 12 __libpthread_version_placeholder aliases, no real functions). No -ldl/-lrt needed. Method note: the archived evidence/songcore-172/audit/abi-complete-consumer.c:6-25 at -O2 is optimized to a 15,976-byte binary that pulls no archive members (nm shows only main; it links even without -lm); at -O0 it forces the closure (raw/probe-elision-proof.txt).
Host-run consumers (build host glibc 2.43): static and shared external-consumer-example.c (evidence/songcore-172/linux/verify/package/external-consumer-example.c:76-160) both ran open->probe->decode 176400 frames->seek (landing 1985306us vs target 2000000us)->clean SONG_EOF->close, exit 0.
Provenance caveat: /home/hoo/g4-172/linux-root was deleted and recreated by another process at ~20:51 after all artifact-dependent measurements; identity is pinned by the sha256 match to the task inputs and by preserved copies (raw/post-measurement-state.txt).
GLIBC_MEASURED_FLOOR=GLIBC_2.43. Command: `objdump -T /home/hoo/g4-172/linux-root/src/native/build/artifacts/shared/libsongcore.so | grep -o 'GLIBC_[0-9.]*' | sort -uV` -> last line GLIBC_2.43; `readelf --version-info` lists GLIBC_2.43 under 'File: libm.so.6'. Full required set: GLIBC_2.2.5, 2.3.2, 2.3.4, 2.4, 2.6, 2.7, 2.14, 2.17, 2.27, 2.28, 2.29, 2.34, 2.35, 2.43. Forcing symbols: atan2f@GLIBC_2.43 (libm.so.6), hypot@GLIBC_2.35 (libm.so.6), pthread_create/join/once@GLIBC_2.34 (libc.so.6). Non-GLIBC versioned deps: NONE - GLIBCXX_/CXXABI_/GCC_/GOMP_/LIBATOMIC_/GLIBC_ABI_ grep exit 1; the only namespace in the dynsym version tags is GLIBC. Raw: raw/objdump-T-so.txt, raw/readelf-version-info-so.txt, raw/glibc-versions.txt, raw/glibc-highest.txt, raw/non-glibc-explicit-check.txt.
CONSUMER_STATIC=PASS（Lane C (linux) ran end-to-end against ONLY the unpacked package (WSL-native /home/hoo/g4-172/laneB-stage/linux/songcore-v0.0.0-preflight-linux-x86_64; 15 files;）
CONSUMER_SHARED_LINUX=PASS
CONSUMER_WINDOWS_PE=PASS
RELEASE_POLICY=Tag/version identity and no-overwrite, measured against tools/check_release.py, docs/releasing.md and the live scripts at 69ba1ce.

Identity: R1 restricts the product tag namespace to `qianqian-vMAJOR.MINOR.PATCH` with no leading zeros (tools/check_release.py:74-76) and explicitly says `songcore-v*` is not a product tag; the version must equal `[workspace.package].version`, the package name and an exact dated CHANGELOG heading (check_release.py:90-94). R2 requires a full 40-hex source, `git rev-parse HEAD == --source`, a fully clean tree including non-ignored untracked files, and in formal mode a tag whose peeled commit equals the source (check_release.py:81-89). R5 binds the ZIP to an external `.sha256` whose content must equal `sha256  filename` and to a provenance JSON, and re-hashes every payload and re-reads the shipped documentation from the source commit (check_release.py:181-219).

Direct probes I ran (evidence/songcore-172/g4/release-policy/publish-policy-probes.log): `check_release.py publish-policy --event push --ref refs/tags/songcore-v0.1.0 --tag songcore-v0.1.0` -> exit 2 `POLICY_RED: R1 invalid product tag (SongCore tags are separate)`; `--tag qianqian-v0.1.0 --existing` -> exit 2 `R7 existing/historical release cannot be replaced`; `--event workflow_dispatch` -> exit 2 `R7 publication requires actual tag push`; a never-used `qianqian-v9.9.9` push -> exit 0 PASS (policy only, creates nothing); `songcore-v0.2.0-rc.1` -> exit 2 R1.

No-overwrite design: publication_policy() refuses every existing Release including the historical 0.1.0 (check_release.py:221-224); publish-qianqian.py re-queries the releases list and re-checks, re-reads the remote peeled tag with `git ls-remote` and requires it to equal --source (publish-qianqian.py:31-35), then creates with `gh release create ... --verify-tag --target <source>` (publish-qianqian.py:41-44). A repo-wide grep of tools/, ops/ and .github/workflows finds no `--clobber`, `release edit`, `release upload` or `release delete` anywhere, so there is no code path that can mutate a published tag, release or asset. The publish job is gated on `github.event_name == 'push' && startsWith(github.ref, 'refs/tags/qianqian-v')` with `contents: write` only there, while build stays `contents: read` (release-qianqian.yml:23-24, 101-105), and docs/releasing.md:45-49 states local/dispatch/PR runs cannot publish (E16-local-publish in the E01-E18 run: `release-qianqian.sh publish` -> exit 2 POLICY_RED).

Conclusion for SongCore: the product-side release machinery cannot serve songcore-v* by construction (hardcoded `qianqian-v` regex, `qianqian.exe` payload, Windows-PE-only target and import allowlist at check_release.py:18-28), and no separate SongCore publisher exists.
DOCS_VERIFY=NOT PASS — verify.mjs green, VitePress build red on this host, and the literal documented route does not exist.

1) The asked-for route: `cmd //c 'tools\docs-verify.cmd .'` -> exit 1, output `'tools\docs-verify.cmd' 不是内部或外部命令...`. The file is absent from the tree at 69ba1ce (`git ls-files | grep -i docs-verify` empty; whole-worktree `find -name docs-verify*` empty), although AGENTS.md:450 documents `cmd /c <tools>\docs-verify.cmd <worktree-root>` as the Windows route. Log: docs-verify-literal-route.log (a580f565…).

2) I therefore ran the gate body from lefthook.yml:134-150 (`cd website && node scripts/verify.mjs && npx vitepress build`), after confirming website/node_modules exists (lockfile package entries 292, 148 top-level dirs; node v24.19.0 / npm 11.17.0; no npm ci needed).
- `node scripts/verify.mjs` -> exit 0, summary `Errors: 0  Warnings: 0  VERDICT: PASS` (docs-verify-verify.mjs.log fd5716d5…).
- `npx vitepress build` -> exit 1. Failed in `building client + server bundles...` with `[vite:esbuild-transpile] remove C:\Users\fred1\AppData\Local\Temp\esbuild-<hash>: Access is denied.` (docs-verify-vitepress-build.log 445fe747…). Retried twice: with TMP/TEMP redirected to a private dir (same error, 87e0ed3d…) and with the tool sandbox disabled (same error, fb6ae58f…). Leftover esbuild temp bundles remained on disk after each attempt, while plain bash `rm` and node `fs.unlinkSync` in the same directory succeed — so this is a host/file-handle condition in the esbuild child, not a docs content error (VitePress emitted no markdown/config diagnostic). Build never completed -> the VitePress half of docs-verify is NOT a pass on this host, and needs a rerun on CI or a clean host.

This contradicts the AGENTS.md:457-462 record of a docs-verify PASS on 2026-10-30; I report the measured result.
E01_E18=PASS, really run in WSL.

argv: `wsl.exe -d Ubuntu-24.04 -- bash /mnt/c/Users/fred1/source/qianqian/evidence/songcore-172/g4/release-policy/run-e01-e18.sh`, whose inner command is `python3 tools/test_release.py --report <evidence>/release-controls.json` with cwd `/home/hoo/Source/qianqian` (repo at 69ba1ce05a2772f5aa077c6ece3365c40f6e0500, clean).
exit_code: 0. stdout: `PASS: E01–E18; real PE controls; original HEAD/status/tool bytes unchanged`. Start/end 2026-10-30T12:30:14Z..12:30:18Z (~4 s).
case count: 69 recorded subprocess executions across 44 distinct labels covering the E01-E18 families (E01, E02, E03, E04-tracked, E04-staged, E05, E06, E07, E08, E09-<5 files>, E10-<4 traversal/alt-path variants>+symlink, E11, E12+2 missing-sidecar, E13+api-set-spoof, E14-hash/real-ELF/target, E15-missing-tool/zip/json, E16 x5 incl. real publisher, E17, E18 + hook-repository isolation), read from evidence/songcore-172/g4/release-policy/release-controls.json (sha256 4fac2b0e903a1f7236ad1bc7c1f4385762e4a9a9ab507090e3878375f9925224).
Tooling: WSL Ubuntu-24.04 python3 3.14.4 with gcc/objdump/ar/x86_64-w64-mingw32-gcc and the committed ELF fixture; no Windows-side python3 used (WindowsApps stub).
Evidence: evidence/songcore-172/g4/release-policy/{e01-e18.exit.txt (f50c7fbc…), e01-e18.log (16a7d6d4…), release-controls.json, run-e01-e18.sh}.

D1_DISPOSITION=Spec/header-text defect (not an implementation bug). native/include/songcore.h:218 says of the listed song_open failures "*out_handle is then untouched"; the shipped implementation writes NULL at songcore_ffmpeg.c:825 before every post-guard failure, so the sentence is false for two of the three documented classes and true only for a path the header never mentions. The implementation's fail-closed NULL is strictly safer for callers and is already in every built artifact on both release targets. Independently confirmed at source SHA 69ba1ce: linux-x86_64 executed (12/12 probe cases MATCH, exit 0), windows-x86_64/mingw disassembled (build exit 0).；UNDECIDED
D3_DISPOSITION=Code bug；UNDECIDED

Q1=OWNER_UNDECIDED
Q2=RECORDED_ACCEPTANCE_PENDING_FORMAL_CLOSEOUT
Q3=OWNER_UNDECIDED
Q4=OWNER_UNDECIDED
Q5=OWNER_UNDECIDED

PREFLIGHT_VERDICT=ENVIRONMENT_BLOCKED
OWNER_DECISION_PACKET=evidence/songcore-172/g4/G4-OWNER-DECISION-PACKET.md
REVIEW=A=REVISE/8issues, B=REVISE/7issues, C=REVISE/7issues
NEW_RC=NOT_AUTHORIZED
TAG=NOT_CREATED
GITHUB_RELEASE=NOT_CREATED
G4=OWNER_DECISION_REQUIRED

END_SONGCORE_172_G4_PREFLIGHT
-----END-RECEIPT-----
