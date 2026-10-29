# SongCore v1 static/shared link contract — system-dependency audit (#172 D2)

Date: 2026-10-29. Author: D2 fix session, working in worktree
`qianqian-172` branch `songcore-172/pr-a-build-fixes`.
Every claim below was measured in this session; the raw evidence files are
listed per section. Artifacts audited are the GREEN products of the D2 route:
`native/build/artifacts/libsongcore.a` (merged FFmpeg closure + songcore,
280 members, sha256 `91291a37…7ae867`, windows-x86_64 pe-x86-64 replay) and
`native/build/artifacts/shared/libsongcore.so` (linux-x86_64 ELF replay).

## 1. What the merged static archive actually requires

`nm -A --undefined-only` over the merged archive
(`audit/win-undefined-by-member.txt`, 3150 rows; unique externals
`audit/win-external-census.txt`, 1185 symbols) classifies into:

| class | examples | provided by (windows-x86_64) |
|---|---|---|
| CRT (libc/allocator) | `malloc`, `free`, `memcpy`, `memset` | msvcrt — mingw default link spec, no explicit `-l` |
| Win32 API | `ReleaseSRWLockExclusive` | kernel32 — mingw default link spec |
| **BCrypt\*** | `BCryptOpenAlgorithmProvider`, `BCryptGenRandom`, `BCryptCloseAlgorithmProvider` | **`-lbcrypt` — NOT in any default spec; must be explicit** |

BCrypt ownership (measured, `audit/win-undefined-by-member.txt`):

```
libsongcore.a:random_seed.c.obj: U BCryptOpenAlgorithmProvider
libsongcore.a:random_seed.c.obj: U BCryptGenRandom
libsongcore.a:random_seed.c.obj: U BCryptCloseAlgorithmProvider
```

`random_seed.c.obj` is FFmpeg `libavutil/random_seed.c` (FFmpeg n9.0.1,
pin `bf1b838f…`), whose `av_get_random_seed()` uses the BCrypt RNG path under
mingw. It is the **only** member referencing BCrypt\*.

## 2. Windows static consumer contract (measured)

```
x86_64-w64-mingw32-gcc consumer.c -lsongcore -lbcrypt   # exit 0
x86_64-w64-mingw32-gcc consumer.c -lsongcore            # exit 1:
    random_seed.c:(.text+0x24): undefined reference to `BCryptOpenAlgorithmProvider'
    random_seed.c:(.text+0x3b): undefined reference to `BCryptGenRandom'
    random_seed.c:(.text+0x49): undefined reference to `BCryptCloseAlgorithmProvider'
