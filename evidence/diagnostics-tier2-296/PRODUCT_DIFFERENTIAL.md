# Isolated proposal: FFmpeg log switch value `0`

Status: SOURCE_CONFIRMED_CONFIG_GATE_DIFFERENTIAL.
Classification: FAIL_PRODUCT for the inconsistent source-level gate claim;
actual native runtime effect remains NOT_RUN. No fix is implemented here.

At base `98ff343`:

- `crates/qianqian-decode-songcore/src/av_log_bridge.rs::bridge_requested`
  returns false for absent or `0`. Its documented off posture is discard.
- `native/src/songcore_ffmpeg.c::songcore_quiet_ffmpeg_logging` installs the
  discard callback only when getenv returns NULL. A present `0` skips it.
- Pinned FFmpeg log.c initializes its global callback to the default logger.
  The default callback writes synchronously; replacing it is an atomic store.

Thus, in a fresh process with `QIANQIAN_FFMPEG_LOG=0`, neither adapter bridge
installation nor native per-open discard installation selects the claimed
off callback. This conclusion is a branch-composition source fact, not a
claim that a native runtime test was executed.

Owner decision needed: define the common existing-switch interpretation of
`0` and align these two predicates in a separate narrow product fix. Keeping
`0=off` would preserve the current Rust gate tests/documentation; another
decision must explicitly resolve that differential rather than silently
retaining contradictory layers.

Proposed verification for that separate fix:

1. Verify a target-specific SongCore archive, manifest and FFmpeg identity
   under #172 before linking the test.
2. Use fresh processes for unset, `0` and `1`; activate the real adapter,
   execute a real open and real av_log invocation, observe actual callback
   admission and the sink boundary.
3. For the selected off cases, confirm discard rather than only calling the
   Rust pure gate helper. For opt-in, confirm the real α callback record and
   unregistered drop count.
4. Deliberately restore the old predicate mismatch; the same `0` oracle must
   turn RED. Then apply blocked/broken stderr controls without assigning
   diagnostic output semantic authority.

No ABI, Plugin, EventBus/ErrorBus, additional lifecycle or D11/D14/K0 change
is required or authorized by this proposal. Native runs are still NOT_RUN
until the artifact and test receipts exist.
