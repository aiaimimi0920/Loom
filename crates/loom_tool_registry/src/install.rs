//! Art package installer (phase 1 of the art ecosystem).
//!
//! An art package is a zip whose `manifest.json` is a `ToolDefinition` (with an
//! optional `metadata.dependencies` block). Installing it:
//!   1. reads the manifest,
//!   2. checks the art's framework is installed + ready,
//!   3. extracts every zip entry into the publisher-scoped immutable Art tree,
//!   4. rewrites bundled binary/script paths to point inside that art dir,
//!   5. registers the `ToolDefinition` in the tool registry.
//! Dependent arts (workflow `uses` / `dependencies.arts`) are returned for the
//! caller to install recursively (wired with the store in phase 2).

use std::ffi::OsStr;
use std::io::{Cursor, Read};
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use loom_plugin_security::{
    canonical_package_digest, sign_package, verify_package_signature, SigningKeyDocument,
    TrustStore,
};
use loom_protocol::{
    ArtRuntimeManifest, PackageSignature, PackageTrustStatus, PluginLockfile, PublisherIdentity,
    ResolvedDependency,
};

use crate::framework::{read_dependencies, ArtBinary, ArtMcpServerDependency, FrameworkRegistry};
use crate::{ToolDefinition, ToolRegistry};

mod activation;
mod binaries;
mod core;
mod fs_safety;
mod integrity;
mod lockfile;
mod manifest;
mod mcp;
mod package;
mod recursive;
mod resolve;
mod types;
mod uninstall;

use binaries::*;
use fs_safety::*;
use integrity::*;
use lockfile::*;
use manifest::*;
use mcp::*;
use types::*;

pub use activation::{activate_art_version, list_installed_art_versions, rollback_art_package};
pub use core::{install_art_from_zip, install_authored_art_from_zip, install_bundled_art_from_zip};
pub use fs_safety::{recover_art_lifecycle, recover_art_uninstall_tombstones};
pub use integrity::verify_art_package_integrity;
pub use manifest::read_manifest_from_zip;
pub use package::{build_authored_art_package_zip, package_art_to_zip, package_signed_art_to_zip};
pub use recursive::install_art_recursive;
pub use resolve::{resolve_active_art_package, resolve_installed_art_package};
pub use types::{ArtInstallError, ArtInstallReport, ArtInstalledVersion};
pub use uninstall::uninstall_art_package;

#[cfg(test)]
mod tests;
