//! Connect to the operator-selected proxy, validating HTTPS proxy identity separately.

use std::io;
use std::sync::{Arc, OnceLock};

use reqwest::Url;
use tokio::net::TcpStream;

use super::ProxyStream;

pub(super) fn tls_config() -> Arc<rustls::ClientConfig> {
    static CONFIG: OnceLock<Arc<rustls::ClientConfig>> = OnceLock::new();
    CONFIG
        .get_or_init(|| {
            let roots =
                rustls::RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
            Arc::new(
                rustls::ClientConfig::builder_with_provider(Arc::new(
                    rustls::crypto::ring::default_provider(),
                ))
                .with_safe_default_protocol_versions()
                .expect("ring supports the default TLS versions")
                .with_root_certificates(roots)
                .with_no_client_auth(),
            )
        })
        .clone()
}

pub(super) async fn open(proxy: &Url, tls: Arc<rustls::ClientConfig>) -> io::Result<ProxyStream> {
    if !matches!(proxy.scheme(), "http" | "https") {
        return Err(io::Error::other("CONNECT requires an HTTP or HTTPS proxy"));
    }
    let host = proxy
        .host_str()
        .ok_or_else(|| io::Error::other("missing proxy host"))?;
    let port = proxy
        .port_or_known_default()
        .ok_or_else(|| io::Error::other("missing proxy port"))?;
    // Operator-selected proxies are not untrusted package destinations. In
    // particular local/private proxy servers must remain supported.
    let stream = if let Some(ip) = crate::network::parse_host_ip(host) {
        TcpStream::connect((ip, port)).await
    } else {
        TcpStream::connect((host, port)).await
    }
    .map_err(|_| io::Error::other("cannot connect to outbound proxy"))?;
    if proxy.scheme() == "http" {
        return Ok(Box::new(stream));
    }
    let name = match crate::network::parse_host_ip(host) {
        Some(ip) => rustls::pki_types::ServerName::IpAddress(ip.into()),
        None => rustls::pki_types::ServerName::try_from(host.to_owned())
            .map_err(|_| io::Error::other("invalid HTTPS proxy hostname"))?,
    };
    tokio_rustls::TlsConnector::from(tls)
        .connect(name, stream)
        .await
        .map(|stream| Box::new(stream) as ProxyStream)
        .map_err(|_| io::Error::other("HTTPS proxy TLS verification failed"))
}
