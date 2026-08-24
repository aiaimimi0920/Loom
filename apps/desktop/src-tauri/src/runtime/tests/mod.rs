//! Desktop host contract tests grouped by lifecycle and domain boundary.

use super::*;
use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub(super) static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn owned_daemon_sleep_fixture() {
    if std::env::var("LOOM_DESKTOP_OWNED_DAEMON_FIXTURE")
        .ok()
        .as_deref()
        == Some("1")
    {
        std::thread::sleep(Duration::from_secs(30));
    }
}

mod art_bootstrap;
mod commands_cache;
mod fixtures;
mod framework_bootstrap;
mod lifecycle;
mod package_checks;
mod transport_paths;

use fixtures::*;
