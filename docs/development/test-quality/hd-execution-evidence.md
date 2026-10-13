# H-D execution and independent review receipts

Non-normative evidence for #271 / #258. Source/audit subject `e70a5b42bf663d4465fb887b880c8e6e8edabc46`;
base `bc0b317b4f502f986d8dc480b8b0d4e29c7bf525`. Final PR-head publication
gate and hosted CI belong to append-only issue/PR receipts, not inferred here.

[Subject/authority](hd-subject-and-authority.md) ·
[derived ledger](hd-disposition-ledger.csv) ·
[full initial evidence](evidence/hd-initial-evidence.zip)
(SHA256 `625cb3f4a2ece96562dc07da4fd63fa9f48ac5a298bb5224ab5b291f35cd34ac`). The original 918-row inventory's 15 segments
reconstruct to `975de34ad6d296deb8945cd8d1045fc01af8b405c5a7dda3f5494fe5702be7cd`.

| Lane | Result / scope |
| --- | --- |
| Before: machine_input lib | TEST_EXECUTED_PASS 21/0, including H4 |
| Before: playback lib | TEST_EXECUTED_PASS 191/0, including deleted test |
| Before: exact EOF integration | TEST_EXECUTED_PASS 1/0 |
| After: machine_input lib | TEST_EXECUTED_PASS 21/0, same matrix |
| After: playback lib | TEST_EXECUTED_PASS 190/0, exactly one test deleted |
| After: pause/seek/stop targets | TEST_EXECUTED_PASS 11/18/7, zero failures |
| Initial shared-target pre-push | FAIL: stale CARGO_MANIFEST_DIR, two missing-fixture failures, not semantic RED; docs skipped |
| Fresh isolated pre-push | TEST_EXECUTED_PASS workspace 876/0; seven checks PASS incl. gate negative controls, fmt, clippy and website verify/build |
| Reviewer A | SOURCE_REVIEW APPROVE; outstanding P0–P3=0; no execution claim |
| Reviewer B | SOURCE_REVIEW APPROVE; 7 nonempty commands/10 tests PASS; zero-test run 0 retained/excluded |
| Base hosted Linux/Windows | fetched success logs 38360046293/38360046301 at bc0b317; not final PR proof |

Review corrected a stale deleted-oracle comment, limited funnel claim to loop
exits/caught loop unwind, and corrected headless binary/lib identity. Exact review
subject is `e70a5b42bf663d4465fb887b880c8e6e8edabc46`; later evidence additions change no Rust file.

The archive contains full logs, argv/rc, RUN_ID, source snapshots, patch hashes,
source/binary checksums, reviews/independent commands, failed runs/diagnosis and
original remote records. Baseline shared binaries were not freshly rebuilt;
later-collected hashes cannot prove original overwritten baseline identity.
The successful complete gate rebuilt this checkout in fresh CARGO_TARGET_DIR,
with 41 executed binary paths/hashes. No unexplained retry hid a failure.

Limits: production funnel order SOURCE_PROOF, swapped-order mutant NOT_RUN;
pre-catch setup panic can bypass the funnel. Setup/funnel panic, panic=abort,
poisoned locks/allocator abort are not covered. Integration load does not prove
a causal interleaving. Semantic delta NONE is DERIVED_ONLY from authority/dataflow
plus regression evidence. No new ADR/API/ABI/observer/scheduler/test infrastructure.
No endpoint/audio/FFI/macOS/Android/iOS runtime or new formal campaign claim.
