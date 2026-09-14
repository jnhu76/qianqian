# NATIVE-BOUNDARY-AUDIT-0 — Report

> STATUS: EVIDENCE. This report records what was checked, against which
> contract, on which host, with what result. It claims exactly what ran —
> nothing more. It is not architecture authority and it does not promote
> any new semantic, primitive, or lifecycle rule (verification-authority
> boundary, AGENTS.md).

```text
AUDIT_BASE_SHA:   c3064782d68020b46bb2babcc8e80fd5c02c4367 (main, expected base)
BRANCHES:         corrective/native-boundary-songcore-1
                  corrective/native-boundary-wasapi-1
                  phase-f/f1-cli-stop (Part B, separate PR)
                  gate/f1-integration (local integration of the above for
                  the Windows reality gate; evidence only, not a deliverable)
HOSTS:            WSL2 (Linux 6.18.33.2-microsoft-standard-WSL2, x86_64;
                  rustc 1.97.1) — construction console
                  Windows native (x86_64-pc-windows-msvc, rustc 1.98.1,
                  real Realtek audio endpoint) — reality gate, via a
                  temporary native checkout at C:\qianqian-gate pinned to
                  the integration SHA (same source commit, different host
                  execution; no \\wsl.localhost build)
SONGCORE_ARTIFACT (Linux):  native/build/artifacts/libsongcore.a
SONGCORE_ARTIFACT (Windows): native/build/artifacts-mingw/libsongcore.a
                  (sha256 17323291fc7b5399...a2a, symbol table verified:
                  14 song_* exports + songcore_abi_version present)
NATIVE CONTRACT:  native/include/songcore.h (ABI v1, in-repo) — the
                  normative FFI contract; native/src/songcore_ffmpeg.c —
                  implementation evidence
```

Scope: two boundaries — (A1) the SongCore C ABI as consumed by
`qianqian-decode-songcore` / `qianqian-songcore-sys`, and (A2) the
Windows WASAPI/COM/real-device boundary in `qianqian-output-wasapi`.
Audit first, correctives separate, no rewrites.

---

## PART 1 — SongCore FFI contract audit (A2)

### Contract map (functions consumed by the wrapper)

Fields abbreviated; the full column set of the audit plan is collapsed
into the notes. "Header" = native/include/songcore.h.

```text
FUNCTION             CONTRACT (header)                      RUST USE                     VERDICT
songcore_abi_version returns version, any time               bind-time identity check     PASS
song_open            err: *out_handle untouched;             handle + File ownership      S2 (below)
                     no diagnostic (no handle)               transferred on SONG_OK
song_probe           idempotent after probe; selects         format read once at open     PASS
                     default stream; rate>0, ch>0 validated  (info i32->u32/u16 casts
                     by the impl before publishing           are contract-backed:
                     (impl song_probe validates              impl refuses rate/ch <= 0,
                      sample_rate<=0 || channels<=0          so negative wrap cannot
                      -> SONG_ERR_CORRUPT_DATA)              reach the wrapper)
song_read_pcm        SONG_OK: 0 < produced <= capacity;      capacity = dst.len()/ch,     S3 PASS
                     SONG_EOF: produced == 0;                samples = capacity*ch ==
                     partial success: error surfaces         dst sample capacity; no
                     on the NEXT call; frame_capacity        unit misalignment; wrapper
                     == 0 -> INVALID_ARGUMENT; dst owned     rejects ch==0 / tiny dst
                     by caller                               before the call
song_seek            landing explicit or -1, never           NOT consumed by the          PASS (unused)
                     manufactured                            wrapper in v1 episodes
song_last_error      diagnostic valid until the NEXT         copied to String             PASS
                     call on the same handle;                immediately (no retention);
                     SONG_OK after success (null msg)        null/empty handled
song_close           frees every SongCore-owned resource;    Drop, once, while the File   PASS
                     all views invalidated; NULL is a        is still alive (explicit
                     no-op                                   Drop body precedes field
                                                             drops; close runs before
                                                             fd close)
callbacks            read: >0 bytes / 0 EOF / <0 error;      file_read/file_seek/         S1 (read),
(file_read/seek/     seek: absolute, <0 error;               file_size over Box<File>     FAIL-CLOSED
 size)               size: >=0 bytes / <0 unavailable        owned by the endpoint        after fix
```

