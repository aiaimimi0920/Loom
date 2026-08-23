//! Outbound network policy and bounded HTTP helpers.
//!
//! Moved out of `loom_tool_registry` so `loom_mcp` can enforce the same policy; see the
//! crate-level documentation for why a leaf crate is required.

use std::io::Read;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, ToSocketAddrs};
use std::path::{Component, Path, Prefix};
use std::sync::{OnceLock, RwLock};
use std::time::Duration;

use reqwest::blocking::{Client, ClientBuilder, Response};
use reqwest::redirect;
use reqwest::{Client as AsyncClient, ClientBuilder as AsyncClientBuilder, Proxy, Url};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RuntimeProxy {
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

fn runtime_proxy_store() -> &'static RwLock<RuntimeProxy> {
    RUNTIME_PROXY.get_or_init(|| RwLock::new(RuntimeProxy::System))
}

pub fn configure_runtime_proxy(mode: &str, protocol: &str, address: &str) -> Result<(), String> {
    let proxy = match mode {
        "system" => RuntimeProxy::System,
        "disabled" => RuntimeProxy::Disabled,
        "custom" => {
            let address = address.trim();
            if address.is_empty() {
                return Err("custom proxy address is required".to_owned());
            }
            let url = format!("{}://{}", protocol.trim(), address);
            Url::parse(&url).map_err(|error| format!("invalid proxy URL: {error}"))?;
            RuntimeProxy::Custom(url)
        }
        _ => return Err(format!("unsupported proxy mode `{mode}`")),
    };
    *runtime_proxy_store()
        .write()
        .map_err(|_| "lock runtime proxy settings".to_owned())? = proxy;
    Ok(())
}

pub fn runtime_proxy() -> RuntimeProxy {
    runtime_proxy_store()
        .read()
        .map(|proxy| proxy.clone())
        .unwrap_or_default()
}

pub fn apply_runtime_proxy(builder: ClientBuilder) -> Result<ClientBuilder, String> {
    match runtime_proxy() {
        RuntimeProxy::System => Ok(builder),
        RuntimeProxy::Disabled => Ok(builder.no_proxy()),
        RuntimeProxy::Custom(url) => Proxy::all(&url)
            .map(|proxy| builder.proxy(proxy))
            .map_err(|error| format!("configure proxy `{url}`: {error}")),
    }
}

pub fn apply_runtime_proxy_async(
    builder: AsyncClientBuilder,
) -> Result<AsyncClientBuilder, String> {
    match runtime_proxy() {
        RuntimeProxy::System => Ok(builder),
        RuntimeProxy::Disabled => Ok(builder.no_proxy()),
        RuntimeProxy::Custom(url) => Proxy::all(&url)
            .map(|proxy| builder.proxy(proxy))
            .map_err(|error| format!("configure proxy `{url}`: {error}")),
    }
}

#[derive(Clone, Debug)]
pub struct OutboundPolicy {
    pub allow_http_loopback: bool,
    pub allow_private_networks: bool,
    pub allowed_domains: Vec<String>,
    pub max_redirects: usize,
}

impl Default for OutboundPolicy {
    fn default() -> Self {
        Self {
            allow_http_loopback: false,
            allow_private_networks: false,
            allowed_domains: Vec::new(),
            max_redirects: 5,
        }
    }
}

pub fn secure_client(
    user_agent: &str,
    timeout: Duration,
    policy: OutboundPolicy,
) -> Result<Client, String> {
    let redirect_policy = policy.clone();
    let builder = Client::builder()
        .user_agent(user_agent)
        .timeout(timeout)
        .redirect(redirect::Policy::custom(move |attempt| {
            if attempt.previous().len() >= redirect_policy.max_redirects {
                return attempt.error("redirect limit exceeded");
            }
            match validate_outbound_url(attempt.url(), &redirect_policy) {
                Ok(()) => attempt.follow(),
                Err(error) => attempt.error(error),
            }
        }));
    apply_runtime_proxy(builder)?
        .build()
        .map_err(|error| error.to_string())
}

pub fn secure_async_client(
    user_agent: &str,
    timeout: Duration,
    policy: OutboundPolicy,
) -> Result<AsyncClient, String> {
    let redirect_policy = policy.clone();
    let builder = AsyncClient::builder()
        .user_agent(user_agent)
        .timeout(timeout)
        .redirect(redirect::Policy::custom(move |attempt| {
            if attempt.previous().len() >= redirect_policy.max_redirects {
                return attempt.error("redirect limit exceeded");
            }
            match validate_outbound_url(attempt.url(), &redirect_policy) {
                Ok(()) => attempt.follow(),
                Err(error) => attempt.error(error),
            }
        }));
    apply_runtime_proxy_async(builder)?
        .build()
        .map_err(|error| error.to_string())
}

