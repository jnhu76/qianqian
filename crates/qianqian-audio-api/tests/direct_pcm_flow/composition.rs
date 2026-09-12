//! Composition-time fixture: assembles the pre-bound PCM flow through the
//! real generic kernel.
//!
//! Setup path only. The three participant roles are hosted as kernel
//! capability contracts; the flow assembler resolves each participant
//! exactly once during its single activation step and stores the resulting
//! pre-bound path outside kernel storage — the same resolve-once,
//! pre-bind-outside-the-kernel shape the runtime composition root uses.
//! After `settle`, nothing in the hot path touches the kernel.

use std::cell::RefCell;
use std::rc::Rc;

use qianqian_composition::{
    ActivationError, Capability, ComponentSpec, CompositionKernel, DesiredEntry, ResolveError,
    Revision,
};

use super::harness::PcmFormat;
use super::mutations::DirectoryLookupFlow;
use super::participants::{PcmSink, PcmSource, PcmStage, PreboundPcmFlow};

/// Capability contract for the stream-source role. Identity is this contract
/// definition site; the NAME is diagnostic vocabulary.
pub struct StreamSourceCapability;

impl Capability for StreamSourceCapability {
    const NAME: &'static str = "stream source";
    type Service = RefCell<dyn PcmSource>;
}

/// Capability contract for the processing-stage role.
pub struct ProcessingStageCapability;

impl Capability for ProcessingStageCapability {
    const NAME: &'static str = "processing stage";
    type Service = RefCell<dyn PcmStage>;
}

/// Capability contract for the stream-sink role.
pub struct StreamSinkCapability;

impl Capability for StreamSinkCapability {
    const NAME: &'static str = "stream sink";
    type Service = RefCell<dyn PcmSink>;
}

/// Provider fiber for the source participant.
pub fn source_provider_component(service: Rc<RefCell<dyn PcmSource>>) -> ComponentSpec {
    ComponentSpec::new("stream_source")
        .provides::<StreamSourceCapability>()
        .on_activate(move |ctx| {
            ctx.provide::<StreamSourceCapability>(service.clone())
                .expect("the source provider declares its capability");
            Ok(())
        })
}

/// Provider fiber for the stage participant.
pub fn stage_provider_component(service: Rc<RefCell<dyn PcmStage>>) -> ComponentSpec {
    ComponentSpec::new("processing_stage")
        .provides::<ProcessingStageCapability>()
        .on_activate(move |ctx| {
            ctx.provide::<ProcessingStageCapability>(service.clone())
                .expect("the stage provider declares its capability");
            Ok(())
        })
}

/// Provider fiber for the sink participant.
pub fn sink_provider_component(service: Rc<RefCell<dyn PcmSink>>) -> ComponentSpec {
    ComponentSpec::new("stream_sink")
        .provides::<StreamSinkCapability>()
        .on_activate(move |ctx| {
            ctx.provide::<StreamSinkCapability>(service.clone())
                .expect("the sink provider declares its capability");
            Ok(())
        })
}

fn activation_failure(error: ResolveError) -> ActivationError {
    ActivationError::new(format!("{error:?}"))
}

/// The flow assembler: requires the three participant capabilities and, in
/// its single activation step, resolves each exactly once and builds the
/// pre-bound executable path outside kernel storage.
pub fn flow_assembler_component(
    flow_slot: FlowSlot,
    block_storage_capacity_frames: usize,
    format: PcmFormat,
) -> ComponentSpec {
    ComponentSpec::new("flow_assembler")
        .requires::<StreamSourceCapability>()
        .requires::<ProcessingStageCapability>()
        .requires::<StreamSinkCapability>()
        .on_activate(move |ctx| {
            let source = ctx
                .resolve::<StreamSourceCapability>()
                .map_err(activation_failure)?
                .service();
            let stage = ctx
                .resolve::<ProcessingStageCapability>()
                .map_err(activation_failure)?
                .service();
            let sink = ctx
                .resolve::<StreamSinkCapability>()
                .map_err(activation_failure)?
                .service();
            *flow_slot.borrow_mut() = Some(PreboundPcmFlow::new(
                source,
                stage,
                sink,
                block_storage_capacity_frames,
                format,
            ));
            Ok(())
        })
}

