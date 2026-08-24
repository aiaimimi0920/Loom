//! Generic stdin/stdout bridge for externally packaged Art frameworks.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::ChildStdin;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use loom_process::{ManagedChild, ProcessError, ProcessSpec};
use serde::Deserialize;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

use crate::framework::{
    enforce_framework_permission_policy, read_bounded_framework_metadata,
    resolve_framework_package_dir, FrameworkPackageManifest, FRAMEWORK_PROTOCOL_VERSION,
};
use crate::{ToolDefinition, ToolRegistryError, ToolRegistryResult};

pub use loom_protocol::{
    ExecutionDiagnostics, FrameworkExecuteError, FrameworkExecuteRequest, FrameworkExecuteResponse,
    FrameworkExecutionContext, FrameworkMcpServer,
};

pub const DEFAULT_FRAMEWORK_PROCESS_TIMEOUT: Duration = Duration::from_secs(120);
/// File-size ceiling for an image an Art hands back by path.
///
/// This is checked against the file on disk, and it only became a memory bound when the reader stopped
/// decoding the file: peak is now the file plus its base64 at roughly 1.37×, both linear in the number
/// checked here. Before that a decode sat in the middle whose cost was width × height × 4 and had no
/// relation to the compressed size, so a file well under this limit could still ask for gigabytes.
///
/// The number itself is unchanged, and it is still generous for an image — 256 MiB of file is close to
/// 600 MiB of peak once the base64 and the JSON copy of it are counted. Lowering it would reject outputs
/// that work today, so it is left as a size limit to revisit rather than quietly tightened here.
const MAX_FRAMEWORK_IMAGE_OUTPUT_BYTES: u64 = 256 * 1024 * 1024;
const MAX_PERSISTENT_MCP_HOSTS: usize = 4;
const PERSISTENT_MCP_HOST_IDLE_LIFETIME: Duration = Duration::from_secs(60);
const PERSISTENT_HOST_ERROR_BYTES: usize = 8 * 1024;

mod candidates;
mod execute;
mod host;
mod image;
mod package;
mod redaction;

use candidates::*;
#[cfg(test)]
use execute::execute_framework_art_in_root_with_timeout;
use host::*;
use image::*;
use package::*;
use redaction::*;

pub use execute::{
    execute_framework_art, execute_framework_art_with_timeout,
    execute_framework_art_with_timeout_and_cancellation,
};

#[cfg(test)]
mod tests;
