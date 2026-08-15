use std::collections::BTreeSet;
use std::fs::{self, File, OpenOptions};
use std::io::Write as _;
use std::path::{Component, Path, PathBuf};

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use loom_protocol::{
    PackageSignature, PackageSignatureDocument, PackageTrustStatus, PublisherIdentity,
    PublisherTrustRecord,
};
use rand_core::OsRng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const TRUST_STORE_SCHEMA_VERSION: u32 = 1;
const SIGNING_KEY_SCHEMA_VERSION: u32 = 1;
const MAX_SIGNED_PACKAGE_FILES: usize = 4096;
const MAX_SIGNED_PACKAGE_BYTES: u64 = 512 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum PluginSecurityError {
    #[error("unsafe package-relative path: {0}")]
    UnsafePath(String),
    #[error("symbolic links are not allowed in signed packages: {0}")]
    SymbolicLink(String),
    #[error("package contains a duplicate or case-colliding path: {0}")]
    DuplicatePath(String),
    #[error("package exceeds signing limit of {MAX_SIGNED_PACKAGE_FILES} files")]
    FileCount,
    #[error("package exceeds signing limit of {MAX_SIGNED_PACKAGE_BYTES} bytes")]
    PackageSize,
    #[error("unsupported signature algorithm: {0}")]
    UnsupportedAlgorithm(String),
    #[error("signature document does not match manifest metadata")]
    SignatureMetadataMismatch,
    #[error("package digest mismatch: expected {expected}, got {actual}")]
    DigestMismatch { expected: String, actual: String },
    #[error("invalid Ed25519 key or signature: {0}")]
    InvalidKey(String),
    #[error("signature verification failed")]
    VerificationFailed,
    #[error("plugin trust policy rejected package status {0:?}")]
    TrustPolicyRejected(PackageTrustStatus),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("base64 error: {0}")]
    Base64(#[from] base64::DecodeError),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SigningKeyDocument {
    #[serde(default = "default_signing_key_schema_version")]
    pub schema_version: u32,
    pub key_id: String,
    pub private_key: String,
    pub public_key: String,
}

