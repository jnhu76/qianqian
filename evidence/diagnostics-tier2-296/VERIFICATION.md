# Final execution receipt

Core Rust / test subject: 8517c1f5abe473b8bc8e585e89fb079c7b5ad95d.
Base main: 98ff343467b66890a979edd54d21e14f0275c574.

- Native Windows MSVC: 32 armed child executions. Fault ledger: 48 cells,
  22 PASS / 3 KNOWN_RESIDUAL / 11 NOT_APPLICABLE / 12 NOT_RUN.
- 23 targeted mutation executions: exact RED; 2 harness-negative REDs.
- 11 mutation classes: current target bytes match recorded preimages.
- Native Windows targeted Clippy: PASS. Native Windows workspace: FAIL_ORACLE
  at two pre-existing source mutation oracles, not blanket PASS. Independent
  baseline source-oracle comparison: LF 2/2 GREEN; CRLF 2/2 RED.
- Native Linux LF checkout: original complete `lefthook run pre-push --all-files`,
  all eight checks GREEN, GATE_EXIT=0, 187.41 seconds. This includes all plugin
  negative controls, formatting, workspace Clippy/tests, release-policy,
  website verification and VitePress build. See raw/pre-push-linux.log.
- Windows whole-gate attempts remain failed receipts. The unsuccessful LF
  adaptation was withdrawn. No checker rule or negative control was skipped.
- Independent review: REVIEW.md. Scoped delivery accepted; not GitHub review
  approval, owner residual acceptance, or #296 closure approval.

Raw metadata includes the Windows 16-file input manifest, individual executable
hashes, core commit, target/compiler/OS/device facts, Linux gate environment,
source newline distinction, and explicit outer-runner overlays. The initial
14-file Linux environment is retained; later 16-file metadata labels its
post-gate outer script hash refresh rather than pretending those scripts
were Rust runtime inputs.

No product fix is included. FFMPEG_LOG=0 is a source-confirmed gate differential;
actual native impact is NOT_RUN. PRODUCT_DIFFERENTIAL.md isolates the proposal.
Native FFmpeg/decode rows and owner residual disposition keep #296 open.
