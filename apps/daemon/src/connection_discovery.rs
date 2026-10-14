//! Opt-in, one-hop LAN announcements. An announcement is a hint, never an authentication proof.
use crate::SharedDeviceRegistryStore;
use std::{
    net::{SocketAddr, UdpSocket},
    sync::mpsc,
    thread,
    time::Duration,
};

const DISCOVERY_ADDRESS: &str = "255.255.255.255:49779";

pub(crate) struct Announcer {
    stop: mpsc::Sender<()>,
    worker: Option<thread::JoinHandle<()>>,
}

impl Announcer {
    pub fn start(registry: SharedDeviceRegistryStore) -> std::io::Result<Self> {
        Self::start_to(
            registry,
            DISCOVERY_ADDRESS.parse().expect("static discovery address"),
            Duration::from_secs(2),
        )
    }

    fn start_to(
        registry: SharedDeviceRegistryStore,
        destination: SocketAddr,
        interval: Duration,
    ) -> std::io::Result<Self> {
        let (stop, wait) = mpsc::channel();
        let worker = thread::Builder::new()
            .name("loom-discovery".into())
            .spawn(move || {
                let mut socket = None;
                loop {
                    let announcement = registry
                        .try_lock()
                        .ok()
                        .map(|store| store.connection_policy.public_info())
                        .filter(|info| info.discoverable && info.endpoint.is_some());
                    if let Some(info) = announcement {
                        if socket.is_none() {
                            socket = UdpSocket::bind("0.0.0.0:0").ok().filter(|socket| {
                                socket.set_broadcast(true).is_ok()
                                    && socket.set_ttl(1).is_ok()
                                    && socket
                                        .set_write_timeout(Some(Duration::from_millis(100)))
                                        .is_ok()
                            });
                        }
                        if let (Some(socket), Ok(packet)) = (&socket, serde_json::to_vec(&info)) {
                            if packet.len() <= 1024 {
                                let _ = socket.send_to(&packet, destination);
                            }
                        }
                    } else {
                        socket = None;
                    }
                    match wait.recv_timeout(interval) {
                        Err(mpsc::RecvTimeoutError::Timeout) => (),
                        _ => break,
                    }
                }
            })?;
        Ok(Self {
            stop,
            worker: Some(worker),
        })
    }
}

#[cfg(test)]
pub(crate) fn start_test_announcer(
    registry: SharedDeviceRegistryStore,
    destination: SocketAddr,
) -> std::io::Result<Announcer> {
    Announcer::start_to(registry, destination, Duration::from_millis(30))
}

impl Drop for Announcer {
    fn drop(&mut self) {
        let _ = self.stop.send(());
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
