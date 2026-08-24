//! MCP server package intake, trust, persistence, and pre-spawn integrity facade.

use std::collections::BTreeMap;
use std::fs;
use std::io::{Cursor, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use loom_plugin_security::{verify_package_signature, TrustStore};
use loom_protocol::{PackageSignature, PackageTrustStatus, PublisherIdentity};
use loom_security::archive::{extract_zip_securely, SecureZipError};
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use zip::ZipArchive;

use crate::{
    validate_mcp_environment_name, validate_mcp_header_name, validate_mcp_tool_identifier,
    McpCredentialRequirement, McpServerConfig, McpServerPackageState, McpTransport,
    MAX_MCP_ARGUMENTS, MAX_MCP_ARGUMENT_BYTES, MAX_MCP_CREDENTIALS, MAX_MCP_CREDENTIAL_LABEL_BYTES,
    MAX_MCP_SERVER_DESCRIPTION_BYTES, MAX_MCP_SERVER_NAME_BYTES, MAX_MCP_TOOLS,
};

mod archive;
mod config;
mod digest;
mod error;
mod install;
mod model;
mod paths;
mod state;
mod trust;
mod uninstall;
mod validation;
mod verify;

pub use error::*;
pub use install::*;
pub use model::*;
pub use uninstall::*;
pub use verify::*;

use archive::*;
use config::*;
use digest::*;
use paths::*;
use state::*;
use trust::*;
use validation::*;

#[cfg(test)]
mod tests;