```

(`audit/win-link-bcrypt.log` empty = zero undefined refs; `audit/win-link-nobcrypt.log`;
probe source `audit/abi-complete-consumer.c` — takes the address of all 15
`song*` ABI symbols, forcing every defining member to be pulled.)

Two facts sharpen this into an unconditional requirement:

1. The failure is not usage-dependent: a **minimal** consumer that references
   only `songcore_abi_version()` still fails without `-lbcrypt`
   (`audit/win-link-minimal.log`). Pulling any SongCore member drags enough
   of the libavutil closure to pull `random_seed.c.obj` with it. Do not
   reason that "if you don't touch the RNG you don't need bcrypt" — the
   linker's member-level pull-in reaches it through ordinary ABI use.
2. With `-lbcrypt` the ABI-complete link resolves completely (exit 0). No
   other system `-l` is needed on windows-x86_64: CRT + kernel32 come from
   the mingw spec, and the census shows no winmm/user32/ole32-class refs.

**Windows static contract: `-lsongcore -lbcrypt`. Nothing else.**

The current Rust consumer does not violate this — but it satisfies it
indirectly: `crates/qianqian-songcore-sys/build.rs` emits only
`cargo:rustc-link-lib=static=songcore` for windows targets (build.rs:193-200),
and the `-lbcrypt` arrives from the repo-wide `.cargo/config.toml`
`rustflags = ["-C", "link-arg=-lbcrypt"]` for `x86_64-pc-windows-gnu`, whose
comment attributes the flag to *rustc's own windows-gnu runtime* (`random_seed.o`
inside rustc). Both consumers of the flag are real and independent:

- rustc's windows-gnu runtime references BCrypt\* (the flag's stated reason);
- the SongCore merged archive references BCrypt\* (this audit's finding).

Risk note for #172 release engineering: if `.cargo/config.toml` ever drops
that flag believing it is only for rustc's runtime, `qianqian-songcore-sys`
windows-gnu builds will die on exactly the three BCrypt symbols above.
Either keep the flag for both reasons, or move the Windows-specific
`cargo:rustc-link-lib=bcrypt` into build.rs — build.rs was intentionally not
modified in D2 (D1 owns it this campaign; the recipe-side fix scope is
native/build/songcore.lua).

The (unsupported) Windows shared form would need the same `-lbcrypt` plus the
`SONGCORE_BUILD_SHARED`/`__declspec(dllexport)` surface; its link failure in
the pre-fix build was exactly these three symbols
(`build/red-windows-songcore-build.log`). See §4.

## 3. Linux static consumer contract (measured)

`nm -A --undefined-only` over the linux replay archive
(`audit/lin-external-census.txt`, 1172 unique externals) shows:

- libm-class math (`cos`, `exp`, `atan`, `log2`, … — pulled by e.g.
  `vorbisdec.c`): without `-lm` the link fails
  (`audit/lin-link-nomath.log`: `undefined reference to 'cos'/'exp'`).
- pthread-class refs: 14 symbols (`pthread_cond_*`, `pthread_create/join`,
  `pthread_mutex_*`, `ff_pthread_init/free`,
  `audit/lin-external-census.txt`). On glibc ≥ 2.34 these resolve from libc
  itself; on older glibc they need `-lpthread`. Keep `-lpthread`: it is a
  stub no-op on new glibc and required on old.

```
gcc consumer.c -lsongcore -lm -lpthread   # exit 0 (audit/lin-link-libs.log)
gcc consumer.c -lsongcore -lpthread       # exit 1: cos/exp undefined (lin-link-nomath.log)
gcc consumer.c -lsongcore                 # exit 1: cos/exp/atan undefined (lin-link-nolibs.log)
```

**Linux static contract: `-lsongcore -lm -lpthread`.** This is what
`build.rs` emits for non-Windows targets (build.rs:197-199) and what
`songcore_common()` declares via `add_syslinks("m", "pthread")` for ELF
platforms — consistent.

## 4. Shared form (linux-x86_64: enabled; windows-x86_64: disabled)

The shared artifact links the same merged archive; on ELF the closure is
forced local with `-Wl,--exclude-libs,ALL` (`native/build/songcore.lua`), so
the exported surface is exactly the ABI: `nm -D` on
`shared/libsongcore.so` counts **15** exported `song*` symbols
(`build/postfix-so-songcount.txt`), matching the 15-symbol audit in
`native/ffmpeg/targets/linux-x86_64.json` evidence.

Per the recipes (`native/ffmpeg/targets/*.json`), `artifacts.shared=false` on
windows-x86_64: the form is not built by `xmake build songcore`, and an
explicit `xmake build songcore_shared` fails closed with the recipe verdict
(`build/green-shared-explicit.log`). If a future recipe amendment enables
Windows shared, this audit is its link-contract input: the DLL link must add
`-lbcrypt` (syslinks) or it will die exactly like the pre-fix RED.

## 5. Replaying the audit

Commands (WSL Ubuntu-24.04, xmake v2.9.7, gcc 15.2.0 host /
x86_64-w64-mingw32-gcc 13-win32 cross): `build/link-contract-audit.sh` +
`build/link-contract-audit2.sh` (build route:
`build/linux-import-route.sh`, `build/green-windows-route.sh`,
`build/linux-route.sh`). All scripts are the exact ones executed for this
evidence; no output was hand-edited.
