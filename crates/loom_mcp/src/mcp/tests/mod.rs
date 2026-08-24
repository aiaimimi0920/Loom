//! Focused tests for MCP configuration and transports.

use super::*;
use std::io::{BufRead, Write};
use std::net::{TcpListener, TcpStream};

pub(super) static PROCESS_CONFIG_LOCK: Mutex<()> = Mutex::new(());

pub(super) struct ProcessConfigTestGuard {
    _lock: std::sync::MutexGuard<'static, ()>,
    request_timeout_seconds: u64,
    memory_limit_bytes: u64,
    allow_local_servers: bool,
}

impl ProcessConfigTestGuard {
    pub(super) fn capture() -> Self {
        let lock = PROCESS_CONFIG_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        Self {
            _lock: lock,
            request_timeout_seconds: MCP_REQUEST_TIMEOUT_SECONDS.load(Ordering::Relaxed),
            memory_limit_bytes: MCP_MEMORY_LIMIT_BYTES.load(Ordering::Relaxed),
            allow_local_servers: MCP_ALLOW_LOCAL_SERVERS.load(Ordering::Relaxed),
        }
    }
}

impl Drop for ProcessConfigTestGuard {
    fn drop(&mut self) {
        MCP_REQUEST_TIMEOUT_SECONDS.store(self.request_timeout_seconds, Ordering::Relaxed);
        MCP_MEMORY_LIMIT_BYTES.store(self.memory_limit_bytes, Ordering::Relaxed);
        MCP_ALLOW_LOCAL_SERVERS.store(self.allow_local_servers, Ordering::Relaxed);
    }
}

mod config_validation;
mod fixture_server;
mod hardening;
mod http;
mod http_fixtures;
mod protocol;
mod stdio;
mod windows_fixtures;

use fixture_server::*;
use http_fixtures::*;
use windows_fixtures::*;
