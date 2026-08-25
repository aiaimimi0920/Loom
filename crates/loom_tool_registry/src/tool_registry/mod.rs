//! Internal implementation of the public tool registry facade.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::future::Future;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
#[cfg(windows)]
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
#[cfg(windows)]
use loom_process::ProcessSpec;
use loom_protocol::{
    is_safe_publisher_id, is_safe_surface_identifier, PublisherIdentity, SurfaceActionRisk,
    SurfacePackageManifest, SurfaceRuntimeKind, SURFACE_API_VERSION, SURFACE_PROTOCOL_VERSION,
};
use reqwest::{multipart, Method};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::{art_settings, framework, framework_process, network_policy};

mod cloud_request;
mod cloud_templates;
mod cloud_transport;
mod error;
mod execution;
mod image_candidates;
mod image_download;
mod image_normalize;
mod image_walk;
mod mcp_session;
mod model;
mod registry;
mod response;
mod validation;

pub use error::{ToolRegistryError, ToolRegistryResult};
pub use execution::{
    execute_tool, execute_tool_with_timeout, execute_tool_with_timeout_and_cancellation,
    prepare_tool_arguments,
};
pub use model::{
    ToolDefinition, ToolExecution, WorkflowExecutionBindings, WorkflowInputBinding,
    WorkflowOutputBinding,
};
pub(crate) use registry::replace_registry_file;
pub use registry::ToolRegistry;

use cloud_request::*;
use cloud_templates::*;
use cloud_transport::*;
use image_candidates::*;
use image_download::*;
use image_normalize::*;
use image_walk::*;
use mcp_session::*;
use response::*;
use validation::*;

const TOOLS_FILE: &str = "tools.json";
/// Deadline for a cloud API call when neither the caller nor the package asks for anything else.
const CLOUD_API_TIMEOUT: Duration = Duration::from_secs(30);
/// Host ceiling for a cloud API call. Image generation and background removal routinely run past
/// a minute, so callers may request a longer deadline without silently falling back to 30 seconds.
const CLOUD_API_MAX_TIMEOUT: Duration = Duration::from_secs(600);
const MCP_IMAGE_FETCH_USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/138.0.0.0 Safari/537.36";
const MCP_IMAGE_FETCH_ACCEPT: &str =
    "image/avif,image/webp,image/apng,image/svg+xml,image/*,*/*;q=0.8";
const MCP_IMAGE_FETCH_ACCEPT_LANGUAGE: &str = "en-US,en;q=0.9";
const MAX_MCP_IMAGE_BYTES: usize = 32 * 1024 * 1024;
const MAX_CLOUD_RESPONSE_BYTES: usize = 64 * 1024 * 1024;
/// Maximum borrowed diagnostic text retained in an error payload.
const MAX_BORROWED_ERROR_TEXT_BYTES: usize = 2 * 1024;

/// Bound process and remote-response diagnostics before they enter logs or surface payloads.
pub(crate) fn bounded_error_text(text: &str) -> String {
    let text = text.trim();
    if text.len() <= MAX_BORROWED_ERROR_TEXT_BYTES {
        return text.to_owned();
    }
    let mut end = MAX_BORROWED_ERROR_TEXT_BYTES;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    let omitted = text.len() - end;
    format!("{}… [{omitted} more bytes omitted]", &text[..end])
}

/// Extensions accepted as passive image candidates. SVG is intentionally excluded because it can
/// contain script and external references.
const IMAGE_URL_EXTENSIONS: [&str; 7] = [".png", ".jpg", ".jpeg", ".webp", ".gif", ".bmp", ".avif"];
/// Raster image MIME types accepted by the canvas normalization boundary.
const SUPPORTED_IMAGE_MIME_TYPES: [&str; 6] = [
    "image/png",
    "image/jpeg",
    "image/webp",
    "image/gif",
    "image/bmp",
    "image/avif",
];
static REGISTRY_FILE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[cfg(test)]
mod tests;
