# S3 verdict — Linux x86_64 SongCore reference rebuild (#172)

```text
TRUTH CLASS   Execution evidence for songcore-172 S3. Every claim below was
              executed/read during the S3 session on 2026-10-29 (run stamps
              20261028T200502Z / 20261028T200519Z UTC). It is not an ADR and
              grants no maturity above what the listed checks support.
VERDICT       PASS (for the S3 scope: Linux x86_64 reference rebuild under the
              same release contract as the windows-x86_64 route — clone,
              FFmpeg closure import, build, ELF/ABI identity, provenance,
              replayable entry, evidence sync). See NOT VERIFIED for what
              this PASS does not cover.
```

## 1. Source identity

```text
branch        songcore-172/pr-a-build-fixes
HEAD          bf23516b5851174dc63fb5058997fa5478751713
tree          3247b280f26efe6bc4174307758d791100c12b0d
git fsck      clean (clone-fsck.txt, --full --no-dangling, exit 0)
tree verify   git archive HEAD | sha256sum:
              f8ce88a55f674bc288652f870e30f32ca2249bda8733fef27e9b98e8e9cfc0b6
              — identical from the Windows worktree
              (C:\Users\fred1\source\qianqian-172) and the WSL clone
              (~/qianqian-172-linux) → cross-boundary tree match
```

