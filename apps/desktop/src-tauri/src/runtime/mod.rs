//! Desktop bootstrap, daemon ownership, command, cache, transport, and package internals.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs;
use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{Manager, WindowEvent};

mod app;
mod binary_http;
mod commands;
mod config;
mod daemon;
mod diagnostics;
mod file_io;
mod framework_packages;
mod hook_cache;
mod http_response;
mod loom_cache;
mod package_bootstrap;
mod transport;
mod types;

pub use app::*;
pub use daemon::*;
pub use hook_cache::*;
pub use loom_cache::*;
pub use types::*;

use binary_http::*;
use config::*;
#[cfg(test)]
use diagnostics::*;
use file_io::*;
use framework_packages::*;
use http_response::*;
use package_bootstrap::*;
use transport::*;

#[cfg(test)]
mod tests;
