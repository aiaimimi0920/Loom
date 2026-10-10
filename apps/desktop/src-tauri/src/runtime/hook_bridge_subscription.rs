//! Owns one native authenticated subscription; the WebView only reads counters.

use super::{read_bounded_regular_file, LOOM_EXITING};
use serde::Serialize;
use std::net::{Shutdown, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::JoinHandle;
use std::time::Duration;

#[path = "hook_bridge_subscription/session.rs"]
mod session;
#[cfg(test)]
#[path = "hook_bridge_subscription/tests.rs"]
mod tests;

static OWNER: OnceLock<Mutex<Option<Subscription>>> = OnceLock::new();

#[derive(Default)]
struct Counters {
    connected: bool,
    epoch: u64,
    workflow: u64,
    capabilities: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SubscriptionState {
    connected: bool,
    epoch: String,
    workflow_revision: String,
    capabilities_revision: String,
}

impl Counters {
    fn snapshot(&self) -> SubscriptionState {
        SubscriptionState {
            connected: self.connected,
            epoch: self.epoch.to_string(),
            workflow_revision: self.workflow.to_string(),
            capabilities_revision: self.capabilities.to_string(),
        }
    }
}

#[derive(Default)]
struct Shared {
    cancelled: AtomicBool,
    interrupt: Mutex<Option<TcpStream>>,
    counters: Mutex<Counters>,
}

impl Shared {
    fn cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    fn disconnect(&self) {
        self.counters
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .connected = false;
        if let Some(socket) = self
            .interrupt
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
        {
            let _ = socket.shutdown(Shutdown::Both);
        }
    }
}

struct Subscription {
    shared: Arc<Shared>,
    worker: Option<JoinHandle<()>>,
}

impl Subscription {
    fn start(manifest: PathBuf) -> std::io::Result<Self> {
        let shared = Arc::new(Shared::default());
        let task = Arc::clone(&shared);
        let worker = std::thread::Builder::new()
            .name("loom-bridge-subscription".into())
            .spawn(move || {
                while !task.cancelled() {
                    // Discovery/TLS errors may contain sensitive data; never forward or log them.
                    let _ = session::run(&manifest, &task);
                    task.disconnect();
                    if !task.cancelled() {
                        std::thread::park_timeout(Duration::from_secs(1));
                    }
                }
            })?;
        Ok(Self {
            shared,
            worker: Some(worker),
        })
    }

    fn stop(&mut self) {
        self.shared.cancelled.store(true, Ordering::Release);
        self.shared.disconnect();
        if let Some(worker) = self.worker.take() {
            worker.thread().unpark();
            let _ = worker.join();
        }
    }
}

impl Drop for Subscription {
    fn drop(&mut self) {
        self.stop();
    }
}

pub(super) fn start() -> Result<(), String> {
    let mut owner = OWNER
        .get_or_init(|| Mutex::new(None))
        .lock()
        .map_err(|_| "订阅状态不可用")?;
    if LOOM_EXITING.load(Ordering::Acquire) {
        return Err("Loom 正在退出".into());
    }
    if owner.is_none() {
        // No private discovery location means no integration, not a fatal UI error.
        // In particular, non-Windows preview builds must not try anonymous fallback.
        let Some(appdata) = std::env::var_os("APPDATA").filter(|value| !value.is_empty()) else {
            return Ok(());
        };
        let manifest = PathBuf::from(appdata).join("Neuro/capabilities/loom.json");
        *owner = Some(Subscription::start(manifest).map_err(|_| "无法启动本地订阅")?);
    }
    Ok(())
}

pub(super) fn stop() {
    let owner = OWNER
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .take();
    drop(owner);
}

fn require_main(label: &str) -> Result<(), String> {
    if label != "main" {
        return Err("仅主窗口可读取本地订阅".into());
    }
    Ok(())
}

#[tauri::command]
pub(super) fn read_hook_bridge_subscription_state(
    window: tauri::WebviewWindow,
) -> Result<SubscriptionState, String> {
    require_main(window.label())?;
    let owner = OWNER
        .get_or_init(|| Mutex::new(None))
        .lock()
        .map_err(|_| "订阅状态不可用")?;
    let Some(owner) = owner.as_ref() else {
        return Ok(Counters::default().snapshot());
    };
    let state = owner
        .shared
        .counters
        .lock()
        .map_err(|_| "订阅状态不可用")?
        .snapshot();
    Ok(state)
}
