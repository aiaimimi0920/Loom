//! Native-only TLS transport. Discovery credentials must come from a private manifest,
//! never from WebView arguments. TLS identity is verified before the HTTP auth header.

mod client;
mod discovery;
#[cfg(feature = "server")]
mod server;
mod socket;

pub use client::{connect, ClientSocket};
pub use discovery::BridgeDiscovery;
#[cfg(feature = "server")]
pub use server::{ServerIdentity, ServerSocket};
pub use socket::DeadlineStream;

pub const HANDSHAKE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(3);
pub const MAX_MESSAGE_BYTES: usize = 64 * 1024 * 1024;
const AUTH_EXPORTER_LABEL: &[u8] = b"EXPORTER-Loom-Local-Bridge-v1";

fn proof_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn websocket_config() -> tungstenite::protocol::WebSocketConfig {
    tungstenite::protocol::WebSocketConfig {
        max_message_size: Some(MAX_MESSAGE_BYTES),
        max_frame_size: Some(MAX_MESSAGE_BYTES),
        max_write_buffer_size: MAX_MESSAGE_BYTES + 1024 * 1024,
        ..Default::default()
    }
}

#[cfg(all(test, feature = "server"))]
mod auth_tests;
#[cfg(all(test, feature = "server"))]
mod tests;