Clone differential (recorded, not hidden): the campaign worktree's `.git`
gitfile carries a Windows-absolute gitdir
(`C:/Users/fred1/source/qianqian/.git/worktrees/qianqian-172`) that Linux git
cannot resolve, so a direct clone of
`/mnt/c/Users/fred1/source/qianqian-172` fails on WSL ("not a git repository:
…/qianqian-172/C:/Users/…", preserved in run1-derive/s3-clone-1.log). The clone
was taken via `git clone --no-local file://…` from the owning repository
`/mnt/c/Users/fred1/source/qianqian` (same object store and refs as the
worktree; branch ref verified present there before cloning). HEAD + tree
digest above prove identical content. `clone-source.txt` records this.

## 2. FFmpeg closure (linux-x86_64 recipe)

```text
recipe        native/ffmpeg/targets/linux-x86_64.json
              sha256 79c307e731427bf1d94ab2ed43d7729c91d1d747a02392ee84170616467cdb6d
              artifacts policy as declared: static=true, shared=true, wasm=false
profile       codec-base, sha256 57fb70ed2c29a8464d939c611b3c5ed1bc99879438eb7f159530dfa52de554fa
pin           n9.0.1 @ commit bf1b838f2ab88b4f8fd83443325c782ea0e0f7fa
              zip sha256 d16837ddbb0963753aa2739971a09b07fe14fe855a6e84f253f851127f99c746
fetch         scripts/fetch-ffmpeg: live tag recheck OK
              ("tag recheck ok: n9.0.1 -> bf1b838f2ab88b4f8fd83443325c782ea0e0f7fa")
              + zip sha256 verified before extraction (fetch-ffmpeg.log).
              Download needed the host's local proxy (WSL direct egress to
              github.com fails; mirrored networking + 127.0.0.1:7897), used
              via QIANQIAN_FETCH_PROXY — recorded, not hidden.
import        python3 tools/ffmpeg_import.py --recipe linux-x86_64 (exit 0):
              FFmpeg configure + make -j20 V=1 upstream oracle, closure frozen
              to build/ffmpeg-xmake/manifest.json (schema 2):
              279 translation units (0 generated)
              target  {id linux-x86_64, platform linux, arch x86_64,
                       target_os linux, cross_prefix null, recipe_sha256 79c307e7…}
              toolchain {system linux, machine x86_64, cc gcc,
                         cc_ident "gcc 15 (Ubuntu 15.2.0-16ubuntu1)"}
manifest sha  d037e8b472d247cf457b1af204546c5feb66798f9f44aec3f96b54663cba5525
```

## 3. Build (xmake, host-native gcc)

```text
configure     xmake f -p linux -a x86_64 -m release -y   exit 0
build         xmake build songcore                      exit 0
              (phony resolved both forms from the recipe: archiving+merging
               libsongcore.a, linking libsongcore.so)
explicit      xmake build songcore_static               exit 0
              xmake build songcore_shared               exit 0
              — the fail-closed recipe artifact-policy gates (native/build/
               songcore.lua, #172 D2 fix) accept both forms on linux-x86_64
toolchain     gcc (Ubuntu 15.2.0-16ubuntu1) 15.2.0, xmake v2.9.7+20250101,
              Ubuntu 26.04.1 LTS (WSL2, registered name Ubuntu-24.04),
              kernel 6.18.54.1-microsoft-standard-WSL2, nproc 20
```

## 4. Artifact identity (ELF + SHA256)

```text
static   /home/hoo/qianqian-172-linux/native/build/artifacts/libsongcore.a
         4,067,628 bytes
         sha256 409ef3da505f94d559a5fb5cb042fccd225d52238a00bd50722dd5f2669dd0f3
         "current ar archive" (file); objdump -f: 280/280 members
         "file format elf64-x86-64" (279 closure TUs + songcore_ffmpeg.o)
         nm: 15 song* T symbols == SONGCORE_API set of native/include/songcore.h
shared   /home/hoo/qianqian-172-linux/native/build/artifacts/shared/libsongcore.so
         2,288,536 bytes
         sha256 5e9c56972bc063e0020750d562225dcb8a3ac3c10df3957688bb86ebd9a12843
         "ELF 64-bit LSB shared object, x86-64 … dynamically linked, stripped"
         readelf -h: Class ELF64, Type DYN, Machine Advanced Micro Devices X86-64
         readelf -d NEEDED: libm.so.6, libc.so.6 — no libav*/libswresample
         (closure internalized via -Wl,--exclude-libs,ALL + hidden visibility)
         nm -D: 15 song* dynamic exports == SONGCORE_API set of songcore.h
closure  native/build/artifacts/libqianqian_av.a 4,033,040 bytes (replay archive)
stable   src/songcore_ffmpeg.c sha256
         55eab2432bf2f1959fe13b3f97282895b5918983925f175cad521b0ae5c05ad6
         — equals the stable source identity recorded by the #324 Windows
           campaign (WORKBRIEF §4), same commit lineage
ABI set   (both forms, exact set equality with songcore.h, diff files empty):
         song_open song_close song_probe song_read_pcm song_seek
         song_last_error song_select_stream song_audio_stream_count
         song_audio_stream_info song_get_metadata song_get_metadata_count
         song_get_metadata_entry song_get_artwork_count song_get_artwork_item
         songcore_abi_version
```

The artifact sha256 values are identity RECORDS of this reference rebuild, not
cross-machine byte-reproducibility claims (debug/BuildID/path inputs vary by
tree; #324 documented the same for the Windows route).

## 5. Replayable entry (gate contract)

```text
path          /home/hoo/qianqian-172-linux/ops/build-linux.sh
              sha256 fa4dcf8c233892976755074c8a49572633de75dbf0c4d610dba5cd85d788a724
contract      5 stages, set -Eeuo pipefail + ERR trap → FAIL summary + real
              nonzero exit; stdout = stage lines + one JSON summary
              (asserted < 64KB in-script); all logs land in
              ops/build-evidence/<UTC stamp>/
re-entrancy   reuses the clone when HEAD == pinned commit and the tracked tree
              is clean; reuses the imported closure only when manifest + pinned
              source stamp + oracle config exist AND manifest identity matches
              host + current recipe (revalidated every run; the Xmake-side
              fail-closed target gate re-checks at every build regardless)
runs          20261028T200502Z: clone=reuse, closure=reuse → PASS, exit 0
              20261028T200519Z: clone=reuse, closure=reuse → PASS, exit 0
              (the derive run that first produced the closure is archived
               under run1-derive/ with its fetch/import/xmake logs)
```

## 6. Evidence sync (WSL → Windows, sha-verified)

```text
destination   C:\Users\fred1\source\qianqian-172\evidence\songcore-172\linux\build\
scope         55 files (run1-derive/, run-20261028T200502Z/,
              run-20261028T200519Z/, entry-script/, manifests)
check 1       per-file sha256 on WSL ext4 before copy == after read-back from
              /mnt/c (55/55 match) → sha256-manifest.txt
check 2       independent native-Windows read (Git Bash sha256sum) of the same
              55 files == WSL manifest (55/55, after normalizing MSYS's
              binary-mode `*` separator) → verify-copy-sha256-win.txt
```

## 7. NOT VERIFIED / NOT CLAIMED (fail-closed)

```text
- CORE_RUNTIME / BINDING_RUNTIME maturity (#173 vocabulary): NOT claimed.
  No C consumer, no decode/seek/EOF runtime scenario, no audio device smoke
  ran against these artifacts in S3. This evidence supports BUILD/LINK-class
  claims only (artifact exists, ABI export surface audited, link contract
  files present for later gates).
- Rust binding (qianqian-songcore-sys) consumption of this Linux artifact:
  not run in S3 (out of the S3 build-contract scope as tasked).
- Byte-level cross-machine reproducibility: not claimed (single reference
  rebuild recorded; see §4).
- Windows-x86_64 route: not re-run here (that is #324 evidence; S3 did not
  touch it, and windows shared=false policy was not exercised here).
- Historical songcore-v0.1.0 release: untouched (nothing under any release
  namespace was modified; all writes were evidence copies).
- TLA+/TLC, Kani/Loom/Miri, mutation: not run (not in S3 scope).
- lefthook pre-push: not run (no commits created by S3).
```

## 8. Commands of record (abbreviated)

```text
wsl.exe -d Ubuntu-24.04 -- bash <driver>   (drivers in the session handoff dir;
  inline multi-line payloads through wsl.exe are forbidden per
  ops/verify-build.ps1's own lesson — stage scripts ran as LF files)
git clone --no-local file:///mnt/c/Users/fred1/source/qianqian-172 \
  → fell back to file:///mnt/c/Users/fred1/source/qianqian
bash scripts/fetch-ffmpeg                       # in native/, proxy env
python3 tools/ffmpeg_import.py --recipe linux-x86_64
xmake f -p linux -a x86_64 -m release -y && xmake build songcore
file/readelf/objdump/nm/ar audits               # see run-*/ evidence files
/home/hoo/qianqian-172-linux/ops/build-linux.sh # x2 (re-entry contract)
```
