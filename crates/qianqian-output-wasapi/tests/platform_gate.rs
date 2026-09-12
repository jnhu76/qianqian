//! Platform gate: the Output plugin publishes its capability for real on
//! Windows, and fails its activation loudly on every other platform —
//! never a fake success, never a silent null output.

use qianqian_kernel::{DesiredEntry, FiberState, Revision};
use qianqian_output_wasapi::wasapi_output_plugin;
use qianqian_runtime::AppRuntime;

fn desired_plugin(id: &str) -> DesiredEntry {
    DesiredEntry::enabled(id, "wasapi_output_plugin", Revision::new(1))
}

#[cfg(not(windows))]
#[test]
fn non_windows_activation_fails_loudly_without_ghost_provisions() {
    let mut runtime = AppRuntime::new();
    runtime
        .register_component(wasapi_output_plugin())
        .expect("legal");
    runtime
        .revise_desired(vec![desired_plugin("output")])
        .expect("legal");

    let snap = runtime.composition_snapshot();
    let fiber = snap.fibers.get("output").expect("installed");
    assert_eq!(
        fiber.state,
        FiberState::Failed,
        "activation refuses the platform"
    );
    assert!(fiber.failed_outcome, "the refusal is recorded as FAILED");
    assert!(
        snap.capabilities
            .get("AudioOutput")
            .is_none_or(|p| p.is_none()),
        "no capability binding is published for a mechanism that does not exist"
    );
    assert!(
        snap.provisions
            .get("AudioOutput")
            .is_none_or(|p| p.is_empty()),
        "a raised activation publishes no provision"
    );
    let snap = runtime.dispose();
    assert!(snap.quiet, "the failed fiber disposes cleanly");
}

#[cfg(windows)]
#[test]
fn windows_activation_publishes_the_capability() {
    let mut runtime = AppRuntime::new();
    runtime
        .register_component(wasapi_output_plugin())
        .expect("legal");
    runtime
        .revise_desired(vec![desired_plugin("output")])
        .expect("legal");

    let snap = runtime.composition_snapshot();
    let fiber = snap.fibers.get("output").expect("installed");
    assert_eq!(fiber.state, FiberState::Active);
    assert_eq!(
        snap.capabilities.get("AudioOutput").map(|p| p.as_deref()),
        Some(Some("output")),
        "the real output mechanism is kernel truth on Windows"
    );
    let snap = runtime.dispose();
    assert!(snap.quiet);
}
