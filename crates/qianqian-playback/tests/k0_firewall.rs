//! K0 hot-path firewall (first-audible-slice design §5): after activation,
//! the steady data plane must flow with zero K0 work per quantum. Uses
//! Kernel directly (not QianqianApp) because debug_op_count is the oracle.

mod common;

use std::time::Duration;

use qianqian_composition::{CompositionKernel, DesiredEntry, Revision};
use qianqian_playback::{SessionCompletion, SessionOutcome, playback_session_spec};

use common::{OutputBehavior, SourceBehavior, TestDecode, TestOutput, within};

fn desired(id: &str, component: &'static str) -> DesiredEntry {
    DesiredEntry::enabled(id, component, Revision::new(1))
}

fn kernel_with_session(completion: &SessionCompletion) -> CompositionKernel {
    let mut kernel = CompositionKernel::new();
    kernel
        .register_component({
            let behavior = SourceBehavior::EofAfter(48_000);
            qianqian_composition::ComponentSpec::new("test_decode_plugin")
                .provides::<qianqian_audio_api::ports::PcmDecodeCapability>()
                .on_activate(move |ctx| {
                    ctx.provide::<qianqian_audio_api::ports::PcmDecodeCapability>(
                        std::rc::Rc::new(TestDecode { behavior }),
                    )
                    .map_err(|e| qianqian_composition::ActivationError::new(format!("{e:?}")))?;
                    Ok(())
                })
        })
        .expect("legal");
    kernel
        .register_component({
            let behavior = OutputBehavior::Consume;
            qianqian_composition::ComponentSpec::new("test_output_plugin")
                .provides::<qianqian_audio_api::ports::AudioOutputCapability>()
                .on_activate(move |ctx| {
                    ctx.provide::<qianqian_audio_api::ports::AudioOutputCapability>(
                        std::rc::Rc::new(TestOutput { behavior }),
                    )
                    .map_err(|e| qianqian_composition::ActivationError::new(format!("{e:?}")))?;
                    Ok(())
                })
        })
        .expect("legal");
    kernel
        .register_component(playback_session_spec(
            std::path::PathBuf::from("test://sine"),
            completion.clone(),
        ))
        .expect("legal");
    kernel
}

#[test]
fn steady_data_plane_performs_zero_kernel_work() {
    within(Duration::from_secs(10), move || {
        let _lifecycle = common::lifecycle_lock();
        let completion = SessionCompletion::new();
        let mut kernel = kernel_with_session(&completion);

        let ops_before = kernel.debug_op_count();
        kernel
            .set_desired(vec![
                desired("decode", "test_decode_plugin"),
                desired("output", "test_output_plugin"),
                desired("session", "playback_session"),
            ])
            .expect("legal");
        kernel.settle();
        let ops_after_activation = kernel.debug_op_count();

        // Sensitivity of the oracle itself: activation does K0 work and
        // the counter must show it. (This is what makes the steady-state
        // zero below a real measurement rather than a stuck counter.)
        assert!(
            ops_after_activation > ops_before,
            "debug_op_count must observe control-plane work"
        );

        let outcome = completion.wait();
        assert_eq!(outcome, SessionOutcome::Completed);

        assert_eq!(
            kernel.debug_op_count(),
            ops_after_activation,
            "the steady data plane returned to the kernel (firewall breach)"
        );
        kernel.dispose_root();
    });
}
