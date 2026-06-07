use std::collections::HashSet;
use std::sync::Arc;

use failsafe_clipboard::feature::{ClipboardFeature, ClipboardFeatureSpec};
use failsafe_clipboard::limits::ClipboardLimits;
use failsafe_core::api::FeatureInfo;
use failsafe_core::feature::{FeatureControl, FeatureId, FeatureSpec, UnknownFeatureId};
use failsafe_core::outbound::OutboundPublisher;
use failsafe_core::peer::PeerDirectory;
use failsafe_core::registry::FeatureRegistry;
use failsafe_port::{PortFeature, PortFeatureControl, PortFeatureSpec};
use failsafe_send::{SendCoordinator, SendFeature, SendFeatureControl, SendFeatureSpec};
use failsafe_shell::{ShellFeature, ShellFeatureControl, ShellFeatureSpec};
use failsafe_transport::blobs::BlobTransfer;
use failsafe_transport::iroh::IrohTransport;
use failsafe_transport::transport::Transport;
use tokio::sync::RwLock;

type SpecFn = fn() -> FeatureInfo;

fn spec_info<S: FeatureSpec>() -> FeatureInfo {
    FeatureInfo {
        id: S::id().to_owned(),
        label: S::label().to_owned(),
        description: S::description().to_owned(),
    }
}

const CATALOG_BUILDERS: &[SpecFn] = &[
    || spec_info::<ClipboardFeatureSpec>(),
    || spec_info::<ShellFeatureSpec>(),
    || spec_info::<PortFeatureSpec>(),
    || spec_info::<SendFeatureSpec>(),
];

pub fn catalog() -> Vec<FeatureInfo> {
    CATALOG_BUILDERS.iter().map(|build| build()).collect()
}

pub fn all_ids() -> Vec<FeatureId> {
    vec![
        ClipboardFeatureSpec::feature_id(),
        ShellFeatureSpec::feature_id(),
        PortFeatureSpec::feature_id(),
        SendFeatureSpec::feature_id(),
    ]
}

pub fn is_known_feature(id: &str) -> bool {
    all_ids().iter().any(|feature| feature.as_str() == id)
}

pub fn parse_feature_id(value: &str) -> Result<FeatureId, UnknownFeatureId> {
    let value = value.trim();
    all_ids()
        .into_iter()
        .find(|id| id.as_str() == value)
        .ok_or_else(|| UnknownFeatureId(value.to_owned()))
}

pub struct DaemonBuildContext {
    pub publisher: Arc<dyn OutboundPublisher>,
    pub blob_transfer: Arc<dyn BlobTransfer>,
    pub clipboard_limits: ClipboardLimits,
    pub transport: Arc<dyn Transport>,
    pub send_coordinator: Arc<SendCoordinator>,
    pub iroh: Option<Arc<IrohTransport>>,
}

pub fn build_feature_registry(ctx: &DaemonBuildContext) -> Result<FeatureRegistry, failsafe_core::feature::FeatureError> {
    let mut registry = FeatureRegistry::new();

    registry.register(Box::new(ClipboardFeature::new_with_limits(
        ctx.publisher.clone(),
        Some(ctx.blob_transfer.clone()),
        ctx.clipboard_limits,
    )))?;

    registry.register(Box::new(SendFeature::new(
        ctx.blob_transfer.clone(),
        ClipboardLimits::unlimited(),
        ctx.transport.clone(),
        ctx.send_coordinator.clone(),
    )))?;

    if let Some(iroh) = ctx.iroh.clone() {
        registry.register(Box::new(ShellFeature::new(iroh.clone())))?;
        registry.register(Box::new(PortFeature::new(iroh)))?;
    }

    Ok(registry)
}

pub struct ControlBuildContext {
    pub iroh: Arc<IrohTransport>,
    pub transport: Arc<dyn Transport>,
    pub blob_transfer: Arc<dyn BlobTransfer>,
    pub device_name: String,
    pub send_limits: ClipboardLimits,
    pub coordinator: Arc<SendCoordinator>,
    pub local_features: Arc<RwLock<HashSet<FeatureId>>>,
    pub peers: Arc<PeerDirectory>,
}

pub fn build_control_handlers(
    ctx: &ControlBuildContext,
) -> Vec<Box<dyn FeatureControl>> {
    vec![
        Box::new(ShellFeatureControl::new(ctx.iroh.clone())),
        Box::new(PortFeatureControl::new(ctx.iroh.clone())),
        Box::new(SendFeatureControl::new(
            ctx.transport.clone(),
            ctx.blob_transfer.clone(),
            ctx.device_name.clone(),
            ctx.send_limits,
            ctx.coordinator.clone(),
        )),
    ]
}
