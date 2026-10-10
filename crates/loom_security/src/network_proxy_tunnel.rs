//! HTTP(S) CONNECT with an exact numeric authority and independent proxy TLS validation.

use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use hyper_util::client::proxy::matcher::Intercept;
use reqwest::Url;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};

use crate::network::{validate_ip, OutboundPolicy};

mod transport;

pub(crate) trait ProxyIo: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> ProxyIo for T {}
pub(crate) type ProxyStream = Box<dyn ProxyIo>;

pub(crate) async fn connect_selected(
    proxy: &Intercept,
    peer: SocketAddr,
    policy: &OutboundPolicy,
    timeout: Duration,
) -> io::Result<ProxyStream> {
    connect_with_config(proxy, peer, policy, timeout, transport::tls_config()).await
}

async fn connect_with_config(
    proxy: &Intercept,
    peer: SocketAddr,
    policy: &OutboundPolicy,
    timeout: Duration,
    tls: Arc<rustls::ClientConfig>,
) -> io::Result<ProxyStream> {
    validate_ip(peer.ip(), policy).map_err(io::Error::other)?;
    tokio::time::timeout(timeout, async {
        let url = Url::parse(&proxy.uri().to_string())
            .map_err(|_| io::Error::other("invalid outbound proxy configuration"))?;
        let mut stream = transport::open(&url, tls).await?;
        // URL normalization strips :80; CONNECT requires the exact port.
        let mut request = format!("CONNECT {peer} HTTP/1.1\r\nHost: {peer}\r\n").into_bytes();
        if let Some(auth) = proxy.basic_auth() {
            request.extend_from_slice(b"Proxy-Authorization: ");
            request.extend_from_slice(auth.as_bytes());
            request.extend_from_slice(b"\r\n");
        }
        request.extend_from_slice(b"\r\n");
        stream.write_all(&request).await?;
        let mut stream = BufReader::new(stream);
        let mut response = Vec::with_capacity(1024);
        while !response.ends_with(b"\r\n\r\n") {
            if response.len() == 16 * 1024 {
                return Err(io::Error::other(
                    "outbound proxy response header limit exceeded",
                ));
            }
            response.push(stream.read_u8().await?);
        }
        let mut headers = [httparse::EMPTY_HEADER; 64];
        let mut parsed = httparse::Response::new(&mut headers);
        if !parsed
            .parse(&response)
            .map_err(io::Error::other)?
            .is_complete()
        {
            return Err(io::Error::other("incomplete outbound proxy response"));
        }
        let status = parsed
            .code
            .ok_or_else(|| io::Error::other("missing proxy response status"))?;
        if !(200..300).contains(&status) {
            return Err(io::Error::other(format!(
                "outbound proxy CONNECT returned status {status}"
            )));
        }
        // Keep bytes buffered after the CONNECT response for end-to-end TLS.
        Ok(Box::new(stream) as ProxyStream)
    })
    .await
    .map_err(|_| io::Error::other("outbound proxy CONNECT deadline"))?
}

#[cfg(test)]
fn intercept(proxy: &Url) -> Intercept {
    hyper_util::client::proxy::matcher::Matcher::builder()
        .all(proxy.as_str())
        .build()
        .intercept(&"https://target.test/".parse().unwrap())
        .unwrap()
}

#[cfg(test)]
async fn connect(
    proxy: &Url,
    peer: SocketAddr,
    policy: &OutboundPolicy,
    timeout: Duration,
) -> io::Result<ProxyStream> {
    connect_selected(&intercept(proxy), peer, policy, timeout).await
}

#[cfg(test)]
mod https_tests;
#[cfg(test)]
mod tests;
