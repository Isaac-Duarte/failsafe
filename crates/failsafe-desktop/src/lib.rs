mod capture;
mod control;
mod feature;
mod host;
mod input;
mod keys;
mod protocol;
mod relay;
mod viewer;

pub use capture::list_displays;
pub use control::{DesktopFeatureControl, OpenDesktopRequest};
pub use feature::{DesktopFeature, DesktopFeatureSpec, ID as DESKTOP_FEATURE_ID};
pub use relay::{
    handle_incoming_desktop, handle_incoming_input, run_outgoing_desktop, start_desktop_acceptor,
    start_input_acceptor, stop_desktop_acceptor, stop_input_acceptor,
};