Callback lifetime: the `File` is boxed at open, leaked to `userdata`,
and reclaimed into the endpoint struct — it outlives `song_close`
exactly (explicit `Drop` body, then field drops). The native side never
retains `userdata` past `song_close` (impl: `song_io` copied by value
into the handle; `cleanup()` frees FFmpeg state without invoking host
IO — `avformat_close_input` on a custom-IO context does not read/seek).

song_error lifetime: header "valid until the next SongCore call on the
same handle"; the wrapper calls `song_last_error` immediately after the
failed `song_read_pcm` and copies the bytes before any other call.
No retention. PASS.

### Case verdicts (adversarial points from the audit plan)

```text
CASE  CLASSIFICATION        FINDING
S1    PRODUCTION-DEFECT      file_read formed slice::from_raw_parts_mut
      (SAFETY-DEFECT)        from a NULL destination when size == 0: the
                             guard only rejected null dst for size > 0.
                             from_raw_parts_mut requires a non-null data
                             pointer even for a zero-length slice, and the
                             ABI contract does not exclude (dst==NULL,
                             size==0) — so wrapper soundness could not
                             depend on native behavior. FIXED (5ae7d2b):
                             the guard now rejects a null destination
                             regardless of length.
                             Negative control: with the guard relaxed back
                             to the defect shape, Miri reports
                             "Undefined Behavior: constructing invalid
                             value of type &mut [u8]: encountered a null
                             reference" at the from_raw_parts_mut site;
                             with the fix the regression passes under
                             Miri. The regression is not vacuous.
S2    REFINEMENT-GAP +       The header does not spell out
      DOC-DEFECT             "SONG_OK implies *out_handle != NULL" (the
                             impl does satisfy it: *out_handle = h only
                             with a calloc'd non-null h). The wrapper
                             built the endpoint on the handle unchecked,
                             and the Drop comment claimed a check that
                             did not exist. FIXED (5ae7d2b): explicit
                             fail-closed null-handle check on every open
                             path; Drop comment states the invariant
                             accurately.
S3    PASS                   Header guarantees 0 < produced <= capacity on
                             SONG_OK (song_read_pcm contract), produced
                             == 0 on SONG_EOF. dst sample capacity ==
                             capacity_frames x channels exactly; no unit
                             error. (A native contract violation writing
                             past dst could not be defended post-hoc by
                             the wrapper; that direction is covered by
                             the equivalence harness + this audit, not
                             by a runtime check.)
S4    PASS with DOC note     Header: "a song_handle is NOT internally
                             thread-safe; calls on one handle must be
                             externally serialized. Different handles may
                             be used concurrently." That is the
                             externally-serialized class, not
                             thread-affine. Implementation evidence:
                             no TLS/_Thread_local/pthread-key state in
                             songcore_ffmpeg.c; the handle is a plain
                             heap struct over FFmpeg contexts.
                             `unsafe impl Send for SongcoreDecodeStream`
                             is therefore sound (movable between calls,
                             calls externally serialized — which the
                             `Send`-but-not-`Sync` endpoint type
                             enforces). DOC note (MINOR, no change this
                             round): the header never literally says
                             "may move between threads"; worth one
                             sentence at the next ABI-doc touch, since
                             the header SHA is part of the artifact
                             identity and is not edited casually.
S5    PASS, FYI notes        file_size u64->i64 wraps for files > 8 EiB
                             (FYI, unrepresentable media); seek negative
                             offsets are guarded (-1, fail closed); read
                             counts usize->i64 fine; probe-published
                             rate/channels are validated > 0 by the
                             native impl before the wrapper's i32->u32/
                             u16 casts; produced u64->usize is exact on
                             the 64-bit targets this workspace builds.
S6    PASS                   The three callbacks cannot panic: every
                             fallible std operation (read/seek/metadata)
                             returns a Result that is mapped to -1;
                             no unwrap, no allocation, no indexing. The
                             guard hits return -1 before any slice is
                             formed. (Rust 1.98 additionally defines
                             panic-across-extern-"C" as abort; the
                             reachable path never panics.)
```

Additional native finding (not corrected this round):

```text
N1   MINOR RESOURCE-LEAK (latent, native side)
     song_probe (impl line ~872) re-mallocs h->audio_streams on every
     entry into the !probed block without freeing a previous partial
     allocation, so a probe that fails after stream enumeration and is
     RETRIED leaks the first array. No current consumer retries: the
     wrapper closes the handle on any probe failure. Record for the
     next native artifact rebuild; not patched now because the native
     artifact identity is frozen evidence and the leak is unreachable
     from the current wrapper.
```

