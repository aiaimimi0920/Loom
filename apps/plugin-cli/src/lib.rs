//! Loom plugin authoring CLI facade.
//!
//! Private implementation fragments stay in this lexical module so extraction does not widen the
//! public API beyond `run` or create compatibility surfaces for internal helpers.

use std::collections::BTreeSet;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, bail, Context, Result};
use loom_plugin_security::{
    generate_signing_key, restrict_private_path_permissions, sign_package,
    verify_package_signature, SigningKeyDocument, TrustStore,
};
use loom_protocol::{
    is_safe_package_id, is_safe_publisher_id, is_safe_surface_identifier, schemas,
    validate_framework_manifest_contract, validate_surface_node_tree, validate_surface_protocol,
    ArtRuntimeManifest, FrameworkExecuteRequest, FrameworkExecuteResponse,
    FrameworkExecutionContext, FrameworkPackageManifest, PackageSignature, PackageTrustStatus,
    PublisherIdentity, PublisherTrustRecord, SurfaceNode, SurfacePackageManifest,
    SurfaceRuntimeKind, ART_RUNTIME_PROTOCOL_VERSION, DECLARATIVE_SURFACE_NODE_TYPES,
    FRAMEWORK_PROTOCOL_VERSION, SURFACE_API_VERSION,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use zip::write::SimpleFileOptions;

const MAX_PACKAGE_FILES: usize = 4096;
const MAX_PACKAGE_BYTES: u64 = 512 * 1024 * 1024;
const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;
const MAX_SIGNING_KEY_BYTES: u64 = 64 * 1024;
const MAX_CONFORMANCE_OUTPUT_BYTES: usize = 8 * 1024 * 1024;
const CONFORMANCE_TIMEOUT: Duration = Duration::from_secs(30);
static CONFORMANCE_SEQUENCE: AtomicU64 = AtomicU64::new(0);
static ATOMIC_WRITE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

include!("cli.rs");
include!("filesystem.rs");
include!("validation.rs");
include!("signing.rs");
include!("package.rs");
include!("scaffold.rs");
include!("conformance.rs");

#[cfg(test)]
include!("tests.rs");
#[cfg(test)]
include!("security_tests.rs");
