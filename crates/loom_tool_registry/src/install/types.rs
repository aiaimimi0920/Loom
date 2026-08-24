use super::*;

pub(super) const MANIFEST_NAME: &str = "manifest.json";
pub(super) const ART_LIFECYCLE_FILE: &str = "lifecycle.json";
pub(super) const ART_UNINSTALL_TOMBSTONE_PREFIX: &str = ".loom-delete-art-";

#[derive(Debug, thiserror::Error)]
pub enum ArtInstallError {
    #[error("invalid art package: {0}")]
    InvalidPackage(String),
    #[error("art package missing {MANIFEST_NAME}")]
    MissingManifest,
    #[error("invalid art id `{0}`")]
    InvalidArtId(String),
    #[error("art `{art_id}` requires framework `{framework}` which is not {reason}")]
    FrameworkNotReady {
        art_id: String,
        framework: String,
        reason: String,
    },
    #[error("art binary `{name}` is not bundled and has no download url")]
    BinaryMissing { name: String },
    #[error("download of art binary `{name}` failed: {reason}")]
    BinaryDownloadFailed { name: String, reason: String },
    #[error("remote art binary `{name}` must declare a sha256 digest")]
    RemoteBinaryHashRequired { name: String },
    #[error("art binary `{name}` sha256 mismatch: expected {expected}, got {actual}")]
    BinaryHashMismatch {
        name: String,
        expected: String,
        actual: String,
    },
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("zip error: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("tool registry error: {0}")]
    Registry(String),
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtInstallReport {
    pub tool_id: String,
    pub framework: String,
    pub art_dir: PathBuf,
    pub installed_files: Vec<String>,
    /// Third-party binaries resolved (bundled or downloaded) into the art dir.
    pub binaries: Vec<String>,
    /// Dependent art ids to install next (from the manifest's dependencies).
    pub dependent_arts: Vec<String>,
    pub trust_status: PackageTrustStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtInstalledVersion {
    pub version: String,
    pub digest: String,
    pub active: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ArtPackageSecurityMetadata {
    #[serde(default)]
    pub(super) version: Option<String>,
    #[serde(default)]
    pub(super) publisher: Option<PublisherIdentity>,
    #[serde(default)]
    pub(super) signature: Option<PackageSignature>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ArtActivationState {
    pub(super) active: ArtVersionPointer,
    pub(super) previous: Option<ArtVersionPointer>,
    #[serde(default)]
    pub(super) local_authoring: bool,
    #[serde(default)]
    pub(super) bundled_catalog: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ArtInstallSource {
    ExternalPackage,
    LocalAuthoring,
    BundledCatalog,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ArtVersionPointer {
    pub(super) path: String,
    pub(super) version: String,
    pub(super) digest: String,
    pub(super) lockfile: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ArtLifecycleJournal {
    pub(super) old_activation: Option<ArtActivationState>,
    pub(super) next_activation: ArtActivationState,
    pub(super) target: String,
    /// Whether `target` was created by the operation this journal describes.
    ///
    /// Recovery may only delete a version directory the interrupted operation itself put on disk.
    /// An install that reuses an already-present directory, and an activation that points at an
    /// older version, both name a directory that predates the operation; deleting it would destroy
    /// the very version recovery is supposed to restore. Journals written by an older build lack
    /// the field, so the default is `false`: never delete.
    #[serde(default)]
    pub(super) created_target: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ArtMcpExecutionMetadata {
    pub(super) server_id: String,
    pub(super) package_id: String,
    pub(super) version: String,
}