pub fn get_bounded(
    client: &Client,
    url: &str,
    policy: &OutboundPolicy,
    max_bytes: usize,
) -> Result<Vec<u8>, String> {
    let url = Url::parse(url).map_err(|error| format!("invalid URL: {error}"))?;
    validate_outbound_url(&url, policy)?;
    let response = client
        .get(url.clone())
        .send()
        .and_then(|response| response.error_for_status())
        .map_err(|error| format!("{url}: {error}"))?;
    read_bounded_response(response, max_bytes)
}

pub fn read_bounded_response(mut response: Response, max_bytes: usize) -> Result<Vec<u8>, String> {
    if response
        .content_length()
        .is_some_and(|content_length| content_length > max_bytes as u64)
    {
        return Err(format!("response exceeds {max_bytes} bytes"));
    }
    let mut bytes = Vec::with_capacity(
        response
            .content_length()
            .and_then(|length| usize::try_from(length).ok())
            .unwrap_or(0)
            .min(max_bytes),
    );
    response
        .by_ref()
        .take(max_bytes as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() > max_bytes {
        return Err(format!("response exceeds {max_bytes} bytes"));
    }
    Ok(bytes)
}

pub fn validate_outbound_url(url: &Url, policy: &OutboundPolicy) -> Result<(), String> {
    validate_url_without_dns(url, policy)?;
    let host = url
        .host_str()
        .ok_or_else(|| "URL host is required".to_owned())?;
    let port = url
        .port_or_known_default()
        .ok_or_else(|| "URL port is required".to_owned())?;
    if let Ok(ip) = host.parse::<IpAddr>() {
        return validate_ip(ip, policy);
    }
    let addresses = (host, port)
        .to_socket_addrs()
        .map_err(|error| format!("cannot resolve URL host `{host}`: {error}"))?
        .collect::<Vec<_>>();
    if addresses.is_empty() {
        return Err(format!("URL host `{host}` resolved to no addresses"));
    }
    for address in addresses {
        validate_ip(address.ip(), policy)?;
    }
    Ok(())
}

/// Validate a string that is supposed to name a *local* file before the host reads or writes
/// it on behalf of untrusted content.
///
/// `validate_outbound_url` only sees values that parse as a URL with an `http`/`https`
/// scheme, so it never inspects a plain filesystem path. A UNC path (`\\host\share\...`),
/// its verbatim form (`\\?\UNC\host\share`), a forward-slash UNC (`//host/share`) or a Win32
/// device path (`\\.\PhysicalDrive0`) all reach the network or the raw device without ever
/// looking like a URL, which is the gap this covers.
pub fn validate_local_path(path: &Path) -> Result<(), String> {
    let display = path.display().to_string();
    if display.starts_with("//") {
        return Err(format!("remote or device path `{display}` is not allowed"));
    }
    match path.components().next() {
        Some(Component::Prefix(prefix)) => match prefix.kind() {
            Prefix::UNC(..) | Prefix::VerbatimUNC(..) | Prefix::DeviceNS(..) => {
                Err(format!("remote or device path `{display}` is not allowed"))
            }
            _ => Ok(()),
        },
        _ => Ok(()),
    }
}

fn validate_url_without_dns(url: &Url, policy: &OutboundPolicy) -> Result<(), String> {
    let host = url
        .host_str()
        .ok_or_else(|| "URL host is required".to_owned())?;
    if !domain_allowed(host, &policy.allowed_domains) {
        return Err(format!("URL host `{host}` is not declared by the package"));
    }
    match url.scheme() {
        "https" => Ok(()),
        "http" if policy.allow_http_loopback && host_is_loopback_literal(host) => Ok(()),
        "http" => {
            Err("HTTP is only allowed for explicit loopback development endpoints".to_owned())
        }
        scheme => Err(format!("unsupported URL scheme `{scheme}`")),
    }
}

fn domain_allowed(host: &str, allowed: &[String]) -> bool {
    if allowed.is_empty() {
        return true;
    }
    let host = host.to_ascii_lowercase();
    allowed.iter().any(|pattern| {
        let pattern = pattern.trim().to_ascii_lowercase();
        pattern == host
            || pattern
                .strip_prefix("*.")
                .is_some_and(|suffix| host.ends_with(&format!(".{suffix}")) && host != suffix)
    })
}

/// Report whether `host` names the local machine without a DNS lookup.
///
/// Callers that must decide whether plain `http` is acceptable need this answer before any
/// resolution happens, which is why it only accepts `localhost` and loopback IP literals: a
/// name that merely resolves to `127.0.0.1` today is under the control of whoever answers DNS.
#[must_use]
pub fn host_is_loopback_literal(host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<IpAddr>()
            .is_ok_and(|address| address.is_loopback())
}

fn validate_ip(ip: IpAddr, policy: &OutboundPolicy) -> Result<(), String> {
    if ip.is_loopback() {
        return if policy.allow_http_loopback {
            Ok(())
        } else {
            Err(format!("loopback address `{ip}` is not allowed"))
        };
    }
    if policy.allow_private_networks {
        return Ok(());
    }
    let private = match ip {
        IpAddr::V4(ip) => ipv4_is_private_or_special(ip),
        IpAddr::V6(ip) => ipv6_is_private_or_special(ip),
    };
    if private {
        Err(format!("private or special address `{ip}` is not allowed"))
    } else {
        Ok(())
    }
}

fn ipv4_is_private_or_special(ip: Ipv4Addr) -> bool {
    ip.is_private()
        || ip.is_link_local()
        || ip.is_broadcast()
        || ip.is_documentation()
        || ip.is_multicast()
        || ip.is_unspecified()
        || ip.octets()[0] == 0
        || ip.octets()[0] >= 240
}

fn ipv6_is_private_or_special(ip: Ipv6Addr) -> bool {
    ip.is_unspecified()
        || ip.is_multicast()
        || (ip.segments()[0] & 0xfe00) == 0xfc00
        || (ip.segments()[0] & 0xffc0) == 0xfe80
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn blocks_metadata_private_and_unlisted_domains() {
        let policy = OutboundPolicy::default();
        assert!(validate_outbound_url(
            &Url::parse("http://169.254.169.254/latest/meta-data").unwrap(),
            &policy
        )
        .is_err());
        let policy = OutboundPolicy {
            allowed_domains: vec!["api.example.com".to_owned()],
            ..OutboundPolicy::default()
        };
        assert!(validate_url_without_dns(
            &Url::parse("https://other.example.com/v1").unwrap(),
            &policy
        )
        .is_err());
    }

    #[test]
    fn explicit_loopback_development_policy_allows_local_http() {
        let policy = OutboundPolicy {
            allow_http_loopback: true,
            ..OutboundPolicy::default()
        };
        assert!(
            validate_outbound_url(&Url::parse("http://127.0.0.1:8765/test").unwrap(), &policy)
                .is_ok()
        );
    }

    #[test]
    fn runtime_proxy_modes_are_validated_and_applied() {
        configure_runtime_proxy("disabled", "http", "").unwrap();
        assert_eq!(runtime_proxy(), RuntimeProxy::Disabled);
        assert!(apply_runtime_proxy(Client::builder()).is_ok());

        configure_runtime_proxy("custom", "socks5", "127.0.0.1:7890").unwrap();
        assert_eq!(
            runtime_proxy(),
            RuntimeProxy::Custom("socks5://127.0.0.1:7890".to_owned())
        );
        assert!(configure_runtime_proxy("custom", "http", "").is_err());
        configure_runtime_proxy("system", "http", "").unwrap();
    }

    #[test]
    fn non_http_schemes_are_rejected_before_any_lookup() {
        let policy = OutboundPolicy::default();
        for value in [
            "file:///C:/Windows/win.ini",
            "ftp://example.com/payload",
            "smb://example.com/share",
        ] {
            let url = Url::parse(value).expect("parse");
            assert!(
                validate_url_without_dns(&url, &policy).is_err(),
                "scheme of `{value}` must be rejected"
            );
        }
    }

    #[test]
    fn remote_and_device_paths_are_rejected() {
        for value in [
            r"\\fileserver\share\payload.zip",
            r"\\?\UNC\fileserver\share\payload.zip",
            r"\\.\PhysicalDrive0",
            "//fileserver/share/payload.zip",
        ] {
            assert!(
                validate_local_path(&PathBuf::from(value)).is_err(),
                "`{value}` must be rejected"
            );
        }
    }

    #[test]
    fn ordinary_local_paths_are_accepted() {
        for value in [
            r"C:\Users\example\AppData\Local\Loom\cache\art.zip",
            r"\\?\C:\Users\example\long-path\art.zip",
            "relative/inside/package.zip",
        ] {
            assert!(
                validate_local_path(&PathBuf::from(value)).is_ok(),
                "`{value}` must be accepted"
            );
        }
    }
}