const fn default_signing_key_schema_version() -> u32 {
    SIGNING_KEY_SCHEMA_VERSION
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrustStore {
    #[serde(default = "default_trust_store_schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub publishers: Vec<PublisherTrustRecord>,
    #[serde(default)]
    pub policy: TrustPolicy,
    #[serde(default)]
    pub trusted_publishers: Vec<String>,
}

impl Default for TrustStore {
    fn default() -> Self {
        Self {
            schema_version: TRUST_STORE_SCHEMA_VERSION,
            publishers: Vec::new(),
            policy: TrustPolicy::default(),
            trusted_publishers: Vec::new(),
        }
    }
}

const fn default_trust_store_schema_version() -> u32 {
    TRUST_STORE_SCHEMA_VERSION
}

impl TrustStore {
    pub fn load(path: &Path) -> Result<Self, PluginSecurityError> {
        // Repair the ACL before opening a legacy trust store. Older Loom
        // builds protected the parent with OWNER RIGHTS/SYSTEM only, which
        // leaves the current token unable to read or atomically replace it.
        match restrict_private_path_permissions(path, false) {
            Ok(()) => Ok(serde_json::from_slice(&fs::read(path)?)?),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(PluginSecurityError::Io(error)),
        }
    }

    pub fn write_atomic(&self, path: &Path) -> Result<(), PluginSecurityError> {
        if let Some(parent) = path.parent() {
            match restrict_private_path_permissions(parent, true) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    fs::create_dir_all(parent)?;
                    restrict_private_path_permissions(parent, true)?;
                }
                Err(error) => return Err(PluginSecurityError::Io(error)),
            }
        }
        let mut bytes = serde_json::to_vec_pretty(self)?;
        bytes.push(b'\n');
        let (temporary, mut file) = create_atomic_temporary(path)?;
        let result = (|| {
            file.write_all(&bytes)?;
            file.sync_all()?;
            drop(file);
            restrict_private_path_permissions(&temporary, false)?;
            replace_file_atomic(&temporary, path)?;
            restrict_private_path_permissions(path, false)?;
            sync_parent_directory(path)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }

    pub fn trust(&mut self, record: PublisherTrustRecord) {
        self.publishers.retain(|existing| {
            existing.publisher_id != record.publisher_id || existing.key_id != record.key_id
        });
        self.publishers.push(record);
        self.publishers.sort_by(|left, right| {
            (&left.publisher_id, &left.key_id).cmp(&(&right.publisher_id, &right.key_id))
        });
    }

    pub fn revoke(&mut self, publisher_id: &str, key_id: &str) -> bool {
        let Some(record) = self
            .publishers
            .iter_mut()
            .find(|record| record.publisher_id == publisher_id && record.key_id == key_id)
        else {
            return false;
        };
        record.revoked = true;
        true
    }

    pub fn set_policy(&mut self, policy: TrustPolicy) {
        self.policy = policy;
    }

    pub fn trust_publisher_id(&mut self, publisher_id: impl Into<String>) {
        let publisher_id = publisher_id.into();
        if !self
            .trusted_publishers
            .iter()
            .any(|existing| existing == &publisher_id)
        {
            self.trusted_publishers.push(publisher_id);
            self.trusted_publishers.sort();
        }
    }

    pub fn untrust_publisher_id(&mut self, publisher_id: &str) -> bool {
        let before_ids = self.trusted_publishers.len();
        self.trusted_publishers
            .retain(|existing| existing != publisher_id);
        let before_keys = self.publishers.len();
        self.publishers
            .retain(|record| record.publisher_id != publisher_id);
        before_ids != self.trusted_publishers.len() || before_keys != self.publishers.len()
    }

    pub fn effective_policy(&self) -> TrustPolicy {
        TrustPolicy::from_env_override().unwrap_or(self.policy)
    }
}

fn create_atomic_temporary(path: &Path) -> std::io::Result<(PathBuf, File)> {
    let parent = path.parent().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "trust store path has no parent",
        )
    })?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "trust store path has no UTF-8 file name",
            )
        })?;
    for attempt in 0..100u32 {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let temporary = parent.join(format!(
            ".{file_name}.tmp-{}-{nonce}-{attempt}",
            std::process::id()
        ));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        match options.open(&temporary) {
            Ok(file) => return Ok((temporary, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "could not allocate a unique trust-store temporary file",
    ))
}

#[cfg(not(windows))]
fn replace_file_atomic(source: &Path, destination: &Path) -> std::io::Result<()> {
    fs::rename(source, destination)
}

#[cfg(windows)]
fn replace_file_atomic(source: &Path, destination: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };

    fn extended_length_path(path: &Path) -> std::io::Result<Vec<u16>> {
        let absolute = match fs::canonicalize(path) {
            Ok(path) => path,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let parent = path.parent().ok_or_else(|| {
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        "atomic replacement path has no parent",
                    )
                })?;
                let file_name = path.file_name().ok_or_else(|| {
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        "atomic replacement path has no file name",
                    )
                })?;
                fs::canonicalize(parent)?.join(file_name)
            }
            Err(error) => return Err(error),
        };
        let wide = absolute.as_os_str().encode_wide().collect::<Vec<_>>();
        let mut extended =
            if wide.starts_with(&[b'\\' as u16, b'\\' as u16, b'?' as u16, b'\\' as u16])
                || wide.starts_with(&[b'\\' as u16, b'\\' as u16, b'.' as u16, b'\\' as u16])
            {
                wide
            } else if wide.starts_with(&[b'\\' as u16, b'\\' as u16]) {
                let mut path = r"\\?\UNC\".encode_utf16().collect::<Vec<_>>();
                path.extend_from_slice(&wide[2..]);
                path
            } else {
                let mut path = r"\\?\".encode_utf16().collect::<Vec<_>>();
                path.extend_from_slice(&wide);
                path
            };
        extended.push(0);
        Ok(extended)
    }

    let source = extended_length_path(source)?;
    let destination = extended_length_path(destination)?;
    let moved = unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if moved == 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(unix)]
