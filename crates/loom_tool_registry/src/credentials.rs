use std::fs;
use std::path::{Path, PathBuf};

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use chrono::{DateTime, Utc};
use loom_protocol::{is_safe_package_id, CredentialGrant};
use serde::{Deserialize, Serialize};

const CREDENTIALS_FILE: &str = "plugin-credentials.json";
const CREDENTIAL_STORE_SCHEMA_VERSION: u32 = 1;

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
    pub scope: CredentialScope,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialSummary {
    pub name: String,
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
    #[error("credential expiration is invalid: {0}")]
    InvalidExpiration(String),
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

    pub fn upsert(&self, input: CredentialInput) -> Result<CredentialSummary, CredentialError> {
        validate_input(&input)?;
        let (protected_value, protection) = protect_value(input.value.as_bytes())?;
        let mut file = self.read_file()?;
        file.credentials
            .retain(|credential| credential.name != input.name || credential.scope != input.scope);
        let summary = CredentialSummary {
            name: input.name.clone(),
            scope: input.scope.clone(),
            expires_at: input.expires_at.clone(),
            protection: protection.clone(),
        };
        file.credentials.push(StoredCredential {
            name: input.name,
            protected_value,
            protection,
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
            grants.push(CredentialGrant {
                name: credential.name,
                value: String::from_utf8(bytes)
                    .map_err(|error| CredentialError::Protection(error.to_string()))?,
                expires_at: credential.expires_at,
            });
        }
        Ok(grants)
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
        }
        let temporary = self.path.with_extension("json.tmp");
        let mut bytes = serde_json::to_vec_pretty(file)?;
        bytes.push(b'\n');
        fs::write(&temporary, bytes)?;
        crate::replace_registry_file(&temporary, &self.path)?;
        restrict_file_permissions(&self.path)?;
        Ok(())
    }
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
        if !is_safe_package_id(scope) {
            return Err(CredentialError::UnsafeScope(scope.to_owned()));
        }
    }
    if input.value.is_empty() {
        return Err(CredentialError::EmptyValue);
    }
    if let Some(expires_at) = &input.expires_at {
        DateTime::parse_from_rfc3339(expires_at)
            .map_err(|error| CredentialError::InvalidExpiration(error.to_string()))?;
    }
    Ok(())
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
fn restrict_file_permissions(path: &Path) -> Result<(), CredentialError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    Ok(())
}

#[cfg(not(unix))]
fn restrict_file_permissions(_path: &Path) -> Result<(), CredentialError> {
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
                scope: CredentialScope {
                    framework_id: Some("cloud_api".to_owned()),
                    art_id: Some("example-art".to_owned()),
                },
                expires_at: None,
            })
            .expect("upsert");
        let summaries = store.summaries().expect("summaries");
        assert_eq!(summaries.len(), 1);
        assert!(!serde_json::to_string(&summaries)
            .unwrap()
            .contains("secret-value"));
        assert!(store
            .grants_for("cloud_api", "other-art", &["api_key".to_owned()])
            .unwrap()
            .is_empty());
        let grants = store
            .grants_for("cloud_api", "example-art", &["api_key".to_owned()])
            .expect("grants");
        assert_eq!(grants[0].value, "secret-value");
        let _ = fs::remove_dir_all(root);
    }
}
