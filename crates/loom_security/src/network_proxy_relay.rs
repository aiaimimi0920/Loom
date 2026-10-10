//! Bounded, client-owned loopback transport adapter; never a general-purpose proxy.

use std::sync::{Arc, OnceLock};
use std::time::Duration;

use base64::Engine;
use rand_core::RngCore;
use reqwest::Url;
use tokio::runtime::Runtime;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio::task::{AbortHandle, JoinSet};

use crate::network::OutboundPolicy;
use crate::network_proxy::Routing;

const MAX_CLIENTS: usize = 128;
const MAX_CONNECTIONS: usize = 128;
const MAX_CLIENT_CONNECTIONS: usize = 16;
static RUNTIME: OnceLock<Result<Runtime, String>> = OnceLock::new();
static CLIENTS: OnceLock<Arc<Semaphore>> = OnceLock::new();
static CONNECTIONS: OnceLock<Arc<Semaphore>> = OnceLock::new();

pub(crate) struct Lease {
    pub(crate) url: Url,
    pub(crate) token: String,
    task: AbortHandle,
}

impl Lease {
    pub(crate) fn start(routing: Arc<Routing>, policy: OutboundPolicy) -> Result<Self, String> {
        let client_slot = CLIENTS
            .get_or_init(|| Arc::new(Semaphore::new(MAX_CLIENTS)))
            .clone()
            .try_acquire_owned()
            .map_err(|_| "protected proxy client limit reached".to_owned())?;
        let runtime = RUNTIME
            .get_or_init(|| {
                tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(2)
                    .thread_name("loom-proxy")
                    .enable_all()
                    .build()
                    .map_err(|_| "cannot start protected proxy runtime".to_owned())
            })
            .as_ref()
            .map_err(Clone::clone)?;
        let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .map_err(|_| "cannot bind protected proxy transport".to_owned())?;
        listener
            .set_nonblocking(true)
            .map_err(|error| error.to_string())?;
        let url = Url::parse(&format!(
            "http://{}",
            listener.local_addr().map_err(|e| e.to_string())?
        ))
        .map_err(|error| error.to_string())?;
        let mut secret = [0; 32];
        rand_core::OsRng
            .try_fill_bytes(&mut secret)
            .map_err(|_| "cannot generate protected proxy authentication".to_owned())?;
        let token = base64::engine::general_purpose::STANDARD.encode(secret);
        let auth = format!(
            "Basic {}",
            base64::engine::general_purpose::STANDARD.encode(format!("loom:{token}"))
        );
        let task = runtime.spawn(serve(listener, routing, policy, auth, client_slot));
        Ok(Self {
            url,
            token,
            task: task.abort_handle(),
        })
    }
}

impl Drop for Lease {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn serve(
    listener: std::net::TcpListener,
    routing: Arc<Routing>,
    policy: OutboundPolicy,
    auth: String,
    _client_slot: OwnedSemaphorePermit,
) {
    let Ok(listener) = tokio::net::TcpListener::from_std(listener) else {
        return;
    };
    let slots = Arc::new(Semaphore::new(MAX_CLIENT_CONNECTIONS));
    let global = CONNECTIONS.get_or_init(|| Arc::new(Semaphore::new(MAX_CONNECTIONS)));
    let mut tasks = JoinSet::new();
    loop {
        tokio::select! {
            Some(_) = tasks.join_next(), if !tasks.is_empty() => {},
            incoming = listener.accept() => {
                let Ok((stream, _)) = incoming else { break };
                let Ok(local_slot) = slots.clone().try_acquire_owned() else { continue };
                let Ok(global_slot) = global.clone().try_acquire_owned() else { continue };
                let routing = routing.clone();
                let policy = policy.clone();
                let auth = auth.clone();
                tasks.spawn(async move {
                    let (_local_slot, _global_slot) = (local_slot, global_slot);
                    // The outer bound also covers a client that trickles bytes forever.
                    let _ = tokio::time::timeout(
                        Duration::from_secs(15 * 60),
                        crate::network_proxy_request::serve(stream, &routing, &policy, &auth),
                    ).await;
                });
            }
        }
    }
    // JoinSet aborts all children on both ordinary return and cancellation.
}

#[cfg(test)]
mod tests;