/// The adversarial control assembler: identical setup semantics, but the
/// bound flow re-consults a participant directory on every quantum instead
/// of using pre-bound references (the hot-path anti-shape under test).
pub fn directory_lookup_assembler_component(
    flow_slot: Rc<RefCell<Option<DirectoryLookupFlow>>>,
    block_storage_capacity_frames: usize,
    format: PcmFormat,
) -> ComponentSpec {
    ComponentSpec::new("flow_assembler")
        .requires::<StreamSourceCapability>()
        .requires::<ProcessingStageCapability>()
        .requires::<StreamSinkCapability>()
        .on_activate(move |ctx| {
            let source = ctx
                .resolve::<StreamSourceCapability>()
                .map_err(activation_failure)?
                .service();
            let stage = ctx
                .resolve::<ProcessingStageCapability>()
                .map_err(activation_failure)?
                .service();
            let sink = ctx
                .resolve::<StreamSinkCapability>()
                .map_err(activation_failure)?
                .service();
            *flow_slot.borrow_mut() = Some(DirectoryLookupFlow::new(
                source,
                stage,
                sink,
                block_storage_capacity_frames,
                format,
            ));
            Ok(())
        })
}

/// Where setup leaves the assembled flow for extraction.
pub type FlowSlot = Rc<RefCell<Option<PreboundPcmFlow>>>;

/// What setup produced: the kernel that mediated the bindings, the extracted
/// pre-bound flow, and direct handles to the sink-side participants for
/// external observation (cursor readouts, conservation cross-checks).
pub struct CompositionFixture {
    pub kernel: CompositionKernel,
    pub flow: PreboundPcmFlow,
    pub source: Rc<RefCell<dyn PcmSource>>,
    pub sink: Rc<RefCell<dyn PcmSink>>,
}

/// Registers the three provider fibers plus the flow assembler, settles the
/// desired composition, and extracts the pre-bound flow.
///
/// The participants are resolved exactly once, by the assembler's single
/// activation step, through real capability resolution — and then used
/// without the kernel.
pub fn compose_prebound_flow(
    source: Rc<RefCell<dyn PcmSource>>,
    stage: Rc<RefCell<dyn PcmStage>>,
    sink: Rc<RefCell<dyn PcmSink>>,
    format: PcmFormat,
    block_storage_capacity_frames: usize,
) -> CompositionFixture {
    let flow_slot: FlowSlot = Rc::new(RefCell::new(None));
    let mut kernel = CompositionKernel::new();
    kernel
        .register_component(source_provider_component(source.clone()))
        .expect("the source component registers");
    kernel
        .register_component(stage_provider_component(stage))
        .expect("the stage component registers");
    kernel
        .register_component(sink_provider_component(sink.clone()))
        .expect("the sink component registers");
    kernel
        .register_component(flow_assembler_component(
            flow_slot.clone(),
            block_storage_capacity_frames,
            format,
        ))
        .expect("the flow assembler registers");
    kernel
        .set_desired(vec![
            DesiredEntry::enabled("stream_source", "stream_source", Revision::fresh()),
            DesiredEntry::enabled("processing_stage", "processing_stage", Revision::fresh()),
            DesiredEntry::enabled("stream_sink", "stream_sink", Revision::fresh()),
            DesiredEntry::enabled("flow_assembler", "flow_assembler", Revision::fresh()),
        ])
        .expect("the desired composition is legal");
    kernel.settle();
    let flow = flow_slot
        .borrow_mut()
        .take()
        .expect("setup must have produced the pre-bound flow");
    CompositionFixture {
        kernel,
        flow,
        source,
        sink,
    }
}
