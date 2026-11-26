//! Scoped MCP Share Tokens for exposing Loom capabilities safely over `/mcp` and `loom mcp-stdio`.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use anyhow::Result;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest as _, Sha256};
use uuid::Uuid;

use crate::{invalid_request, path_id, structured_error, unix_time_millis, ParsedHttpRequest};

const TOKENS_FILE_REL: &str = "settings/mcp-share-tokens.json";
const MAX_SHARE_TOKENS: usize = 64;
const TOKEN_PREFIX: &str = "loom_mcp_";

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub enum McpShareScope {
    #[serde(rename = "arts:execute")]
    ArtsExecute,
    #[serde(rename = "capabilities:invoke")]
    CapabilitiesInvoke,
    #[serde(rename = "screen:read")]
    ScreenRead,
    #[serde(rename = "devices:control")]
    DevicesControl,
    #[serde(rename = "projection:control")]
    ProjectionControl,
}

impl McpShareScope {
    #[must_use]
    pub fn all() -> Vec<Self> {
        vec![
            Self::ArtsExecute,
            Self::CapabilitiesInvoke,
            Self::ScreenRead,
            Self::DevicesControl,
            Self::ProjectionControl,
        ]
    }
}

#[derive(Clone, Debug)]
pub struct McpShareGrant {
    pub token_id: String,
    pub scopes: Vec<McpShareScope>,
}

impl McpShareGrant {
    #[must_use]
    pub fn admin_all() -> Self {
        Self {
            token_id: "admin".to_owned(),
            scopes: McpShareScope::all(),
        }
    }

