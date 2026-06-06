mod address;
mod config;
mod manager;
mod protocol;
mod stream;
mod transport;

#[cfg(all(test, feature = "iroh"))]
mod tests;

pub use address::SharedAddressState;
pub use config::IrohConfig;
pub use stream::{
    ScreenAcceptor, ScreenSession, SharedScreenAcceptor, SharedShellAcceptor, ShellAcceptor,
    ShellSession, relay_screen_inbound, relay_shell_streams, relay_shell_to_channels,
};
pub use transport::IrohTransport;
