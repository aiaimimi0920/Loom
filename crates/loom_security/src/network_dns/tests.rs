use super::*;
use std::net::{SocketAddr, TcpListener};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

struct FixedResolver {
    addresses: Vec<SocketAddr>,
    calls: Arc<AtomicUsize>,
}

impl Resolve for FixedResolver {
    fn resolve(&self, _name: Name) -> Resolving {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let addresses = self.addresses.clone();
        Box::pin(async move { Ok(Box::new(addresses.into_iter()) as Addrs) })
    }
}

fn resolver(addresses: Vec<SocketAddr>) -> (PolicyResolver, Arc<AtomicUsize>) {
    let calls = Arc::new(AtomicUsize::new(0));
    let inner = Arc::new(FixedResolver {
        addresses,
        calls: Arc::clone(&calls),
    });
    (
        PolicyResolver::with_inner(OutboundPolicy::default(), inner),
        calls,
    )
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

#[test]
fn validated_answer_set_is_returned_without_another_lookup() {
    let addresses = vec!["8.8.8.8:0".parse().unwrap(), "1.1.1.1:0".parse().unwrap()];
    let (resolver, calls) = resolver(addresses.clone());
    let actual = runtime()
        .block_on(resolver.resolve("public.example".parse().unwrap()))
        .unwrap();
    assert_eq!(actual.collect::<Vec<_>>(), addresses);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn mixed_empty_oversized_and_mapped_private_answers_fail_closed() {
    for addresses in [
        Vec::new(),
        vec!["8.8.8.8:0".parse().unwrap(), "10.0.0.1:0".parse().unwrap()],
        vec!["[::ffff:127.0.0.1]:0".parse().unwrap()],
        vec!["[::ffff:169.254.169.254]:0".parse().unwrap()],
        vec!["8.8.8.8:0".parse().unwrap(); 257],
    ] {
        let (resolver, _) = resolver(addresses);
        assert!(runtime()
            .block_on(resolver.resolve("public.example".parse().unwrap()))
            .is_err());
    }
}

#[test]
fn a_late_private_dns_answer_never_reaches_the_listener() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    // The earlier admission result was public; the actual connection lookup changes.
    validate_ip("8.8.8.8".parse().unwrap(), &OutboundPolicy::default()).unwrap();
    let (resolver, calls) = resolver(vec![listener.local_addr().unwrap()]);
    let client = reqwest::blocking::Client::builder()
        .no_proxy()
        .dns_resolver(Arc::new(resolver))
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap();
    assert!(client
        .get(format!(
            "https://rebound.invalid:{}/",
            listener.local_addr().unwrap().port()
        ))
        .send()
        .is_err());
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
}

#[test]
fn mapped_ipv6_literals_use_the_same_policy_as_ipv4() {
    use crate::network::{host_is_loopback_literal, validate_outbound_url};
    for host in [
        "[::ffff:127.0.0.1]",
        "[::ffff:169.254.169.254]",
        "[::ffff:10.1.2.3]",
        "[::ffff:172.16.1.1]",
        "[::ffff:192.168.1.1]",
        "[::ffff:0.0.0.0]",
        "[::ffff:224.0.0.1]",
    ] {
        let url = reqwest::Url::parse(&format!("https://{host}/")).unwrap();
        assert!(
            validate_outbound_url(&url, &OutboundPolicy::default()).is_err(),
            "{host}"
        );
    }
    assert!(host_is_loopback_literal("[::1]"));
    assert!(host_is_loopback_literal("[::ffff:127.0.0.1]"));
    let policy = OutboundPolicy {
        allow_http_loopback: true,
        ..Default::default()
    };
    assert!(validate_outbound_url(
        &reqwest::Url::parse("http://[::ffff:127.0.0.1]/").unwrap(),
        &policy
    )
    .is_ok());
    assert!(validate_outbound_url(
        &reqwest::Url::parse("https://[::ffff:8.8.8.8]/").unwrap(),
        &OutboundPolicy::default()
    )
    .is_ok());
}