fn sync_parent_directory(path: &Path) -> std::io::Result<()> {
    let parent = path.parent().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "path has no parent")
    })?;
    File::open(parent)?.sync_all()
}

#[cfg(not(unix))]
fn sync_parent_directory(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

#[cfg(unix)]
pub fn restrict_private_path_permissions(path: &Path, directory: bool) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = fs::metadata(path)?.permissions();
    permissions.set_mode(if directory { 0o700 } else { 0o600 });
    fs::set_permissions(path, permissions)
}

#[cfg(windows)]
pub fn restrict_private_path_permissions(path: &Path, directory: bool) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Authorization::{
        ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
    };
    use windows_sys::Win32::Security::{
        SetFileSecurityW, DACL_SECURITY_INFORMATION, PROTECTED_DACL_SECURITY_INFORMATION,
        PSECURITY_DESCRIPTOR,
    };

    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    let wide = absolute.as_os_str().encode_wide().collect::<Vec<_>>();
    let mut extended = if wide.starts_with(&[b'\\' as u16, b'\\' as u16, b'?' as u16, b'\\' as u16])
        || wide.starts_with(&[b'\\' as u16, b'\\' as u16, b'.' as u16, b'\\' as u16])
    {
        wide
    } else if wide.starts_with(&[b'\\' as u16, b'\\' as u16]) {
        let mut value = r"\\?\UNC\".encode_utf16().collect::<Vec<_>>();
        value.extend_from_slice(&wide[2..]);
        value
    } else {
        let mut value = r"\\?\".encode_utf16().collect::<Vec<_>>();
        value.extend_from_slice(&wide);
        value
    };
    extended.push(0);
    let inheritance = if directory { "OICI" } else { "" };
    let current_user_sid = current_user_sid_string()?;
    let sddl = format!(
        "D:P(A;{inheritance};FA;;;{current_user_sid})(A;{inheritance};FA;;;OW)(A;{inheritance};FA;;;SY)\0"
    )
        .encode_utf16()
        .collect::<Vec<_>>();
    let mut descriptor: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
    let converted = unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            SDDL_REVISION_1,
            &mut descriptor,
            std::ptr::null_mut(),
        )
    };
    if converted == 0 {
        return Err(std::io::Error::last_os_error());
    }
    let updated = unsafe {
        SetFileSecurityW(
            extended.as_ptr(),
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            descriptor,
        )
    };
    unsafe {
        LocalFree(descriptor.cast());
    }
    if updated == 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(not(any(unix, windows)))]
pub fn restrict_private_path_permissions(_path: &Path, _directory: bool) -> std::io::Result<()> {
    Ok(())
}

