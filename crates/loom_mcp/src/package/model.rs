//! Public MCP package manifest and persisted-state models.

use super::*;

pub const MCP_SERVER_PACKAGE_MANIFEST: &str = "mcp.server.json";
/// Maximum accepted compressed size for one MCP server package archive.
pub const MAX_MCP_SERVER_PACKAGE_BYTES: usize = 64 * 1024 * 1024;
/// Entry-count ceiling for a server package archive, matched to the shared extractor's own limit.
///
/// It was 128, which no real MCP server fits: an npm or Python server vendors its dependencies, and a
/// dependency tree is thousands of files before it is anything. The cap that mattered was never this
/// one anyway — `extract_zip_securely` enforces 4096 entries, per-entry and total size limits, and a
/// compression-ratio check — so 128 only turned normal packages away, and did it at install time with
/// a message about entry counts rather than anything a publisher could act on.
pub(super) const MAX_PACKAGE_FILES: usize = 4096;
pub(super) const MAX_EXTRACTED_BYTES: u64 = 128 * 1024 * 1024;
pub(super) const MAX_PACKAGE_MANIFEST_BYTES: usize = 1024 * 1024;
pub(super) const MAX_ACTIVE_STATE_BYTES: usize = 8 * 1024 * 1024;

/// How much of the archive digest names the version directory.
///
/// The name has to be unique per archive, because two archives landing on one directory means one of
/// them runs the other's files. Twelve hex characters — 48 bits — was not enough for that: about 2^24
/// hashes finds two packages sharing a version string and a prefix, which is minutes of work rather
/// than an attack. Thirty-two characters is 128 bits, so a collision is out of reach, and the rest of
/// the digest is not spent on the path: an MCP server that vendors its dependencies nests deeply
/// inside this directory, and every character here comes out of the `MAX_PATH` budget those files
/// need. The full digest is recorded in `active.json` and in the server config either way.
pub const PACKAGE_DIRECTORY_DIGEST_CHARS: usize = 32;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerPackageManifest {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub version: String,
    pub publisher: McpPackagePublisher,
    pub transport: McpTransport,
    pub entry: McpPackageEntry,
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(default)]
    pub credentials: Vec<McpPackageCredential>,
    /// Publisher signature over the package, in the shape Art packages already use.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package_security: Option<McpPackageSecurity>,
}

impl McpServerPackageManifest {
    #[must_use]
    pub fn qualified_id(&self) -> String {
        format!("{}/{}", self.publisher.id, self.id)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpPackagePublisher {
    pub id: String,
    pub name: String,
}

/// The signature block of an MCP server package manifest.
///
/// This is deliberately the same `PackageSignature` an Art's `metadata.packageSecurity` carries, so
/// one signing tool, one trust store, and one verifier serve both package kinds. Unlike the Art
/// block it holds no publisher identity: the manifest already names its publisher, and letting the
/// security block name a second one would only create two places to disagree.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpPackageSecurity {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<PackageSignature>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpPackageEntry {
    #[serde(default)]
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub url: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpPackageCredential {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub required: bool,
    pub target: McpPackageCredentialTarget,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpPackageCredentialTarget {
    pub kind: McpPackageCredentialTargetKind,
    pub name: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum McpPackageCredentialTargetKind {
    Env,
    Header,
}

/// The persisted `active.json` state for an installed MCP server package.
///
/// The installer used to write this file and nobody read it back, which made it a decoration
/// rather than a record: the digests it carried were never compared against anything. It is now
/// the authoritative copy of what was installed, and `verify_installed_entry` reads it before a
/// package-backed server is spawned.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpPackageActiveState {
    pub qualified_id: String,
    pub version: String,
    pub digest: String,
    pub package_dir: PathBuf,
    /// SHA-256 of every extracted file, keyed by its package-relative path with `/` separators.
    #[serde(default)]
    pub files: BTreeMap<String, String>,
    /// What the trust check concluded at install time, so a later reader does not have to re-verify
    /// a signature to say whether the package was signed and by whom.
    #[serde(default)]
    pub trust_status: PackageTrustStatus,
}