### Native executable evidence (A2.3)

```text
SONGCORE_NATIVE_GATE (Linux) = RUN, 16/16 PASS
  host artifact libsongcore.a, real FFI, committed reference corpus
  pre-existing: exact reference frame count + stable EOF; full-drain
    PCM SHA-256 identity (mp3/flac/alac); missing-file open error;
    undecodable-file open error; two independent endpoints; capability
    published through the kernel; plugin vs raw-FFI steady decode
added by this audit (corrective branch): zero-length file open error;
  truncated container drains deterministically (EOF or typed error);
  drop-before-EOF releases the native handle (16 reopen cycles stable);
  callback guard regressions (no native call needed)
WINDOWS artifact: identity checked by symbol table + sha256 at the
  gate host; behavioral evidence = the Windows physical gates (Part 3
  / Part B), which exercise the same ABI end to end
SANITIZER: SANITIZER-NOT-AVAILABLE as a full-library gate this round
  (ASan/UBSan would require rebuilding the pinned FFmpeg closure with
  sanitizer runtime - out of audit scope per A2.4). MIRI ran on the
  wrapper-owned callback surface: 3/3 PASS + the S1 negative control
  UB witnessed. MIRI-PARTIAL is the honest label.
FORMALIZATION: NOT EARNED. The FFI contract is a sequential,
  externally-serialized protocol; no two independently legal events
  were found whose interleaving could produce an illegal state. The
  right mechanisms were used instead: types, RAII, guards, tests,
  Miri on the unsafe surface.
```

---

## PART 2 — WASAPI / Windows boundary audit (A3)

### Ownership / lifetime map (code-reality projection, not authority)

```text
OBJECT              CREATED BY      OWNED BY         RELEASE OP / ORDER          VERDICT
render thread       open_stream     WasapiStream     join via stop_and_join      PASS
                                                     or abort_thread on failed
                                                     verdict/timeout
COM apartment       CoInitializeEx  ComApartment     CoUninitialize exactly      PASS (after
(RENDER thread)     (MTA)           (RAII, drop      once on every path;          COM fix)
                                    runs in unwind)  S_OK and S_FALSE both
                                                     balanced
IMMDeviceEnumerator CoCreateInstance open_session    smart-pointer Release at    PASS
IMMDevice           CoCreateInstance open_session    scope end (after client     PASS
                                                     holds its own ref)
IAudioClient        device.Activate DeviceSession    Stop() in Drop, Release     PASS
                                                     by smart pointer (field 2)
IAudioRenderClient  GetService      DeviceSession    Release by smart pointer    PASS
                                                     (field 1, before client)
event HANDLE        CreateEventW    DeviceSession    CloseHandle exactly once    H1 LEAK ->
                                                     (field 3, after client)     FIXED
device buffer       GetBuffer       borrow, only     ReleaseBuffer(k<=N) on      PASS
(GetBuffer N)                       between Get/     every branch incl. EOF/
                                    ReleaseBuffer    Stopped; panic unwinds to
                                                     session Drop (Get without
                                                     Release is discarded by
                                                     Stop — legal)
RenderPcmInput      session (edge)  session +        stop() idempotent, wakes    PASS
                    activation      stream request   both legs (campaign L4a/b)
DrainSignal         session         session +        complete() first-wins       PASS
                    activation      render thread    exactly once
WasapiStream        open_stream     session (effect) stop_and_join = stop ->     PASS
                                                     join -> release
DeviceSession       open_session    open_and_run     Drop = Stop; fields drop    PASS
                                    inner scope      render -> client -> event;
                                                     drops on error, panic
                                                     (unwind), and normal path
```

### COM audit (A3.2)

`CoInitializeEx(None, COINIT_MULTITHREADED)` returns Ok for both
`S_OK` and `S_FALSE`; both require (and get) exactly one
`CoUninitialize` through the `ComApartment` guard, which drops during
unwind as well. Finding (MINOR): a failed `CoInitializeEx`
(`RPC_E_CHANGED_MODE`, `E_OUTOFMEMORY`) did not stop the open — the
code proceeded into `CoCreateInstance` and relied on downstream
`CO_E_NOTINITIALIZED` for an honest failure. That violates fail-closed
at the boundary for no benefit. FIXED (29475ed): the open aborts at
the boundary with the COM error as the verdict message.

### Event HANDLE audit (A3.3) — the MAJOR finding

