```test-quality
TEST_ID: qianqian-audio-api::render_gate_seek::a_seek_park_publishes_only_seek_events_in_order
SUBJECT_SHA: 3f139694f175650e09e2843d2280456984052d4b
M: M3
R: R-M
T: T2
PROPERTY: exactly one SeekTailQuiesced per seek park, seek-attributed events only
AUTHORITY: docs/adr/ADR-PBK-002.md (D14.5)
RESULT: PASS_IN_SCOPE
PRODUCTION_BOUNDARY: crates/qianqian-audio-api/src/ports.rs:319 (RenderGate park slices)
PRECONDITION_WITNESS: wait_until(parked) latch on first slice
LEGAL_OUTCOMES: fixture fixes the parked-first ordering by construction
OBSERVATION_BOUNDARY: GateEvent observer log (mechanism evidence)
FALSIFIER: STATIC — a gate that re-publishes quiescence breaks the exact-sequence assert
EXECUTION_PROFILE: cargo test -p qianqian-audio-api --test render_gate_seek, linux stable
ACTUAL_TEST_COUNT: 1
CLEANUP_GUARANTEE: join within watchdog; leg exits on release
EVIDENCE_LIMITS: STATIC falsifier only; flake probability UNMEASURED
```
