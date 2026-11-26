//! Gateway origin validation, classification, and runtime proxy policy.

use std::net::IpAddr;
use std::sync::{OnceLock, RwLock};

use reqwest::{Proxy, Url};
use serde::{Deserialize, Serialize};

use crate::{GatewayError, GatewayResult};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum RuntimeProxy {
    System,
    Disabled,
    Custom(String),
}

impl Default for RuntimeProxy {
    fn default() -> Self {
        Self::System
    }
}

static RUNTIME_PROXY: OnceLock<RwLock<RuntimeProxy>> = OnceLock::new();

pub(crate) fn runtime_proxy_store() -> &'static RwLock<RuntimeProxy> {
    RUNTIME_PROXY.get_or_init(|| RwLock::new(RuntimeProxy::System))
}

pub fn configure_runtime_proxy(mode: &str, protocol: &str, address: &str) -> Result<(), String> {
    let proxy = match mode {
        "system" => RuntimeProxy::System,
        "disabled" => RuntimeProxy::Disabled,
        "custom" => {
            let url = format!("{}://{}", protocol.trim(), address.trim());
            Proxy::all(&url).map_err(|error| format!("invalid Gateway proxy: {error}"))?;
            RuntimeProxy::Custom(url)
        }
        _ => return Err(format!("unsupported Gateway proxy mode `{mode}`")),
    };
    *runtime_proxy_store()
        .write()
        .map_err(|_| "lock Gateway proxy settings".to_owned())? = proxy;
    Ok(())
}

/// Network origin classification for a configured Gateway endpoint.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GatewayOriginKind {
    Loopback,
    PrivateLan,
    RemoteTls,
}

impl GatewayOriginKind {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Loopback => "loopback",
            Self::PrivateLan => "lan",
            Self::RemoteTls => "remote",
        }
    }
}

fn is_private_lan_ip(ip: IpAddr) -> bool {
    match ip.to_canonical() {
        IpAddr::V4(v4) => v4.is_private(),
        IpAddr::V6(v6) => (v6.segments()[0] & 0xfe00) == 0xfc00,
    }
}

pub(crate) fn parse_validated_origin(raw: &str) -> GatewayResult<(Url, GatewayOriginKind)> {
    let base_url =
        Url::parse(raw.trim()).map_err(|error| GatewayError::InvalidBaseUrl(error.to_string()))?;
    if !matches!(base_url.scheme(), "http" | "https") {
        return Err(GatewayError::UnsupportedScheme);
    }
    if !base_url.username().is_empty() || base_url.password().is_some() {
        return Err(GatewayError::CredentialsInUrl);
    }
    if base_url.path() != "" && base_url.path() != "/" {
        return Err(GatewayError::InvalidBaseUrl(
            "base URL must be an origin without a path".to_owned(),
        ));
    }
    if base_url.query().is_some() || base_url.fragment().is_some() {
        return Err(GatewayError::InvalidBaseUrl(
            "base URL must not contain a query or fragment".to_owned(),
        ));
    }
    let parsed_ip = base_url
        .host_str()
        .and_then(|host| host.trim_matches(['[', ']']).parse::<IpAddr>().ok())
        .map(|ip| ip.to_canonical());
    let is_loopback = parsed_ip.is_some_and(|ip| ip.is_loopback());
    let is_lan = parsed_ip.is_some_and(is_private_lan_ip);

    // Cleartext HTTP is restricted to direct IP-literal loopback or RFC1918/ULA home LAN hosts,
    // never public Internet IPs or DNS names that could rebind outside the home LAN.
    if base_url.scheme() == "http" && !is_loopback && !is_lan {
        return Err(GatewayError::InsecureTransport);
    }
    let kind = if is_loopback {
        GatewayOriginKind::Loopback
    } else if is_lan {
        GatewayOriginKind::PrivateLan
    } else {
        GatewayOriginKind::RemoteTls
    };
    Ok((base_url, kind))
}

/// Validate a Gateway base URL and classify whether it targets loopback, private LAN, or remote TLS.
pub fn classify_gateway_origin(raw: &str) -> GatewayResult<GatewayOriginKind> {
    parse_validated_origin(raw).map(|(_, kind)| kind)
}
