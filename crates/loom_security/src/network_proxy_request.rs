//! Authenticate, pin one destination, then relay bounded chunks without inspecting target TLS.

use std::io;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use reqwest::Url;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

use crate::network::OutboundPolicy;
use crate::network_proxy::Routing;

// MCP permits 128 KiB of configured headers across 64 fields. Reserve bounded
// space for the request line and protocol-managed headers as well.
const MAX_HEADERS: usize = 256 * 1024;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);
const IO_TIMEOUT: Duration = Duration::from_secs(60);

struct Request {
    url: Url,
    forwarded: Option<Vec<u8>>,
}

trait RelayIo: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> RelayIo for T {}

pub(crate) async fn serve(
    stream: TcpStream,
    routing: &Routing,
    policy: &OutboundPolicy,
    auth: &str,
) -> io::Result<()> {
    let mut incoming = BufReader::new(stream);
    let deadline = tokio::time::Instant::now() + CONNECT_TIMEOUT;
    let (request, mut tunnel) = tokio::time::timeout_at(deadline, async {
        let headers = read_headers(&mut incoming).await?;
        let request = match parse_request(&headers, auth) {
            Ok(request) => request,
            Err(error) => {
                if error.kind() == io::ErrorKind::PermissionDenied {
                    incoming.write_all(b"HTTP/1.1 407 Proxy Authentication Required\r\nProxy-Authenticate: Basic realm=\"Loom\"\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await?;
                }
                return Err(error);
            }
        };
        let proxy = routing.select(&request.url);
        let addresses = crate::network_dns::resolve_url(&request.url, policy).await?;
        let tunnel = connect_peers(&addresses, proxy.as_ref(), policy, deadline).await?;
        Ok::<_, io::Error>((request, tunnel))
    })
    .await
    .map_err(|_| io::Error::other("protected proxy connection deadline"))??;

    if let Some(headers) = request.forwarded {
        tokio::time::timeout(IO_TIMEOUT, tunnel.write_all(&headers))
            .await
            .map_err(|_| io::Error::other("protected proxy write deadline"))??;
    } else {
        incoming
            .write_all(b"HTTP/1.1 200 Connection established\r\n\r\n")
            .await?;
    }
    relay_streams(incoming, tunnel, IO_TIMEOUT).await
}

async fn connect_peers(
    addresses: &[std::net::SocketAddr],
    proxy: Option<&hyper_util::client::proxy::matcher::Intercept>,
    policy: &OutboundPolicy,
    deadline: tokio::time::Instant,
) -> io::Result<Box<dyn RelayIo>> {
    // All answers were validated together. Retries use this fixed set, never
    // another lookup. Reserve time for every remaining approved candidate.
    for (index, peer) in addresses.iter().copied().enumerate() {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            break;
        }
        let count = u32::try_from(addresses.len() - index).unwrap_or(u32::MAX);
        let budget = if count == 1 {
            remaining
        } else {
            (remaining / count).min(Duration::from_secs(5))
        };
        let connected = tokio::time::timeout(budget, async {
            match proxy {
                Some(proxy) => {
                    crate::network_proxy_tunnel::connect_selected(proxy, peer, policy, budget)
                        .await
                        .map(|stream| Box::new(stream) as Box<dyn RelayIo>)
                }
                // Native direct routes also share the bounded fallback budget.
                None => TcpStream::connect(peer)
                    .await
                    .map(|stream| Box::new(stream) as Box<dyn RelayIo>),
            }
        })
        .await;
        if let Ok(Ok(tunnel)) = connected {
            return Ok(tunnel);
        }
    }
    Err(io::Error::other(
        "no approved outbound proxy peer could be connected",
    ))
}

async fn relay_streams(
    incoming: impl AsyncRead + AsyncWrite + Unpin,
    tunnel: impl AsyncRead + AsyncWrite + Unpin,
    idle_timeout: Duration,
) -> io::Result<()> {
    let (mut client_read, mut client_write) = tokio::io::split(incoming);
    let (mut target_read, mut target_write) = tokio::io::split(tunnel);
    let start = tokio::time::Instant::now();
    let activity = AtomicU64::new(0);
    // Idle means neither direction made progress. Independent read deadlines
    // would abort a healthy long download merely because its upload is silent.
    tokio::select! {
        result = async {
            tokio::try_join!(
                copy_bounded(&mut client_read, &mut target_write, start, &activity),
                copy_bounded(&mut target_read, &mut client_write, start, &activity),
            )?;
            Ok(())
        } => result,
        _ = idle_deadline(start, &activity, idle_timeout) =>
            Err(io::Error::other("protected proxy idle deadline")),
    }
}

