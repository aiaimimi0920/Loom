//! Validate the exact DNS answer set handed to the HTTP connector.

use std::io;
use std::sync::Arc;

use reqwest::dns::{Addrs, Name, Resolve, Resolving};

use crate::network::{validate_ip, OutboundPolicy};

pub(crate) struct PolicyResolver {
    policy: OutboundPolicy,
    inner: Arc<dyn Resolve>,
}

impl PolicyResolver {
    pub(crate) fn new(policy: OutboundPolicy) -> Self {
        Self {
            policy,
            inner: Arc::new(SystemResolver),
        }
    }

    #[cfg(test)]
    fn with_inner(policy: OutboundPolicy, inner: Arc<dyn Resolve>) -> Self {
        Self { policy, inner }
    }
}

impl Resolve for PolicyResolver {
    fn resolve(&self, name: Name) -> Resolving {
        let lookup = self.inner.resolve(name);
        let policy = self.policy.clone();
        Box::pin(async move {
            let mut addresses = Vec::new();
            for address in lookup.await? {
                // Do not truncate a mixed answer set into an apparently safe one.
                if addresses.len() == 256 {
                    return Err(io::Error::other("DNS answer limit exceeded").into());
                }
                validate_ip(address.ip(), &policy).map_err(io::Error::other)?;
                addresses.push(address);
            }
            if addresses.is_empty() {
                return Err(io::Error::other("DNS returned no addresses").into());
            }
            // Returning these same addresses prevents a second unchecked resolution.
            Ok(Box::new(addresses.into_iter()) as Addrs)
        })
    }
}

struct SystemResolver;

pub(crate) async fn resolve_url(
    url: &reqwest::Url,
    policy: &OutboundPolicy,
) -> io::Result<Vec<std::net::SocketAddr>> {
    crate::network::validate_url_without_dns(url, policy).map_err(io::Error::other)?;
    let host = url
        .host_str()
        .ok_or_else(|| io::Error::other("missing URL host"))?;
    let port = url
        .port_or_known_default()
        .ok_or_else(|| io::Error::other("missing URL port"))?;
    if let Some(ip) = crate::network::parse_host_ip(host) {
        validate_ip(ip, policy).map_err(io::Error::other)?;
        return Ok(vec![std::net::SocketAddr::new(ip, port)]);
    }
    let mut addresses = Vec::new();
    for peer in tokio::net::lookup_host((host, port)).await? {
        if addresses.len() == 256 {
            return Err(io::Error::other("DNS answer limit exceeded"));
        }
        validate_ip(peer.ip(), policy).map_err(io::Error::other)?;
        addresses.push(peer);
    }
    if addresses.is_empty() {
        return Err(io::Error::other("DNS returned no addresses"));
    }
    Ok(addresses)
}

impl Resolve for SystemResolver {
    fn resolve(&self, name: Name) -> Resolving {
        Box::pin(async move {
            let addresses = tokio::net::lookup_host((name.as_str().to_owned(), 0)).await?;
            Ok(Box::new(addresses) as Addrs)
        })
    }
}

#[cfg(test)]
mod tests;
