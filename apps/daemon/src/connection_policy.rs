//! Persistent Loom identity and connection admission policy; secrets never enter public DTOs.
use anyhow::{bail, Context, Result};
use ring::{
    pbkdf2,
    rand::{SecureRandom, SystemRandom},
};
use serde::{Deserialize, Serialize};
use std::{
    num::NonZeroU32,
    path::{Path, PathBuf},
};

const ITERATIONS: u32 = 600_000;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum Approval {
    #[default]
    Manual,
    Lan,
    // Legacy persisted value; normalized to manual on load, never grants access.
    Password,
    Authenticated,
}

#[derive(Clone, Serialize, Deserialize)]
struct Password {
    salt: [u8; 16],
    digest: [u8; 32],
}

impl Password {
    fn create(value: &str) -> Result<Self> {
        if !(8..=256).contains(&value.len()) {
            bail!("Connection password must be 8-256 bytes");
        }
        let mut salt = [0; 16];
        SystemRandom::new()
            .fill(&mut salt)
            .map_err(|_| anyhow::anyhow!("Failed to generate password salt"))?;
        let mut digest = [0; 32];
        pbkdf2::derive(
            pbkdf2::PBKDF2_HMAC_SHA256,
            NonZeroU32::new(ITERATIONS).unwrap(),
            &salt,
            value.as_bytes(),
            &mut digest,
        );
        Ok(Self { salt, digest })
    }

    fn verify(&self, value: &str) -> bool {
        (8..=256).contains(&value.len())
            && pbkdf2::verify(
                pbkdf2::PBKDF2_HMAC_SHA256,
                NonZeroU32::new(ITERATIONS).unwrap(),
                &self.salt,
                value.as_bytes(),
                &self.digest,
            )
            .is_ok()
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Document {
    schema_version: u32,
    loom_id: String,
    name: String,
    discoverable: bool,
    endpoint: Option<String>,
    approval: Approval,
    password: Option<Password>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PublicInfo {
    pub protocol_version: &'static str,
    pub loom_id: String,
    pub name: String,
    pub discoverable: bool,
    pub endpoint: Option<String>,
    pub approval: Approval,
    pub password_required: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Update {
    pub name: String,
    pub discoverable: bool,
    pub endpoint: Option<String>,
    pub approval: Approval,
    /// Absent preserves the existing hash; empty explicitly removes the password.
    pub password: Option<String>,
}

#[derive(Clone)]
pub(crate) struct Store {
    path: PathBuf,
    document: Document,
}

impl Store {
    #[cfg(test)]
    pub fn open(path: &Path) -> Result<Self> {
        Self::open_with_identity(path, None)
    }

    pub fn open_with_identity(path: &Path, identity: Option<&str>) -> Result<Self> {
        use std::io::Read;
        let bytes = std::fs::File::open(path).and_then(|file| {
            let mut bytes = Vec::new();
            file.take(8193).read_to_end(&mut bytes).map(|_| bytes)
        });
        let document = match bytes {
            Ok(bytes) => {
                if bytes.len() > 8192 {
                    bail!("Connection policy file is too large");
                }
                let mut document: Document =
                    serde_json::from_slice(&bytes).context("Failed to parse connection policy")?;
                if document.schema_version != 1
                    || !valid_identity(&document.loom_id)
                    || identity.is_some_and(|id| id != document.loom_id)
                {
                    bail!("Invalid connection policy identity format; refusing to reset existing identity");
                }
                validate_fields(
                    &document.name,
                    document.discoverable,
                    document.endpoint.as_deref(),
                )?;
                if document.approval == Approval::Password {
                    document.approval = Approval::Manual;
                }
                document
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let document = Document {
                    schema_version: 1,
                    loom_id: identity
                        .map(str::to_owned)
                        .unwrap_or_else(|| format!("loom-{}", uuid::Uuid::new_v4())),
                    name: "Loom".into(),
                    discoverable: false,
                    endpoint: None,
                    approval: Approval::Manual,
                    password: None,
                };
                if !valid_identity(&document.loom_id) {
                    bail!("Invalid Loom identity");
                }
                crate::write_json_atomically(path, &document)?;
                document
            }
            Err(error) => return Err(error).context("Failed to read connection policy"),
        };
        Ok(Self {
            path: path.to_owned(),
            document,
        })
    }

    pub fn public_info(&self) -> PublicInfo {
        PublicInfo {
            protocol_version: "loom.connection.v1",
            loom_id: self.document.loom_id.clone(),
            name: self.document.name.clone(),
            discoverable: self.document.discoverable,
            endpoint: self.document.endpoint.clone(),
            approval: self.document.approval,
            password_required: self.document.password.is_some(),
        }
    }

    pub fn prepare_update(&self, input: Update) -> Result<Self> {
        validate_fields(&input.name, input.discoverable, input.endpoint.as_deref())?;
        let mut next = self.document.clone();
        next.name = input.name.trim().to_owned();
        next.discoverable = input.discoverable;
        next.endpoint = input
            .endpoint
            .map(|value| value.trim_end_matches('/').to_owned());
        next.approval = input.approval;
        if let Some(password) = input.password {
            next.password = if password.is_empty() {
                None
            } else {
                Some(Password::create(&password)?)
            };
        }
        if next.approval == Approval::Password {
            bail!("Select manual, LAN, or previously authenticated device approval policy");
        }
        Ok(Self {
            path: self.path.clone(),
            document: next,
        })
    }

    pub fn persist(&self) -> Result<()> {
        crate::write_json_atomically(&self.path, &self.document)
    }

    #[cfg(test)]
    pub fn update(&mut self, input: Update) -> Result<PublicInfo> {
        let next = self.prepare_update(input)?;
        next.persist()?;
        *self = next;
        Ok(self.public_info())
    }

    pub fn check_password(&self, password: &str) -> bool {
        self.document
            .password
            .as_ref()
            .is_none_or(|hash| hash.verify(password))
    }
}

fn valid_identity(value: &str) -> bool {
    value.strip_prefix("loom-").is_some_and(|id| {
        uuid::Uuid::parse_str(id).is_ok()
            || (id.len() == 64
                && id
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()))
    })
}

fn validate_fields(name: &str, discoverable: bool, endpoint: Option<&str>) -> Result<()> {
    if name.trim().is_empty() || name.len() > 80 || name.chars().any(char::is_control) {
        bail!("Loom name must be 1-80 bytes and must not contain control characters");
    }
    if discoverable && endpoint.is_none() {
        bail!("Public discovery requires an HTTPS endpoint");
    }
    if let Some(endpoint) = endpoint {
        let url = reqwest::Url::parse(endpoint).context("Invalid connection endpoint")?;
        if endpoint.len() > 256
            || url.scheme() != "https"
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || url.path() != "/"
        {
            bail!("Connection endpoint must be an HTTPS host and port without path, credentials, or query parameters");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