/// Repair an existing private control-plane tree after older builds applied a
/// Windows DACL that omitted the current user's SID. Entries whose owner does
/// not allow their DACL to be repaired are left untouched and reported to the
/// caller; they remain outside the active traversal until their owning token
/// can be migrated. The traversal is deliberately symlink-free so permission
/// repair cannot escape the tree.
pub fn repair_private_tree_permissions(root: &Path) -> std::io::Result<Vec<PathBuf>> {
    restrict_private_path_permissions(root, true).map_err(|error| {
        std::io::Error::new(
            error.kind(),
            format!(
                "repair private directory permissions {}: {error}",
                root.display()
            ),
        )
    })?;
    let mut pending = vec![root.to_path_buf()];
    let mut quarantined = Vec::new();
    while let Some(directory) = pending.pop() {
        let entries = fs::read_dir(&directory)?.collect::<Result<Vec<_>, _>>()?;
        for entry in entries {
            let path = entry.path();
            let file_type = entry.file_type()?;
            if file_type.is_symlink() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!(
                        "private control-plane tree contains a symbolic link: {}",
                        path.display()
                    ),
                ));
            }
            if let Err(error) = restrict_private_path_permissions(&path, file_type.is_dir()) {
                if error.kind() != std::io::ErrorKind::PermissionDenied {
                    return Err(std::io::Error::new(
                        error.kind(),
                        format!(
                            "repair private path permissions {}: {error}",
                            path.display()
                        ),
                    ));
                }
                let relative = path.strip_prefix(root).map_err(|_| {
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "private ACL repair escaped its root",
                    )
                })?;
                quarantined.push(relative.to_path_buf());
                continue;
            }
            if file_type.is_dir() {
                pending.push(path);
            }
        }
    }
    Ok(quarantined)
}

#[cfg(windows)]
fn current_user_sid_string() -> std::io::Result<String> {
    use std::mem::size_of;
    use windows_sys::Win32::Foundation::{CloseHandle, LocalFree, HANDLE};
    use windows_sys::Win32::Security::Authorization::ConvertSidToStringSidW;
    use windows_sys::Win32::Security::{GetTokenInformation, TokenUser, TOKEN_QUERY, TOKEN_USER};
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    let mut token: HANDLE = std::ptr::null_mut();
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
        return Err(std::io::Error::last_os_error());
    }
    let result = (|| -> std::io::Result<String> {
        let mut required = 0u32;
        unsafe {
            let _ = GetTokenInformation(token, TokenUser, std::ptr::null_mut(), 0, &mut required);
        }
        if required < size_of::<TOKEN_USER>() as u32 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Windows token user information is unavailable",
            ));
        }
        let mut buffer = vec![0u8; required as usize];
        if unsafe {
            GetTokenInformation(
                token,
                TokenUser,
                buffer.as_mut_ptr().cast(),
                required,
                &mut required,
            )
        } == 0
        {
            return Err(std::io::Error::last_os_error());
        }
        let token_user = unsafe { &*(buffer.as_ptr().cast::<TOKEN_USER>()) };
        let mut sid_string = std::ptr::null_mut();
        if unsafe { ConvertSidToStringSidW(token_user.User.Sid, &mut sid_string) } == 0 {
            return Err(std::io::Error::last_os_error());
        }
        let mut length = 0usize;
        unsafe {
            while *sid_string.add(length) != 0 {
                length += 1;
            }
        }
        let sid =
            String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(sid_string, length) });
        unsafe {
            LocalFree(sid_string.cast());
        }
        Ok(sid)
    })();
    unsafe {
        CloseHandle(token);
    }
    result
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrustPolicy {
    #[default]
    AllowUnsigned,
    RequireSigned,
    RequireTrusted,
}

impl TrustPolicy {
    pub fn from_env_override() -> Option<Self> {
        match std::env::var("LOOM_PLUGIN_TRUST_POLICY")
            .ok()?
            .trim()
            .to_ascii_lowercase()
            .as_str()
        {
            "allow-unsigned" | "allow_unsigned" => Some(Self::AllowUnsigned),
            "require-signed" | "require_signed" => Some(Self::RequireSigned),
            "require-trusted" | "require_trusted" => Some(Self::RequireTrusted),
            _ => None,
        }
    }

    pub fn from_env() -> Self {
        Self::from_env_override().unwrap_or_default()
    }

    pub fn enforce(self, status: PackageTrustStatus) -> Result<(), PluginSecurityError> {
        let accepted = match self {
            Self::AllowUnsigned => matches!(
                status,
                PackageTrustStatus::Unsigned
                    | PackageTrustStatus::Verified
                    | PackageTrustStatus::Trusted
            ),
            Self::RequireSigned => matches!(
                status,
                PackageTrustStatus::Verified | PackageTrustStatus::Trusted
            ),
            Self::RequireTrusted => status == PackageTrustStatus::Trusted,
        };
        if accepted {
            Ok(())
        } else {
            Err(PluginSecurityError::TrustPolicyRejected(status))
        }
    }
}

