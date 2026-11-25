//! Ephemeral, device-scoped locale requests. Otter remains the persisted setting owner.
mod model;
mod receiver;
mod routes;
mod store;

pub(super) use routes::{handle, request_for};
pub(super) use store::Store;

#[cfg(test)]
mod tests;