async fn idle_deadline(start: tokio::time::Instant, activity: &AtomicU64, timeout: Duration) {
    loop {
        let observed = activity.load(Ordering::Relaxed);
        tokio::time::sleep_until(start + Duration::from_millis(observed) + timeout).await;
        if activity.load(Ordering::Relaxed) == observed {
            return;
        }
    }
}

async fn read_headers(stream: &mut (impl AsyncRead + Unpin)) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::with_capacity(1024);
    while !bytes.ends_with(b"\r\n\r\n") {
        if bytes.len() == MAX_HEADERS {
            return Err(io::Error::other("protected proxy header limit exceeded"));
        }
        bytes.push(stream.read_u8().await?);
    }
    Ok(bytes)
}

fn parse_request(bytes: &[u8], expected_auth: &str) -> io::Result<Request> {
    let mut fields = [httparse::EMPTY_HEADER; 128];
    let mut request = httparse::Request::new(&mut fields);
    if !request
        .parse(bytes)
        .map_err(io::Error::other)?
        .is_complete()
    {
        return Err(io::Error::other("incomplete protected proxy request"));
    }
    let auth = request
        .headers
        .iter()
        .filter(|header| header.name.eq_ignore_ascii_case("proxy-authorization"))
        .map(|header| header.value)
        .collect::<Vec<_>>();
    if auth.len() != 1 || !constant_time_equal(auth[0], expected_auth.as_bytes()) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "protected proxy authentication failed",
        ));
    }
    let method = request
        .method
        .ok_or_else(|| io::Error::other("missing proxy method"))?;
    let path = request
        .path
        .ok_or_else(|| io::Error::other("missing proxy target"))?;
    let connect = method == "CONNECT";
    let url = Url::parse(&if connect {
        format!("https://{path}/")
    } else {
        path.to_owned()
    })
    .map_err(|_| io::Error::other("invalid proxy target"))?;
    if !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || (connect && (url.path() != "/" || url.query().is_some()))
        || (!connect && url.scheme() != "http")
    {
        return Err(io::Error::other("invalid protected proxy request target"));
    }
    let forwarded = if connect {
        None
    } else {
        Some(forward_headers(&request, &url)?)
    };
    Ok(Request { url, forwarded })
}

fn forward_headers(request: &httparse::Request<'_, '_>, url: &Url) -> io::Result<Vec<u8>> {
    let method = request.method.unwrap_or_default();
    reqwest::Method::from_bytes(method.as_bytes()).map_err(io::Error::other)?;
    let mut path = url.path().to_owned();
    if let Some(query) = url.query() {
        path.push('?');
        path.push_str(query);
    }
    let mut host = url
        .host_str()
        .ok_or_else(|| io::Error::other("missing proxy host"))?
        .to_owned();
    if let Some(port) = url.port() {
        host.push_str(&format!(":{port}"));
    }
    let mut output =
        format!("{method} {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n").into_bytes();
    for header in request.headers.iter() {
        if [
            "proxy-authorization",
            "proxy-connection",
            "host",
            "connection",
            "keep-alive",
        ]
        .iter()
        .any(|name| header.name.eq_ignore_ascii_case(name))
        {
            continue;
        }
        output.extend_from_slice(header.name.as_bytes());
        output.extend_from_slice(b": ");
        output.extend_from_slice(header.value);
        output.extend_from_slice(b"\r\n");
    }
    output.extend_from_slice(b"\r\n");
    Ok(output)
}

fn constant_time_equal(actual: &[u8], expected: &[u8]) -> bool {
    actual.len() == expected.len()
        && actual
            .iter()
            .zip(expected)
            .fold(0u8, |difference, (a, b)| difference | (a ^ b))
            == 0
}

async fn copy_bounded(
    reader: &mut (impl AsyncRead + Unpin),
    writer: &mut (impl AsyncWrite + Unpin),
    start: tokio::time::Instant,
    activity: &AtomicU64,
) -> io::Result<()> {
    let mut bytes = [0; 16 * 1024];
    loop {
        let length = reader.read(&mut bytes).await?;
        if length == 0 {
            return tokio::time::timeout(IO_TIMEOUT, writer.shutdown())
                .await
                .map_err(|_| io::Error::other("protected proxy shutdown deadline"))?;
        }
        activity.fetch_max(start.elapsed().as_millis() as u64, Ordering::Relaxed);
        tokio::time::timeout(IO_TIMEOUT, writer.write_all(&bytes[..length]))
            .await
            .map_err(|_| io::Error::other("protected proxy write deadline"))??;
        activity.fetch_max(start.elapsed().as_millis() as u64, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests;