    #[must_use]
    pub fn allows(&self, scope: McpShareScope) -> bool {
        self.scopes.contains(&scope)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredMcpShareToken {
    id: String,
    label: String,
    token_prefix: String,
    secret_sha256: String,
    scopes: Vec<McpShareScope>,
    created_at_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    expires_at_ms: Option<u64>,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TokenStoreDocument {
    #[serde(default)]
    tokens: Vec<StoredMcpShareToken>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpShareTokenSummary {
    pub id: String,
    pub label: String,
    pub token_prefix: String,
    pub scopes: Vec<McpShareScope>,
    pub created_at_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at_ms: Option<u64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateShareTokenRequest {
    label: String,
    scopes: Vec<McpShareScope>,
    #[serde(default)]
    ttl_seconds: Option<u64>,
}

fn store_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn tokens_path(control_plane_root: &Path) -> PathBuf {
    control_plane_root.join(TOKENS_FILE_REL)
}

fn load_document(control_plane_root: &Path) -> Result<TokenStoreDocument> {
    use std::io::Read;
    const MAX_STORE_BYTES: u64 = 256 * 1024;
    let path = tokens_path(control_plane_root);
    match fs::File::open(&path) {
        Ok(file) => {
            let mut bytes = Vec::new();
            file.take(MAX_STORE_BYTES + 1).read_to_end(&mut bytes)?;
            anyhow::ensure!(
                bytes.len() as u64 <= MAX_STORE_BYTES,
                "MCP share token store exceeds byte limit"
            );
            let document: TokenStoreDocument = serde_json::from_slice(&bytes)?;
            anyhow::ensure!(
                document.tokens.len() <= MAX_SHARE_TOKENS,
                "MCP share token store exceeds token limit"
            );
            Ok(document)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Ok(TokenStoreDocument::default())
        }
        Err(error) => Err(error.into()),
    }
}

fn save_document(control_plane_root: &Path, doc: &TokenStoreDocument) -> Result<()> {
    let path = tokens_path(control_plane_root);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension(format!("tmp-{}", Uuid::new_v4()));
    let mut encoded = serde_json::to_vec_pretty(doc)?;
    encoded.push(b'\n');
    {
        let mut opts = OpenOptions::new();
        opts.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        let mut file = opts.open(&tmp)?;
        file.write_all(&encoded)?;
        file.sync_all()?;
    }
    fs::rename(&tmp, &path)?;
    Ok(())
}

fn constant_time_eq_hex(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.bytes().zip(b.bytes()) {
        diff |= x ^ y;
    }
    diff == 0
}

pub fn is_mcp_share_token_format(candidate: &str) -> bool {
    let trimmed = candidate.trim();
    trimmed.starts_with(TOKEN_PREFIX)
        && trimmed.len() >= TOKEN_PREFIX.len() + 24
        && trimmed.len() <= 256
}

pub fn authenticate_share_token(
    control_plane_root: &Path,
    candidate: &str,
) -> Option<McpShareGrant> {
    let trimmed = candidate.trim();
    if !is_mcp_share_token_format(trimmed) {
        return None;
    }
    let _guard = store_lock().lock().ok()?;
    let doc = load_document(control_plane_root).ok()?;
    let now = unix_time_millis();
    let candidate_hash = format!("{:x}", Sha256::digest(trimmed.as_bytes()));
    for entry in &doc.tokens {
        if entry.expires_at_ms.is_some_and(|exp| exp <= now) {
            continue;
        }
        if constant_time_eq_hex(&entry.secret_sha256, &candidate_hash) {
            return Some(McpShareGrant {
                token_id: entry.id.clone(),
                scopes: entry.scopes.clone(),
            });
        }
    }
    None
}

fn to_summary(entry: &StoredMcpShareToken) -> McpShareTokenSummary {
    McpShareTokenSummary {
        id: entry.id.clone(),
        label: entry.label.clone(),
        token_prefix: entry.token_prefix.clone(),
        scopes: entry.scopes.clone(),
        created_at_ms: entry.created_at_ms,
        expires_at_ms: entry.expires_at_ms,
    }
}

pub fn route_admin(
    request: &ParsedHttpRequest,
    route_path: &str,
    control_plane_root: &Path,
    settings_base_url: &str,
) -> Option<Result<(u16, String)>> {
    if !route_path.starts_with("/v1/mcp-share") {
        return None;
    }
    match (request.method.as_str(), route_path) {
        ("GET", "/v1/mcp-share/status") => {
            let origin = reqwest::Url::parse(settings_base_url)
                .ok()
                .map(|url| url.origin().ascii_serialization())
                .unwrap_or_else(|| "http://127.0.0.1:8765".to_owned());
            let _guard = store_lock()
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let now = unix_time_millis();
            let document = match load_document(control_plane_root) {
                Ok(document) => document,
                Err(error) => return Some(Err(error)),
            };
            let active_count = {
                document
                    .tokens
                    .iter()
                    .filter(|t| t.expires_at_ms.is_none_or(|exp| exp > now))
                    .count()
            };
            let relay_status = crate::cloud_relay::status_value(control_plane_root);
            let body = json!({
                "httpEndpoint": format!("{origin}/mcp"),
                "stdioCommand": "loom mcp-stdio",
                "activeTokenCount": active_count,
                "supportedScopes": McpShareScope::all(),
                "cloudRelay": relay_status,
            });
            Some(
                serde_json::to_string(&body)
                    .map(|s| (200, s))
                    .map_err(Into::into),
            )
        }
        ("GET", "/v1/mcp-share/tokens") => {
            let _guard = store_lock()
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let doc = match load_document(control_plane_root) {
                Ok(doc) => doc,
                Err(error) => return Some(Err(error)),
            };
            let summaries = doc.tokens.iter().map(to_summary).collect::<Vec<_>>();
            Some(
                serde_json::to_string(&json!({ "tokens": summaries }))
                    .map(|s| (200, s))
                    .map_err(Into::into),
            )
        }
        ("POST", "/v1/mcp-share/tokens") => {
            let input = match serde_json::from_str::<CreateShareTokenRequest>(&request.body) {
                Ok(input) => input,
                Err(error) => return Some(invalid_request(error.to_string())),
            };
            let label = input.label.trim();
            if label.is_empty() || label.len() > 80 {
                return Some(invalid_request("token label must be 1 to 80 characters"));
            }
            if input.scopes.is_empty() {
                return Some(invalid_request("at least one MCP share scope is required"));
            }
            let mut scopes = input.scopes;
            scopes.sort();
            scopes.dedup();

            let _guard = store_lock()
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let mut doc = match load_document(control_plane_root) {
                Ok(doc) => doc,
                Err(error) => return Some(Err(error)),
            };
            let now = unix_time_millis();
            doc.tokens
                .retain(|t| t.expires_at_ms.is_none_or(|exp| exp > now));
            if doc.tokens.len() >= MAX_SHARE_TOKENS {
                return Some(structured_error(
                    429,
                    json!({ "code": "mcp_share_token_limit", "message": "maximum MCP share tokens reached" }),
                ));
            }
            let mut raw_random = [0u8; 32];
            OsRng.fill_bytes(&mut raw_random);
            let plaintext = format!("{TOKEN_PREFIX}{}", URL_SAFE_NO_PAD.encode(raw_random));
            let prefix = plaintext.chars().take(16).collect::<String>();
            let secret_sha256 = format!("{:x}", Sha256::digest(plaintext.as_bytes()));
            let expires_at_ms = input
                .ttl_seconds
                .filter(|secs| *secs > 0)
                .map(|secs| now.saturating_add(secs.saturating_mul(1000)));
            let entry = StoredMcpShareToken {
                id: format!("mcp-tok-{}", Uuid::new_v4()),
                label: label.to_owned(),
                token_prefix: prefix,
                secret_sha256,
                scopes,
                created_at_ms: now,
                expires_at_ms,
            };
            let summary = to_summary(&entry);
            doc.tokens.push(entry);
            if let Err(error) = save_document(control_plane_root, &doc) {
                return Some(Err(error));
            }
            Some(
                serde_json::to_string(&json!({
                    "token": plaintext,
                    "summary": summary,
                }))
                .map(|s| (201, s))
                .map_err(Into::into),
            )
        }
        ("DELETE", path) if path_id(path, "/v1/mcp-share/tokens/").is_some() => {
            let id = path_id(path, "/v1/mcp-share/tokens/").expect("checked path");
            let _guard = store_lock()
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let mut doc = match load_document(control_plane_root) {
                Ok(doc) => doc,
                Err(error) => return Some(Err(error)),
            };
            let before = doc.tokens.len();
            doc.tokens.retain(|t| t.id != id);
            if doc.tokens.len() == before {
                return Some(structured_error(
                    404,
                    json!({ "code": "token_not_found", "message": "MCP share token not found" }),
                ));
            }
            if let Err(error) = save_document(control_plane_root, &doc) {
                return Some(Err(error));
            }
            Some(
                serde_json::to_string(&json!({ "deleted": true, "id": id }))
                    .map(|s| (200, s))
                    .map_err(Into::into),
            )
        }
        _ => None,
    }
}