```text
H1   MAJOR RESOURCE-LEAK — every successful episode leaked one kernel
     event handle. DeviceSession stored the event as a plain
     windows-rs HANDLE, which is a raw wrapper with NO Drop; the Drop
     comment claimed "fields drop in declaration order: ... event
     handle - the required historical release order", but only the COM
     smart pointers actually released anything. The three error paths
     (SetEventHandle / GetBufferSize / GetService failures) closed the
     event manually; the success path - the only path a real player
     lives on - never did. This is exactly the trap the audit plan
     named: "do not believe 'fields drop in declaration order'.
     Confirm HANDLE itself has a RAII Drop."

     FIXED (2ab25b9): EventHandle(HANDLE) RAII guard; Drop runs
     CloseHandle exactly once on every exit path (open failure, panic,
     stop, normal release), still released after the audio client per
     the historical order.

     REGRESSION EVIDENCE (Windows, real endpoint):
     - positive control: a live nonsignaled event times out
       (WAIT_TIMEOUT); after CloseHandle the same wait returns
       WAIT_FAILED - a closed handle is observably dead, so the loop
       oracle below is meaningful, not vacuous;
     - leak loop: 30 alternating open/stop/drain cycles over the real
       default endpoint; process handle count (GetProcessHandleCount)
       returns to baseline within +4 tolerance. Pre-fix shape: +1
       handle per cycle (~+30 over the loop).
```

### Device buffer safety (A3.4)

Formula, verified at both ends:
`samples = frames x channels` (stream format == source format, Tier 1
negotiation publishes the submitted format verbatim; Initialize would
have refused anything else), `bytes = samples x sizeof(f32)`.
`available = buffer_frames.saturating_sub(padding) <= buffer_frames`;
GetBuffer(available) yields exactly `available x channels` writable
f32; every branch releases with `frames_written <= available` (0 on
EOF/Stopped). Overflow notes (FYI, no defect): `nBlockAlign` is u16
and would wrap for channels > 16383 (no real codec produces this;
probe validates channels > 0 but not an upper bound); `nAvgBytesPerSec`
u32 wraps for absurd rates and Initialize rejects the malformed format.

### Start/Stop/release lifecycle (A3.5)

State machine traced from code: ThreadSpawned -> COMInitialized ->
DeviceActivated -> ClientInitialized -> EventBound ->
RenderServiceAcquired -> [open verdict published] -> Started ->
Rendering -> Draining/Aborting -> Stopped -> Released -> ThreadExited.
All 13 audited failure paths (CoCreateInstance fail, no endpoint,
Activate/Initialize/CreateEvent/SetEventHandle/GetBufferSize/GetService/
Start fail, GetCurrentPadding/GetBuffer/ReleaseBuffer fail, stop during
blocked read, EOF drain, panic) land in exactly one of two sinks:
the open verdict (fail, no session), or the loop outcome (session Drop
runs Stop + field releases; run_render_thread guarantees the drain
verdict and the data-plane stop even on panic). The open-verdict
handshake has no path that leaks the thread: failed verdicts and the
10 s timeout both abort_thread (stop + join).
Wait failure (WaitForSingleObject) is ignored by design: the 100 ms
bounded wait is cadence, not correctness; GetCurrentPadding is the
per-period authority. FYI: a permanently failed wait would degrade to
a busy poll at the padding-check rate, not to a hang.

### Windows reality gate (A4)

```text
FINDING W1 (MAJOR, TOOLING/BUILD): main at the audit base SHA did not
     compile on Windows-native at all — the crate's only real platform:
     (a) qianqian-output-wasapi failed with E0133: when open_session
     was refactored from an unsafe fn into a safe fn with call-site
     unsafe blocks, mem::zeroed() of WAVEFORMATEXTENSIBLE lost its
     unsafe context (the crate-level unsafe_op_in_unsafe_fn allowance
     that previously covered it is gone; no Linux build compiles this
     cfg(windows) module, so nothing noticed);
     (b) cargo test --workspace additionally failed on unconditional
     imports of the Linux-only /proc leak-oracle helpers in
     test_oracles.rs and session_activation.rs.
     The Windows reality gate was red at the base before any F1 work.
     FIXED on corrective/native-boundary-wasapi-1 (8c9ee91, ec596f8):
     explicit unsafe block for the zeroed init; /proc controls gated
     to linux like the helpers they exercise.
```

