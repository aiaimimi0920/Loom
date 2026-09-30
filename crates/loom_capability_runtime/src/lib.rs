//! Generic, identity-bound process host for Capability Plugin runtimes.

mod error;
mod frame;
mod host;
mod model_broker;
mod model_gateway;
mod model_provider;
mod model_schema;
mod process;
mod schema;
mod session;
mod snapshot;
mod verification;

pub use error::CapabilityHostError;
pub use frame::{read_runtime_frame, write_runtime_frame};
pub use host::{
    CapabilityInvocation, CapabilityInvocationOutput, CapabilityRuntimeHealth,
    CapabilityRuntimeHost, CapabilityRuntimePackage, CapabilityStagedResource, RuntimeHostLimits,
    UserGestureTarget,
};

#[cfg(test)]
mod tests;
