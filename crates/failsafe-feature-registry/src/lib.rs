mod feature_registry;

pub use feature_registry::{
    ControlBuildContext, DaemonBuildContext, all_ids, build_control_handlers,
    build_feature_registry, catalog, is_known_feature, parse_feature_id,
};
