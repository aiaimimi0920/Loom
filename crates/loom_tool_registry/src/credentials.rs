use std::collections::BTreeMap;
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use chrono::{DateTime, Utc};
use loom_protocol::{is_safe_package_id, is_safe_publisher_id, CredentialGrant};
use serde::{Deserialize, Serialize};

const CREDENTIALS_FILE: &str = "plugin-credentials.json";
const CREDENTIAL_STORE_SCHEMA_VERSION: u32 = 2;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CredentialValueType {
    #[default]
    String,
    Number,
    Integer,
    Boolean,
    Json,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialScope {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub framework_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub art_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialInput {
    pub name: String,
    pub value: String,
    #[serde(default)]
    pub value_type: CredentialValueType,
    #[serde(default)]
    pub scope: CredentialScope,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialSummary {
    pub name: String,
    #[serde(default)]
    pub value_type: CredentialValueType,
    pub scope: CredentialScope,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    pub protection: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialDetails {
    pub name: String,
    pub value: String,
    #[serde(default)]
    pub value_type: CredentialValueType,
    pub scope: CredentialScope,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    pub protection: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredCredential {
    name: String,
    protected_value: String,
    protection: String,
    #[serde(default)]
    value_type: CredentialValueType,
    #[serde(default)]
    scope: CredentialScope,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    expires_at: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CredentialFile {
    #[serde(default = "default_schema_version")]
    schema_version: u32,
    #[serde(default)]
    credentials: Vec<StoredCredential>,
}

impl Default for CredentialFile {
    fn default() -> Self {
        Self {
            schema_version: CREDENTIAL_STORE_SCHEMA_VERSION,
            credentials: Vec::new(),
        }
    }
}

const fn default_schema_version() -> u32 {
    CREDENTIAL_STORE_SCHEMA_VERSION
}

#[derive(Debug, thiserror::Error)]
pub enum CredentialError {
    #[error("credential name is not a safe id: {0}")]
    UnsafeName(String),
    #[error("credential scope contains an unsafe package id: {0}")]
    UnsafeScope(String),
    #[error("credential value is empty")]
    EmptyValue,
    #[error("credential value is invalid for type `{value_type:?}`: {reason}")]
    InvalidValue {
        value_type: CredentialValueType,
        reason: String,
    },
    #[error("credential expiration is invalid: {0}")]
    InvalidExpiration(String),
    #[error("credential `{credential}` referenced by Art field `{alias}` is missing, expired, or outside its scope")]
    MissingBinding { alias: String, credential: String },
    #[error("credential `{credential}` referenced by secret Art field `{alias}` must be a string, got `{actual:?}`")]
    NonStringSecretBinding {
        alias: String,
        credential: String,
        actual: CredentialValueType,
    },
    #[error("credential protection failed: {0}")]
    Protection(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("base64 error: {0}")]
    Base64(#[from] base64::DecodeError),
}

#[derive(Clone, Debug)]
pub struct CredentialStore {
    path: PathBuf,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedCredentialValue {
    pub value_type: CredentialValueType,
    pub value: serde_json::Value,
}

impl CredentialStore {
    pub fn new(control_plane_root: impl AsRef<Path>) -> Self {
        Self {
            path: control_plane_root.as_ref().join(CREDENTIALS_FILE),
        }
    }

    pub fn summaries(&self) -> Result<Vec<CredentialSummary>, CredentialError> {
        let mut summaries = self
            .read_file()?
            .credentials
            .into_iter()
            .map(|credential| CredentialSummary {
                name: credential.name,
                value_type: credential.value_type,
                scope: credential.scope,
                expires_at: credential.expires_at,
                protection: credential.protection,
            })
            .collect::<Vec<_>>();
        summaries.sort_by(|left, right| {
            (&left.name, &left.scope.framework_id, &left.scope.art_id).cmp(&(
                &right.name,
                &right.scope.framework_id,
                &right.scope.art_id,
            ))
        });
        Ok(summaries)
    }

    pub fn reveal(
        &self,
        name: &str,
        scope: &CredentialScope,
    ) -> Result<Option<CredentialDetails>, CredentialError> {
        let Some(credential) = self
            .read_file()?
            .credentials
            .into_iter()
            .find(|credential| credential.name == name && &credential.scope == scope)
        else {
            return Ok(None);
        };
        let bytes = unprotect_value(&credential.protected_value, &credential.protection)?;
        let value = String::from_utf8(bytes)
            .map_err(|error| CredentialError::Protection(error.to_string()))?;
        Ok(Some(CredentialDetails {
            name: credential.name,
            value,
            value_type: credential.value_type,
            scope: credential.scope,
            expires_at: credential.expires_at,
            protection: credential.protection,
        }))
    }

    pub fn upsert(&self, input: CredentialInput) -> Result<CredentialSummary, CredentialError> {
        validate_input(&input)?;
        let canonical_value = canonicalize_value(input.value_type, &input.value)?;
        let (protected_value, protection) = protect_value(canonical_value.as_bytes())?;
        let mut file = self.read_file()?;
        file.schema_version = CREDENTIAL_STORE_SCHEMA_VERSION;
        file.credentials
            .retain(|credential| credential.name != input.name || credential.scope != input.scope);
        let summary = CredentialSummary {
            name: input.name.clone(),
            value_type: input.value_type,
            scope: input.scope.clone(),
            expires_at: input.expires_at.clone(),
            protection: protection.clone(),
        };
        file.credentials.push(StoredCredential {
            name: input.name,
            protected_value,
            protection,
            value_type: input.value_type,
            scope: input.scope,
            expires_at: input.expires_at,
        });
        self.write_file(&file)?;
        Ok(summary)
    }

    pub fn delete(&self, name: &str, scope: &CredentialScope) -> Result<bool, CredentialError> {
        let mut file = self.read_file()?;
        let before = file.credentials.len();
        file.credentials
            .retain(|credential| credential.name != name || &credential.scope != scope);
        let changed = file.credentials.len() != before;
        if changed {
            file.schema_version = CREDENTIAL_STORE_SCHEMA_VERSION;
            self.write_file(&file)?;
        }
        Ok(changed)
    }

    pub fn grants_for(
        &self,
        framework_id: &str,
        art_id: &str,
        requested: &[String],
    ) -> Result<Vec<CredentialGrant>, CredentialError> {
        let now = Utc::now();
        let mut grants = Vec::new();
        for credential in self.read_file()?.credentials {
            if !requested.iter().any(|name| name == &credential.name)
                || credential
                    .scope
                    .framework_id
                    .as_deref()
                    .is_some_and(|scope| scope != framework_id)
                || credential
                    .scope
                    .art_id
                    .as_deref()
                    .is_some_and(|scope| scope != art_id)
            {
                continue;
            }
            if credential
                .expires_at
                .as_deref()
                .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
                .is_some_and(|expires| expires.with_timezone(&Utc) <= now)
            {
                continue;
            }
            let bytes = unprotect_value(&credential.protected_value, &credential.protection)?;
            let value = String::from_utf8(bytes)
                .map_err(|error| CredentialError::Protection(error.to_string()))?;
            grants.push(CredentialGrant {
                name: credential.name,
                value,
                expires_at: credential.expires_at,
            });
        }
        Ok(grants)
    }

    pub fn grants_for_bindings(
        &self,
        framework_id: &str,
        art_id: &str,
        bindings: &BTreeMap<String, String>,
    ) -> Result<Vec<CredentialGrant>, CredentialError> {
        let now = Utc::now();
        let credentials = self.read_file()?.credentials;
        let mut grants = Vec::with_capacity(bindings.len());
        for (alias, credential_name) in bindings {
            let credential = credentials
                .iter()
                .filter(|credential| {
                    credential.name == *credential_name
                        && credential
                            .scope
                            .framework_id
                            .as_deref()
                            .is_none_or(|scope| scope == framework_id)
                        && credential
                            .scope
                            .art_id
                            .as_deref()
                            .is_none_or(|scope| scope == art_id)
                        && !credential
                            .expires_at
                            .as_deref()
                            .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
                            .is_some_and(|expires| expires.with_timezone(&Utc) <= now)
                })
                .max_by_key(|credential| {
                    usize::from(credential.scope.framework_id.is_some())
                        + usize::from(credential.scope.art_id.is_some())
                })
                .ok_or_else(|| CredentialError::MissingBinding {
                    alias: alias.clone(),
                    credential: credential_name.clone(),
                })?;
            if credential.value_type != CredentialValueType::String {
                return Err(CredentialError::NonStringSecretBinding {
                    alias: alias.clone(),
                    credential: credential_name.clone(),
                    actual: credential.value_type,
                });
            }
            let bytes = unprotect_value(&credential.protected_value, &credential.protection)?;
            grants.push(CredentialGrant {
                name: alias.clone(),
                value: String::from_utf8(bytes)
                    .map_err(|error| CredentialError::Protection(error.to_string()))?,
                expires_at: credential.expires_at.clone(),
            });
        }
        Ok(grants)
    }

    pub fn global_values_for_bindings(
        &self,
        bindings: &BTreeMap<String, String>,
    ) -> Result<BTreeMap<String, ResolvedCredentialValue>, CredentialError> {
        let now = Utc::now();
        let credentials = self.read_file()?.credentials;
        let mut values = BTreeMap::new();
        for (alias, credential_name) in bindings {
            let credential = credentials
                .iter()
                .find(|credential| {
                    credential.name == *credential_name
                        && credential.scope.framework_id.is_none()
                        && credential.scope.art_id.is_none()
                        && !credential
                            .expires_at
                            .as_deref()
                            .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
                            .is_some_and(|expires| expires.with_timezone(&Utc) <= now)
                })
                .ok_or_else(|| CredentialError::MissingBinding {
                    alias: alias.clone(),
                    credential: credential_name.clone(),
                })?;
            let bytes = unprotect_value(&credential.protected_value, &credential.protection)?;
            let raw = String::from_utf8(bytes)
                .map_err(|error| CredentialError::Protection(error.to_string()))?;
            values.insert(
                alias.clone(),
                ResolvedCredentialValue {
                    value_type: credential.value_type,
                    value: decode_canonical_value(credential.value_type, &raw)?,
                },
            );
        }
        Ok(values)
    }

    fn read_file(&self) -> Result<CredentialFile, CredentialError> {
        if !self.path.exists() {
            return Ok(CredentialFile::default());
        }
        Ok(serde_json::from_slice(&fs::read(&self.path)?)?)
    }

    fn write_file(&self, file: &CredentialFile) -> Result<(), CredentialError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
            restrict_path_permissions(parent, true)?;
        }
        let mut bytes = serde_json::to_vec_pretty(file)?;
        bytes.push(b'\n');
        let (temporary, mut output) = create_sensitive_temporary(&self.path)?;
        let result = (|| {
            output.write_all(&bytes)?;
            output.sync_all()?;
            drop(output);
            restrict_path_permissions(&temporary, false)?;
            crate::replace_registry_file(&temporary, &self.path)?;
            restrict_path_permissions(&self.path, false)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }
}

fn create_sensitive_temporary(path: &Path) -> Result<(PathBuf, fs::File), CredentialError> {
    let parent = path.parent().ok_or_else(|| {
        CredentialError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "credential path has no parent",
        ))
    })?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            CredentialError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "credential path has no UTF-8 file name",
            ))
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
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        match options.open(&temporary) {
            Ok(file) => return Ok((temporary, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(CredentialError::Io(error)),
        }
    }
    Err(CredentialError::Io(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "could not allocate a unique credential temporary file",
    )))
}

fn validate_input(input: &CredentialInput) -> Result<(), CredentialError> {
    if !is_safe_package_id(&input.name) {
        return Err(CredentialError::UnsafeName(input.name.clone()));
    }
    for scope in [
        input.scope.framework_id.as_deref(),
        input.scope.art_id.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        if !is_safe_scope_reference(scope) {
            return Err(CredentialError::UnsafeScope(scope.to_owned()));
        }
    }
    if input.value.is_empty() {
        return Err(CredentialError::EmptyValue);
    }
    canonicalize_value(input.value_type, &input.value)?;
    if let Some(expires_at) = &input.expires_at {
        DateTime::parse_from_rfc3339(expires_at)
            .map_err(|error| CredentialError::InvalidExpiration(error.to_string()))?;
    }
    Ok(())
}

fn canonicalize_value(
    value_type: CredentialValueType,
    raw: &str,
) -> Result<String, CredentialError> {
    if raw.is_empty() {
        return Err(CredentialError::EmptyValue);
    }
    match value_type {
        CredentialValueType::String => Ok(raw.to_owned()),
        CredentialValueType::Number => match serde_json::from_str::<serde_json::Value>(raw) {
            Ok(serde_json::Value::Number(value)) => Ok(value.to_string()),
            Ok(_) => Err(invalid_value(value_type, "expected a JSON number")),
            Err(error) => Err(invalid_value(value_type, error.to_string())),
        },
        CredentialValueType::Integer => match serde_json::from_str::<serde_json::Value>(raw) {
            Ok(serde_json::Value::Number(value)) if value.is_i64() || value.is_u64() => {
                Ok(value.to_string())
            }
            Ok(_) => Err(invalid_value(value_type, "expected a JSON integer")),
            Err(error) => Err(invalid_value(value_type, error.to_string())),
        },
        CredentialValueType::Boolean => match serde_json::from_str::<serde_json::Value>(raw) {
            Ok(serde_json::Value::Bool(value)) => Ok(value.to_string()),
            Ok(_) => Err(invalid_value(value_type, "expected true or false")),
            Err(error) => Err(invalid_value(value_type, error.to_string())),
        },
        CredentialValueType::Json => serde_json::from_str::<serde_json::Value>(raw)
            .and_then(|value| serde_json::to_string(&value))
            .map_err(|error| invalid_value(value_type, error.to_string())),
    }
}

fn decode_canonical_value(
    value_type: CredentialValueType,
    raw: &str,
) -> Result<serde_json::Value, CredentialError> {
    match value_type {
        CredentialValueType::String => Ok(serde_json::Value::String(raw.to_owned())),
        _ => {
            serde_json::from_str(raw).map_err(|error| invalid_value(value_type, error.to_string()))
        }
    }
}

fn invalid_value(value_type: CredentialValueType, reason: impl Into<String>) -> CredentialError {
    CredentialError::InvalidValue {
        value_type,
        reason: reason.into(),
    }
}

fn is_safe_scope_reference(value: &str) -> bool {
    value
        .split_once('/')
        .map(|(publisher, id)| {
            !id.contains('/') && is_safe_publisher_id(publisher) && is_safe_package_id(id)
        })
        .unwrap_or_else(|| is_safe_package_id(value))
}

#[cfg(windows)]
fn protect_value(value: &[u8]) -> Result<(String, String), CredentialError> {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{
        CryptProtectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    };

    let mut input = CRYPT_INTEGER_BLOB {
        cbData: value.len() as u32,
        pbData: value.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    let protected = unsafe {
        CryptProtectData(
            &mut input,
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null_mut(),
            std::ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    if protected == 0 {
        return Err(CredentialError::Protection(
            std::io::Error::last_os_error().to_string(),
        ));
    }
    let bytes = unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize) };
    let encoded = BASE64.encode(bytes);
    unsafe {
        LocalFree(output.pbData as *mut _);
    }
    Ok((encoded, "windows-dpapi-current-user".to_owned()))
}

#[cfg(windows)]
fn unprotect_value(value: &str, protection: &str) -> Result<Vec<u8>, CredentialError> {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{
        CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    };

    if protection != "windows-dpapi-current-user" {
        return Err(CredentialError::Protection(format!(
            "unsupported protection `{protection}`"
        )));
    }
    let mut encrypted = BASE64.decode(value.as_bytes())?;
    let mut input = CRYPT_INTEGER_BLOB {
        cbData: encrypted.len() as u32,
        pbData: encrypted.as_mut_ptr(),
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    let unprotected = unsafe {
        CryptUnprotectData(
            &mut input,
            std::ptr::null_mut(),
            std::ptr::null(),
            std::ptr::null_mut(),
            std::ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    if unprotected == 0 {
        return Err(CredentialError::Protection(
            std::io::Error::last_os_error().to_string(),
        ));
    }
    let bytes = unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize) };
    let result = bytes.to_vec();
    unsafe {
        LocalFree(output.pbData as *mut _);
    }
    Ok(result)
}

#[cfg(not(windows))]
fn protect_value(value: &[u8]) -> Result<(String, String), CredentialError> {
    Ok((BASE64.encode(value), "local-file-base64".to_owned()))
}

#[cfg(not(windows))]
fn unprotect_value(value: &str, protection: &str) -> Result<Vec<u8>, CredentialError> {
    if protection != "local-file-base64" {
        return Err(CredentialError::Protection(format!(
            "unsupported protection `{protection}`"
        )));
    }
    Ok(BASE64.decode(value.as_bytes())?)
}

#[cfg(unix)]
fn restrict_path_permissions(path: &Path, directory: bool) -> Result<(), CredentialError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(
        path,
        fs::Permissions::from_mode(if directory { 0o700 } else { 0o600 }),
    )?;
    Ok(())
}

#[cfg(windows)]
fn restrict_path_permissions(path: &Path, _directory: bool) -> Result<(), CredentialError> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Authorization::{
        ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
    };
    use windows_sys::Win32::Security::{
        SetFileSecurityW, DACL_SECURITY_INFORMATION, PROTECTED_DACL_SECURITY_INFORMATION,
        PSECURITY_DESCRIPTOR,
    };

    let canonical = fs::canonicalize(path)?;
    let wide = canonical.as_os_str().encode_wide().collect::<Vec<_>>();
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
    let sddl = "D:P(A;;FA;;;OW)(A;;FA;;;SY)\0"
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
        return Err(CredentialError::Io(std::io::Error::last_os_error()));
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
        return Err(CredentialError::Io(std::io::Error::last_os_error()));
    }
    Ok(())
}

#[cfg(not(any(unix, windows)))]
fn restrict_path_permissions(_path: &Path, _directory: bool) -> Result<(), CredentialError> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root() -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "loom-credentials-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        let _ = fs::remove_dir_all(&path);
        path
    }

    #[test]
    fn credentials_are_scoped_and_never_returned_in_summaries() {
        let root = temp_root();
        let store = CredentialStore::new(&root);
        store
            .upsert(CredentialInput {
                name: "api_key".to_owned(),
                value: "secret-value".to_owned(),
                value_type: CredentialValueType::String,
                scope: CredentialScope {
                    framework_id: Some("cloud_api".to_owned()),
                    art_id: Some("example-art".to_owned()),
                },
                expires_at: None,
            })
            .expect("upsert");
        let summaries = store.summaries().expect("summaries");
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].value_type, CredentialValueType::String);
        assert!(!serde_json::to_string(&summaries)
            .unwrap()
            .contains("secret-value"));
        let revealed = store
            .reveal("api_key", &summaries[0].scope)
            .expect("reveal")
            .expect("stored credential");
        assert_eq!(revealed.value, "secret-value");
        assert_eq!(revealed.value_type, CredentialValueType::String);
        assert!(store
            .grants_for("cloud_api", "other-art", &["api_key".to_owned()])
            .unwrap()
            .is_empty());
        let grants = store
            .grants_for("cloud_api", "example-art", &["api_key".to_owned()])
            .expect("grants");
        assert_eq!(grants[0].value, "secret-value");
        let credential_path = root.join(CREDENTIALS_FILE);
        assert!(credential_path.is_file());
        assert_eq!(
            fs::read_dir(&root)
                .expect("list credential root")
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
                    .expect("credential root metadata")
                    .permissions()
                    .mode()
                    & 0o777,
                0o700
            );
            assert_eq!(
                fs::metadata(&credential_path)
                    .expect("credential file metadata")
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn global_credentials_can_be_reused_under_art_specific_aliases() {
        let root = temp_root();
        let store = CredentialStore::new(&root);
        store
            .upsert(CredentialInput {
                name: "cloudflare_key".to_owned(),
                value: "global-secret".to_owned(),
                value_type: CredentialValueType::String,
                scope: CredentialScope::default(),
                expires_at: None,
            })
            .unwrap();
        let bindings = BTreeMap::from([("api_token".to_owned(), "cloudflare_key".to_owned())]);
        for art_id in ["art-a", "art-b"] {
            let grants = store
                .grants_for_bindings("cloud_api", art_id, &bindings)
                .unwrap();
            assert_eq!(grants.len(), 1);
            assert_eq!(grants[0].name, "api_token");
            assert_eq!(grants[0].value, "global-secret");
        }
        let missing = store
            .grants_for_bindings(
                "cloud_api",
                "art-a",
                &BTreeMap::from([("token".to_owned(), "missing".to_owned())]),
            )
            .unwrap_err();
        assert!(matches!(missing, CredentialError::MissingBinding { .. }));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn credential_scopes_accept_publisher_qualified_package_ids() {
        let root = temp_root();
        let store = CredentialStore::new(&root);
        store
            .upsert(CredentialInput {
                name: "art_secret".to_owned(),
                value: "secret-value".to_owned(),
                value_type: CredentialValueType::String,
                scope: CredentialScope {
                    framework_id: Some("neuro.official/mcp".to_owned()),
                    art_id: Some("neuro.official/custom-image-search".to_owned()),
                },
                expires_at: None,
            })
            .expect("qualified scope");
        let grants = store
            .grants_for_bindings(
                "neuro.official/mcp",
                "neuro.official/custom-image-search",
                &BTreeMap::from([("brave_api_key".to_owned(), "art_secret".to_owned())]),
            )
            .expect("qualified grants");
        assert_eq!(grants[0].value, "secret-value");
        assert!(!is_safe_scope_reference("publisher/art/extra"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn typed_global_values_are_canonicalized_and_resolved_without_disclosure() {
        let root = temp_root();
        let store = CredentialStore::new(&root);
        for (name, value, value_type) in [
            ("ratio", "3.500", CredentialValueType::Number),
            ("count", "3", CredentialValueType::Integer),
            ("enabled", "true", CredentialValueType::Boolean),
            (
                "payload",
                "{ \"b\": 2, \"a\": 1 }",
                CredentialValueType::Json,
            ),
        ] {
            store
                .upsert(CredentialInput {
                    name: name.to_owned(),
                    value: value.to_owned(),
                    value_type,
                    scope: CredentialScope::default(),
                    expires_at: None,
                })
                .unwrap();
        }
        let values = store
            .global_values_for_bindings(&BTreeMap::from([
                ("quality".to_owned(), "ratio".to_owned()),
                ("count".to_owned(), "count".to_owned()),
                ("enabled".to_owned(), "enabled".to_owned()),
                ("config".to_owned(), "payload".to_owned()),
            ]))
            .unwrap();
        assert_eq!(values["quality"].value, serde_json::json!(3.5));
        assert_eq!(values["count"].value, serde_json::json!(3));
        assert_eq!(values["enabled"].value, serde_json::json!(true));
        assert_eq!(
            values["config"].value,
            serde_json::json!({ "a": 1, "b": 2 })
        );
        let serialized = fs::read_to_string(root.join(CREDENTIALS_FILE)).unwrap();
        assert!(!serialized.contains("3.500"));
        assert!(!serde_json::to_string(&store.summaries().unwrap())
            .unwrap()
            .contains("\"a\":1"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn typed_values_reject_invalid_inputs_and_non_string_secret_bindings() {
        let root = temp_root();
        let store = CredentialStore::new(&root);
        for (name, value, value_type) in [
            ("integer", "1.5", CredentialValueType::Integer),
            ("boolean", "yes", CredentialValueType::Boolean),
            ("json", "{", CredentialValueType::Json),
        ] {
            assert!(matches!(
                store.upsert(CredentialInput {
                    name: name.to_owned(),
                    value: value.to_owned(),
                    value_type,
                    scope: CredentialScope::default(),
                    expires_at: None,
                }),
                Err(CredentialError::InvalidValue { .. })
            ));
        }
        store
            .upsert(CredentialInput {
                name: "count".to_owned(),
                value: "3".to_owned(),
                value_type: CredentialValueType::Integer,
                scope: CredentialScope::default(),
                expires_at: None,
            })
            .unwrap();
        assert!(matches!(
            store.grants_for_bindings(
                "process",
                "sample",
                &BTreeMap::from([("api_key".to_owned(), "count".to_owned())]),
            ),
            Err(CredentialError::NonStringSecretBinding { .. })
        ));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn legacy_records_without_value_type_default_to_string() {
        let root = temp_root();
        fs::create_dir_all(&root).unwrap();
        let (protected_value, protection) = protect_value(b"legacy-secret").unwrap();
        fs::write(
            root.join(CREDENTIALS_FILE),
            serde_json::to_vec_pretty(&serde_json::json!({
                "schemaVersion": 1,
                "credentials": [{
                    "name": "legacy",
                    "protectedValue": protected_value,
                    "protection": protection,
                    "scope": {}
                }]
            }))
            .unwrap(),
        )
        .unwrap();
        let store = CredentialStore::new(&root);
        assert_eq!(
            store.summaries().unwrap()[0].value_type,
            CredentialValueType::String
        );
        assert_eq!(
            store
                .global_values_for_bindings(&BTreeMap::from([(
                    "value".to_owned(),
                    "legacy".to_owned(),
                )]))
                .unwrap()["value"]
                .value,
            serde_json::json!("legacy-secret")
        );
        let _ = fs::remove_dir_all(root);
    }
}