pub fn generate_signing_key(key_id: impl Into<String>) -> SigningKeyDocument {
    let signing_key = SigningKey::generate(&mut OsRng);
    SigningKeyDocument {
        schema_version: SIGNING_KEY_SCHEMA_VERSION,
        key_id: key_id.into(),
        private_key: BASE64.encode(signing_key.to_bytes()),
        public_key: BASE64.encode(signing_key.verifying_key().to_bytes()),
    }
}

pub fn write_signing_key(
    path: &Path,
    document: &SigningKeyDocument,
) -> Result<(), PluginSecurityError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut bytes = serde_json::to_vec_pretty(document)?;
    bytes.push(b'\n');
    fs::write(path, bytes)?;
    Ok(())
}

pub fn read_signing_key(path: &Path) -> Result<SigningKeyDocument, PluginSecurityError> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

pub fn sign_package(
    package_dir: &Path,
    signature_path: &str,
    key: &SigningKeyDocument,
) -> Result<PackageSignatureDocument, PluginSecurityError> {
    validate_relative_path(signature_path)?;
    let signing_key = decode_signing_key(&key.private_key)?;
    let public_key = BASE64.encode(signing_key.verifying_key().to_bytes());
    if public_key != key.public_key {
        return Err(PluginSecurityError::InvalidKey(
            "private/public key mismatch".to_owned(),
        ));
    }
    let digest = canonical_package_digest(package_dir, Some(signature_path))?;
    let signature = signing_key.sign(digest.as_bytes());
    let document = PackageSignatureDocument {
        schema_version: 1,
        algorithm: "ed25519".to_owned(),
        key_id: key.key_id.clone(),
        digest_algorithm: "sha256".to_owned(),
        digest,
        signature: BASE64.encode(signature.to_bytes()),
        public_key,
    };
    let output = package_dir.join(signature_path);
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut bytes = serde_json::to_vec_pretty(&document)?;
    bytes.push(b'\n');
    fs::write(output, bytes)?;
    Ok(document)
}

pub fn sign_message(
    key: &SigningKeyDocument,
    message: &[u8],
) -> Result<String, PluginSecurityError> {
    let signing_key = decode_signing_key(&key.private_key)?;
    let public_key = BASE64.encode(signing_key.verifying_key().to_bytes());
    if public_key != key.public_key {
        return Err(PluginSecurityError::InvalidKey(
            "private/public key mismatch".to_owned(),
        ));
    }
    Ok(BASE64.encode(signing_key.sign(message).to_bytes()))
}

