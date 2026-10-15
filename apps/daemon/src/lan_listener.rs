//! Owned, opt-in LAN listener. State and key persist privately; every worker has
//! absolute I/O deadlines and shutdown closes active sockets before joining.
use anyhow::{ensure, Context, Result};
use loom_local_channel::lan_tls::{self, LanIdentityDocument};
use serde::{Deserialize, Serialize};
use std::{
    io::Read,
    net::{Ipv4Addr, Shutdown, SocketAddr, TcpListener, TcpStream, UdpSocket},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Document {
    enabled: bool,
    host: Ipv4Addr,
    port: u16,
    identity: Option<LanIdentityDocument>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Update {
    enabled: bool,
    host: Ipv4Addr,
    port: u16,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct View {
    enabled: bool,
    listening: bool,
    host: Ipv4Addr,
    port: u16,
    endpoint: Option<String>,
    error: Option<String>,
}

pub(crate) struct Server {
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}
impl Server {
    fn running(&self) -> bool {
        !self.stop.load(Ordering::Acquire)
            && self
                .worker
                .as_ref()
                .is_some_and(|worker| !worker.is_finished())
    }
    fn start(document: &Document, backend: SocketAddr, ingress: String) -> Result<Self> {
        ensure!(
            backend.ip().is_loopback(),
            "LAN requires a loopback management listener"
        );
        let listener = TcpListener::bind((document.host, document.port))
            .context("无法监听局域网地址，请检查 IP 和端口占用")?;
        listener.set_nonblocking(true)?;
        let identity = document
            .identity
            .clone()
            .ok_or_else(|| anyhow::anyhow!("LAN identity unavailable"))?;
        identity.server()?;
        let host = listener.local_addr()?.to_string();
        let stop = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&stop);
        let worker = thread::Builder::new()
            .name("loom-lan".into())
            .spawn(move || {
                let mut workers: Vec<(JoinHandle<()>, TcpStream)> = Vec::new();
                while !flag.load(Ordering::Acquire) {
                    let mut index = 0;
                    while index < workers.len() {
                        if workers[index].0.is_finished() {
                            let _ = workers.swap_remove(index).0.join();
                        } else {
                            index += 1;
                        }
                    }
                    match listener.accept() {
                        Ok((tcp, peer)) if workers.len() < 4 && peer.ip().is_ipv4() => {
                            let std::net::IpAddr::V4(ip) = peer.ip() else {
                                continue;
                            };
                            if !ip.is_private() && !ip.is_loopback() {
                                continue;
                            }
                            let Ok(interrupt) = tcp.try_clone() else {
                                continue;
                            };
                            let identity = identity.clone();
                            let ingress = ingress.clone();
                            let host = host.clone();
                            if let Ok(worker) = thread::Builder::new()
                                .name("loom-lan-request".into())
                                .spawn(move || {
                                    if let Ok(mut stream) = lan_tls::accept(&identity, tcp) {
                                        crate::lan_http::serve(
                                            &mut stream,
                                            &host,
                                            backend,
                                            &ingress,
                                        );
                                    }
                                })
                            {
                                workers.push((worker, interrupt));
                            }
                        }
                        Ok(_) => (),
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(10))
                        }
                        Err(_) => break,
                    }
                }
                flag.store(true, Ordering::Release);
                for (_, socket) in &workers {
                    let _ = socket.shutdown(Shutdown::Both);
                }
                for (worker, _) in workers {
                    let _ = worker.join();
                }
            })?;
        Ok(Self {
            stop,
            worker: Some(worker),
        })
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

pub(crate) struct Manager {
    ingress: String,
    path: PathBuf,
    backend: SocketAddr,
    document: Document,
    server: Option<Server>,
    error: Option<String>,
}
impl Manager {
    pub fn open(path: &Path, backend: SocketAddr) -> Result<Self> {
        let document = match std::fs::File::open(path) {
            Ok(file) => {
                let mut bytes = Vec::new();
                file.take(32769).read_to_end(&mut bytes)?;
                ensure!(bytes.len() <= 32768, "LAN configuration too large");
                serde_json::from_slice(&bytes).context("读取局域网配置失败")?
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Document {
                enabled: false,
                host: suggested_host(),
                port: 8766,
                identity: None,
            },
            Err(error) => return Err(error.into()),
        };
        validate(&document)?;
        let mut manager = Self {
            ingress: uuid::Uuid::new_v4().to_string(),
            path: path.into(),
            backend,
            document,
            server: None,
            error: None,
        };
        if manager.document.enabled {
            match Server::start(&manager.document, backend, manager.ingress.clone()) {
                Ok(server) => manager.server = Some(server),
                Err(error) => manager.error = Some(error.to_string()),
            }
        }
        Ok(manager)
    }

    pub fn endpoint(&self) -> Option<String> {
        self.server
            .as_ref()
            .filter(|server| server.running())
            .map(|_| format!("https://{}:{}", self.document.host, self.document.port))
    }

    pub fn apply_public_endpoint(&self, info: &mut crate::connection_policy::PublicInfo) {
        info.endpoint = self.endpoint();
        info.discoverable = info.endpoint.is_some();
    }

    pub fn accepts_ingress(&self, value: Option<&str>) -> bool {
        self.endpoint().is_some() && value == Some(self.ingress.as_str())
    }

    pub fn announcement(
        &self,
        info: &crate::connection_policy::PublicInfo,
    ) -> Option<serde_json::Value> {
        let endpoint = self.endpoint()?;
        let mut packet = serde_json::to_value(info).ok()?;
        packet["certificate"] = self.document.identity.as_ref()?.certificate.clone().into();
        packet["endpoint"] = endpoint.into();
        Some(packet)
    }

    pub fn view(&self) -> View {
        let endpoint = self.endpoint();
        View {
            enabled: self.document.enabled,
            listening: endpoint.is_some(),
            host: self.document.host,
            port: self.document.port,
            endpoint,
            error: self.error.clone(),
        }
    }

    pub fn update(&mut self, input: Update) -> Result<Option<Server>> {
        let mut next = Document {
            enabled: input.enabled,
            host: input.host,
            port: input.port,
            identity: self.document.identity.clone(),
        };
        validate(&next)?;
        if next.host != self.document.host {
            next.identity = None;
        }
        if next.enabled && next.identity.is_none() {
            next.identity =
                Some(LanIdentityDocument::generate(next.host).context("无法生成局域网证书")?);
        }
        let unchanged = next.host == self.document.host
            && next.port == self.document.port
            && self.endpoint().is_some();
        let ingress = if unchanged {
            self.ingress.clone()
        } else {
            uuid::Uuid::new_v4().to_string()
        };
        let server = if next.enabled && !unchanged {
            Some(
                Server::start(&next, self.backend, ingress.clone())
                    .context("无法开启局域网监听，请检查 IP 是否属于本机、端口是否被占用")?,
            )
        } else {
            None
        };
        // Persist before replacing the live listener. Failure drops the new listener.
        crate::write_json_atomically(&self.path, &next)
            .context("无法保存局域网设置，请检查数据目录写入权限")?;
        // Return retired workers to the caller: never join them while holding registry.
        let retired = if !next.enabled {
            self.server.take()
        } else if let Some(server) = server {
            self.server.replace(server)
        } else {
            None
        };
        if let Some(server) = &retired {
            server.stop.store(true, Ordering::Release);
        }
        self.ingress = ingress;
        self.document = next;
        self.error = None;
        Ok(retired)
    }

    pub fn stop(&mut self) -> Option<Server> {
        let server = self.server.take();
        if let Some(server) = &server {
            server.stop.store(true, Ordering::Release);
        }
        server
    }
}

fn validate(document: &Document) -> Result<()> {
    ensure!(
        document.host.is_private() || document.host.is_loopback(),
        "请选择本机局域网 IPv4 地址"
    );
    ensure!(
        !document.enabled || document.host.is_private() || cfg!(test),
        "请填写本机局域网 IP，而不是 127.0.0.1"
    );
    ensure!(document.port >= 1024, "局域网端口须为 1024–65535");
    Ok(())
}
fn suggested_host() -> Ipv4Addr {
    // UDP connect selects a route without sending a packet or contacting this address.
    UdpSocket::bind("0.0.0.0:0")
        .ok()
        .and_then(|socket| {
            socket.connect("192.0.2.1:9").ok()?;
            match socket.local_addr().ok()?.ip() {
                std::net::IpAddr::V4(ip) if ip.is_private() => Some(ip),
                _ => None,
            }
        })
        .unwrap_or(Ipv4Addr::LOCALHOST)
}
