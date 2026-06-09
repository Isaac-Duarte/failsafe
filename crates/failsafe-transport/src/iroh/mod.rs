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
    LanAcceptor, LanSession, PortAcceptor, PortSession, SharedLanAcceptor, SharedPortAcceptor,
    SharedShellAcceptor, ShellAcceptor, ShellSession, read_lan_packet, relay_shell_streams,
    relay_shell_to_channels, write_lan_packet,
};
pub use transport::{IrohTransport, iroh_public_key_hex};
