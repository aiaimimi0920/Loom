//! Loom-owned QR projection identity, coordination, persistence and transport.
//! Hook receives public projection state; the account signing key stays native.

mod central;
mod envelope;
mod identity;
mod model;
mod runtime;
mod runtime_model;
mod runtime_store;
#[cfg(test)]
mod runtime_test_central;
#[cfg(test)]
mod runtime_tests;
mod transport;
#[cfg(test)]
mod transport_tests;
mod validation;

pub use central::{CentralClient, CentralOperation};
pub use envelope::{Content, Envelope, EnvelopeSignature, Source};
pub use identity::{AccountSession, Identity};
pub use model::*;
pub use runtime::ProjectionRuntime;
pub use runtime_model::{LocalImage, LocalOperation};
pub use transport::{
    PullRequest, Snapshot, SnapshotGrant, SnapshotProvider, TransferPath, Transport, ALPN,
};

pub const PROTOCOL: &str = "neuro.qr-projection.v2";
pub const MAX_PNG_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_REVISION: u64 = 9_007_199_254_740_991;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error {
    pub status: u16,
    pub code: &'static str,
}

impl std::fmt::Display for Error {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.code)
    }
}
impl std::error::Error for Error {}
pub type Result<T> = std::result::Result<T, Error>;

pub(crate) fn error(status: u16, code: &'static str) -> Error {
    Error { status, code }
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