pub fn verify_package_signature(
    package_dir: &Path,
    publisher: Option<&PublisherIdentity>,
    signature: Option<&PackageSignature>,
    trust_store: &TrustStore,
) -> Result<PackageTrustStatus, PluginSecurityError> {
    let Some(signature) = signature else {
        return Ok(PackageTrustStatus::Unsigned);
    };
    if signature.algorithm != "ed25519" {
        return Err(PluginSecurityError::UnsupportedAlgorithm(
            signature.algorithm.clone(),
        ));
    }
    validate_relative_path(&signature.file)?;
    let document: PackageSignatureDocument =
        serde_json::from_slice(&fs::read(package_dir.join(&signature.file))?)?;
    if document.algorithm != signature.algorithm
        || document.key_id != signature.key_id
        || document.digest_algorithm != "sha256"
    {
        return Err(PluginSecurityError::SignatureMetadataMismatch);
    }
    let actual_digest = canonical_package_digest(package_dir, Some(&signature.file))?;
    if actual_digest != document.digest {
        return Err(PluginSecurityError::DigestMismatch {
            expected: document.digest,
            actual: actual_digest,
        });
    }
    let verifying_key = decode_verifying_key(&document.public_key)?;
    let signature_bytes = BASE64.decode(document.signature.as_bytes())?;
    let signature = Signature::from_slice(&signature_bytes)
        .map_err(|error| PluginSecurityError::InvalidKey(error.to_string()))?;
    verifying_key
        .verify(actual_digest.as_bytes(), &signature)
        .map_err(|_| PluginSecurityError::VerificationFailed)?;

    let Some(publisher) = publisher else {
        return Ok(PackageTrustStatus::Verified);
    };
    let Some(record) = trust_store
        .publishers
        .iter()
        .find(|record| record.publisher_id == publisher.id && record.key_id == document.key_id)
    else {
        return Ok(PackageTrustStatus::Verified);
    };
    if record.revoked {
        return Ok(PackageTrustStatus::Revoked);
    }
    if record.public_key != document.public_key {
        return Err(PluginSecurityError::VerificationFailed);
    }
    Ok(PackageTrustStatus::Trusted)
}

pub fn canonical_package_digest(
    package_dir: &Path,
    excluded_relative_path: Option<&str>,
) -> Result<String, PluginSecurityError> {
    let excluded = excluded_relative_path.map(|path| path.replace('\\', "/").to_ascii_lowercase());
    let files = collect_files(package_dir, excluded.as_deref())?;
    let mut hasher = Sha256::new();
    for (relative, path) in files {
        let bytes = fs::read(path)?;
        hasher.update(relative.as_bytes());
        hasher.update([0]);
        hasher.update((bytes.len() as u64).to_le_bytes());
        hasher.update([0]);
        hasher.update(bytes);
    }
    Ok(hex_encode(&hasher.finalize()))
}

fn collect_files(
    root: &Path,
    excluded: Option<&str>,
) -> Result<Vec<(String, PathBuf)>, PluginSecurityError> {
    let mut pending = vec![root.to_path_buf()];
    let mut files = Vec::new();
    let mut seen = BTreeSet::new();
    let mut total = 0u64;
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(&directory)? {
            let entry = entry?;
            let file_type = entry.file_type()?;
            if file_type.is_symlink() {
                return Err(PluginSecurityError::SymbolicLink(
                    entry.path().display().to_string(),
                ));
            }
            if file_type.is_dir() {
                pending.push(entry.path());
                continue;
            }
            if !file_type.is_file() {
                continue;
            }
            let relative = entry
                .path()
                .strip_prefix(root)
                .map_err(|_| PluginSecurityError::UnsafePath(entry.path().display().to_string()))?
                .to_string_lossy()
                .replace('\\', "/");
            validate_relative_path(&relative)?;
            let folded = relative.to_ascii_lowercase();
            if excluded == Some(folded.as_str()) {
                continue;
            }
            if !seen.insert(folded) {
                return Err(PluginSecurityError::DuplicatePath(relative));
            }
            total = total.saturating_add(entry.metadata()?.len());
            if total > MAX_SIGNED_PACKAGE_BYTES {
                return Err(PluginSecurityError::PackageSize);
            }
            files.push((relative, entry.path()));
            if files.len() > MAX_SIGNED_PACKAGE_FILES {
                return Err(PluginSecurityError::FileCount);
            }
        }
    }
    files.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(files)
}

fn validate_relative_path(value: &str) -> Result<(), PluginSecurityError> {
    let path = Path::new(value);
    if value.trim().is_empty()
        || path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(PluginSecurityError::UnsafePath(value.to_owned()));
    }
    Ok(())
}

fn decode_signing_key(value: &str) -> Result<SigningKey, PluginSecurityError> {
    let bytes = BASE64.decode(value.as_bytes())?;
    let bytes: [u8; 32] = bytes
        .try_into()
        .map_err(|_| PluginSecurityError::InvalidKey("private key must be 32 bytes".to_owned()))?;
    Ok(SigningKey::from_bytes(&bytes))
}