Windows-native gate results (integration branch
gate/f1-integration = main + all correctives + F1, native checkout at
C:\qianqian-gate\f1; toolchains: MSVC for the workspace test gate,
x86_64-pc-windows-gnu over the C:\mingw64 mingw-w64 closure for the
physical binary, matching the first-audible gate recipe):

    cargo test --workspace (MSVC)                 all green
      incl. stop_seam 9/9, wasapi handle suite 2/2,
      platform gate, test_oracles, session_activation
    cargo test -p qianqian-output-wasapi          3/3 (leak loop on the
      real endpoint: +0 handle growth over 30 cycles)
    cargo test -p qianqian-decode-songcore (gnu)  16/16 real FFI
    cargo check -p qianqian-headless
      --features playback (MSVC)                  PASS
    physical binary (gnu, release)                built
    physical stop gate:  20/20 PASS — real SongCore decode + real
      WASAPI render; stdin 'stop' after the playing witness; exit 0,
      outcome 'stopped before completion', render leg exits with the
      data-plane stop diagnostic, quiet disposal, no hang (20 s
      watchdog per episode)
    physical EOF gate:   10/10 PASS — natural completion preserved,
      exit 0, quiet disposal

### Formalization triage (A3.6)

FORMALIZATION = NOT EARNED (both boundaries). The independently-legal
interleavings that once earned formal treatment (publication/reader
overlap; edge terminal first-wins; blocked endpoint wakeups) are
already covered by `specs/realtime-publication/` and Campaign-1 L1–L4.
F1's new shared state (stop_requested + stop target binding under one
mutex; idempotent first-wins edge stop) introduced no second writer
and no new collision surface: stop x EOF x failure x dispose resolve
through the same first-wins terminals Campaign-1 explored. No new
Loom/TLA target.

---

## Findings register (A5)

```text
ID   AREA      CLASSIFICATION         SEVERITY  STATUS
S1   songcore  SAFETY-DEFECT          MAJOR     FIXED 5ae7d2b (+Miri neg control)
S2   songcore  REFINEMENT-GAP +       MINOR     FIXED 5ae7d2b
               DOC-DEFECT
N1   songcore  RESOURCE-LEAK (latent, MINOR    DEFERRED (native rebuild)
               native impl; unreachable
               from current wrapper)
S4   songcore  DOC-DEFECT (header      FYI      RECORDED (ABI-identity cost;
               movability unstated)             next ABI-doc touch)
H1   wasapi    RESOURCE-LEAK           MAJOR    FIXED 2ab25b9 (+Windows loop
                                                  evidence)
W1   windows   PRODUCTION-DEFECT       MAJOR    FIXED 8c9ee91 + ec596f8
               (gate compile breakage           (+8e93357 import/const)
               at base, 3 sites)
C1   wasapi    REFINEMENT-GAP          MINOR    FIXED 29475ed
               (CoInitializeEx not
               fail-closed)
L1   repo      DOC/HYGIENE (stale      FYI      RECORDED (not this gate's
               committed decode-songcore        cleanup)
               Cargo.lock references
               pre-rename crate names;
               every local cargo run
               rewrites it)
```

## Corrective PRs

```text
fix(decode):  fail closed on null host-IO destination and null song
              handle + adversarial native evidence
              (corrective/native-boundary-songcore-1)
fix(wasapi):  RAII event-handle guard; Windows compile fixes; COM
              fail-closed; procfs oracle gating
              (corrective/native-boundary-wasapi-1)
```

## Unverified surfaces (explicit)

```text
- ASan/UBSan over the native songcore closure (needs a sanitizer
  build of the pinned FFmpeg import; NOT RUN)
- WASAPI under device loss / unplug / default-device switch /
  sleep-wake (MANUAL DEVICE ADVERSARY: DEFERRED - needs human
  hardware interaction this round did not have)
- Windows long-run soak (the 30-cycle handle loop + 20x physical stop
  gate are bounded; no hours-scale soak claim)
- songcore shared-library (DLL) consumption on Windows (this round
  exercised the static mingw artifact only)
```

## VERDICT

```text
PASS_WITH_CORRECTIVES
- both boundaries audited against the in-repo native contract and the
  real devices/artifacts;
- 2 MAJOR resource/lifecycle-safety findings + 1 MAJOR Windows-gate
  breakage found and fixed in separate corrective branches with
  regression or negative-control evidence each;
- audit stops here by design; native scope does not grow further this
  round (A5.2).
```
