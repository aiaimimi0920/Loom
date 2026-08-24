use serde::{Deserialize, Serialize};

pub(super) const CREDENTIALS_FILE: &str = "plugin-credentials.json";
pub(super) const CREDENTIAL_STORE_SCHEMA_VERSION: u32 = 2;
pub(super) const MAX_CREDENTIAL_FILE_BYTES: u64 = 4 * 1024 * 1024;
pub(super) const MAX_CREDENTIAL_VALUE_BYTES: usize = 1024 * 1024;
pub(super) const MAX_CREDENTIAL_JSON_DEPTH: usize = 32;

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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mcp_server_id: Option<String>,
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
pub(super) struct StoredCredential {
    pub(super) name: String,
    pub(super) protected_value: String,
    pub(super) protection: String,
    pub(super) value_type: CredentialValueType,
    #[serde(default)]
    pub(super) scope: CredentialScope,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) expires_at: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct CredentialFile {
    pub(super) schema_version: u32,
    #[serde(default)]
    pub(super) credentials: Vec<StoredCredential>,
}

impl Default for CredentialFile {
    fn default() -> Self {
        Self {
            schema_version: CREDENTIAL_STORE_SCHEMA_VERSION,
            credentials: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedCredentialValue {
    pub value_type: CredentialValueType,
    pub value: serde_json::Value,
}