fn decode_verifying_key(value: &str) -> Result<VerifyingKey, PluginSecurityError> {
    let bytes = BASE64.decode(value.as_bytes())?;
    let bytes: [u8; 32] = bytes
        .try_into()
        .map_err(|_| PluginSecurityError::InvalidKey("public key must be 32 bytes".to_owned()))?;
    VerifyingKey::from_bytes(&bytes)
        .map_err(|error| PluginSecurityError::InvalidKey(error.to_string()))
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "loom-plugin-security-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("directory");
        path
    }

    #[test]
    fn package_signature_roundtrip_and_trust_status() {
        let package = temp_dir("roundtrip");
        fs::write(package.join("manifest.json"), b"{}\n").unwrap();
        let key = generate_signing_key("test-key");
        sign_package(&package, "signature.json", &key).expect("sign");
        let publisher = PublisherIdentity {
            id: "example.vendor".to_owned(),
            key_id: Some(key.key_id.clone()),
            ..PublisherIdentity::default()
        };
        let signature = PackageSignature {
            algorithm: "ed25519".to_owned(),
            key_id: key.key_id.clone(),
            file: "signature.json".to_owned(),
        };
        let mut trust = TrustStore::default();
        trust.trust(PublisherTrustRecord {
            publisher_id: publisher.id.clone(),
            key_id: key.key_id.clone(),
            public_key: key.public_key.clone(),
            revoked: false,
        });
        assert_eq!(
            verify_package_signature(&package, Some(&publisher), Some(&signature), &trust)
                .expect("verify"),
            PackageTrustStatus::Trusted
        );
        fs::write(package.join("manifest.json"), b"tampered\n").unwrap();
        assert!(
            verify_package_signature(&package, Some(&publisher), Some(&signature), &trust).is_err()
        );
        let _ = fs::remove_dir_all(package);
    }

    #[test]
    fn trust_policy_is_explicit() {
        assert!(TrustPolicy::AllowUnsigned
            .enforce(PackageTrustStatus::Unsigned)
            .is_ok());
        assert!(TrustPolicy::RequireSigned
            .enforce(PackageTrustStatus::Unsigned)
            .is_err());
        assert!(TrustPolicy::RequireTrusted
            .enforce(PackageTrustStatus::Verified)
            .is_err());
    }

    #[test]
    fn trust_store_atomic_write_replaces_existing_document() {
        let root = temp_dir("trust-store-atomic");
        let path = root.join("plugin-trust.json");
        let mut trust = TrustStore::default();
        trust
            .write_atomic(&path)
            .expect("write initial trust store");
        trust.trust(PublisherTrustRecord {
            publisher_id: "publisher.atomic".to_owned(),
            key_id: "key-1".to_owned(),
            public_key: "public-key".to_owned(),
            revoked: false,
        });
        trust.set_policy(TrustPolicy::RequireTrusted);
        trust.trust_publisher_id("publisher.atomic");
        trust.write_atomic(&path).expect("replace trust store");

        let loaded = TrustStore::load(&path).expect("load replaced store");
        assert_eq!(loaded, trust);
        assert_eq!(loaded.policy, TrustPolicy::RequireTrusted);
        assert_eq!(loaded.trusted_publishers, ["publisher.atomic"]);
        assert_eq!(
            fs::read_dir(&root)
                .expect("list trust root")
                .filter_map(Result::ok)
                .filter(|entry| entry.file_name().to_string_lossy().contains(".tmp-"))
                .count(),
            0
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&root)
                    .expect("trust root metadata")
                    .permissions()
                    .mode()
                    & 0o777,
                0o700
            );
            assert_eq!(
                fs::metadata(&path)
                    .expect("trust metadata")
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
        let _ = fs::remove_dir_all(root);
    }
}
