//! Snapshot proxy routing and keep protected requests on an authenticated IP-pinning relay.

use std::sync::Arc;

use hyper_util::client::proxy::matcher::{Intercept, Matcher};
use reqwest::{Proxy, Url};

use crate::network::{runtime_proxy, OutboundPolicy, RuntimeProxy};

pub(crate) struct Routing(Matcher);

impl Routing {
    pub(crate) fn snapshot(mode: RuntimeProxy) -> Result<Self, String> {
        Ok(Self(match mode {
            RuntimeProxy::System => Matcher::from_system(),
            RuntimeProxy::Disabled => Matcher::builder().build(),
            RuntimeProxy::Custom(url) => {
                let url = Url::parse(&url).map_err(|_| "invalid protected proxy URL".to_owned())?;
                if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
                    return Err("protected proxy transport requires HTTP or HTTPS".to_owned());
                }
                // URL normalization handles IDNA; a dropped matcher rule must never
                // turn an explicitly selected proxy into a direct connection.
                let matcher = Matcher::builder().all(url.as_str()).build();
                for target in ["http://target.invalid/", "https://target.invalid/"] {
                    if matcher.intercept(&target.parse().unwrap()).is_none() {
                        return Err("protected proxy URL cannot be routed".to_owned());
                    }
                }
                matcher
            }
        }))
    }

    pub(crate) fn select(&self, url: &Url) -> Option<Intercept> {
        self.0.intercept(&url.as_str().parse().ok()?)
    }
}

pub(crate) fn protected_proxy(policy: OutboundPolicy) -> Result<Option<Proxy>, String> {
    protected_proxy_with_mode(policy, runtime_proxy())
}

pub(crate) fn protected_proxy_with_mode(
    policy: OutboundPolicy,
    mode: RuntimeProxy,
) -> Result<Option<Proxy>, String> {
    if mode == RuntimeProxy::Disabled {
        return Ok(None);
    }
    let routing = Arc::new(Routing::snapshot(mode)?);
    let relay = Arc::new(crate::network_proxy_relay::Lease::start(
        routing.clone(),
        policy,
    )?);
    let token = relay.token.clone();
    // Capture the lease in the connector's proxy callback: dropping the last
    // client/request drops its listener and cancels every outstanding tunnel.
    Ok(Some(
        Proxy::custom(move |url| routing.select(url).map(|_| relay.url.clone()))
            .basic_auth("loom", &token),
    ))
}

/// Client-owned adapter for native HTTP stacks that cannot pin their DNS result.
/// Callers must force every request through it, including loopback requests, and
/// keep this guard alive until the native request/process has finished.
pub struct NativeHttpProxy(crate::network_proxy_relay::Lease);

impl NativeHttpProxy {
    pub fn new(policy: OutboundPolicy) -> Result<Self, String> {
        let mode = runtime_proxy();
        let routing = Arc::new(Routing::snapshot(mode)?);
        crate::network_proxy_relay::Lease::start(routing, policy).map(Self)
    }

    pub fn url(&self) -> &str {
        self.0.url.as_str()
    }

    /// Ephemeral local credential; never log it or forward it to an upstream peer.
    pub fn password(&self) -> &str {
        &self.0.token
    }
}

#[cfg(test)]
mod tests;
