use loom_protocol::PackageTrustStatus;
use serde::{Deserialize, Serialize};

use crate::error::invalid_data;
use crate::{PluginSecurityError, SIGNING_KEY_SCHEMA_VERSION};

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

impl SigningKeyDocument {
    pub(crate) fn validate_schema(&self) -> Result<(), PluginSecurityError> {
        if self.schema_version != SIGNING_KEY_SCHEMA_VERSION {
            return Err(invalid_data(format!(
                "unsupported signing-key schema version {}; expected {}",
                self.schema_version, SIGNING_KEY_SCHEMA_VERSION
            )));
        }
        Ok(())
    }
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
