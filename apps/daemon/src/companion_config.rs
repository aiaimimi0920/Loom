//! Schema-driven Companion App Settings Hub, allowing family/desktop companion apps
//! to register and persist revisioned configuration in Loom as a one-stop hub.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use anyhow::Result;
use loom_configuration::UiSection;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    invalid_request, path_id, path_id_with_suffix, structured_error, unix_time_millis,
    ParsedHttpRequest,
};

const COMPANION_SUBDIR: &str = "companion-apps";
const MAX_COMPANION_DOC_BYTES: usize = 256 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompanionAppConfigDocument {
    pub app_id: String,
    pub display_name: String,
    pub schema_version: u32,
    pub revision: u64,
    pub updated_at_ms: u64,
    pub sections: Vec<UiSection>,
    pub default_config: Value,
    pub config: Value,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RegisterCompanionSchemaRequest {
    display_name: String,
    #[serde(default = "default_schema_version")]
    schema_version: u32,
    #[serde(default)]
    sections: Vec<UiSection>,
    default_config: Value,
}

fn default_schema_version() -> u32 {
    1
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateCompanionConfigRequest {
    expected_revision: u64,
    config: Value,
}

fn companion_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn is_safe_companion_app_id(app_id: &str) -> bool {
    let trimmed = app_id.trim();
    !trimmed.is_empty()
        && trimmed.len() <= 64
        && !matches!(trimmed, "loom" | "gateway")
        && trimmed
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'-' | b'_'))
}

fn companion_dir(config_root: &Path) -> PathBuf {
    config_root.join(COMPANION_SUBDIR)
}

fn doc_path(config_root: &Path, app_id: &str) -> PathBuf {
    companion_dir(config_root).join(format!("{app_id}.json"))
}

fn read_doc(config_root: &Path, app_id: &str) -> Result<Option<CompanionAppConfigDocument>> {
    match fs::read(doc_path(config_root, app_id)) {
        Ok(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn write_doc(config_root: &Path, doc: &CompanionAppConfigDocument) -> Result<()> {
    let dir = companion_dir(config_root);
    fs::create_dir_all(&dir)?;
    let path = doc_path(config_root, &doc.app_id);
    let tmp = path.with_extension(format!("tmp-{}", Uuid::new_v4()));
    let mut encoded = serde_json::to_vec_pretty(doc)?;
    encoded.push(b'\n');
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&tmp)?;
    file.write_all(&encoded)?;
    file.sync_all()?;
    fs::rename(&tmp, &path)?;
    Ok(())
}

pub fn route(
    request: &ParsedHttpRequest,
    route_path: &str,
    config_root: &Path,
) -> Option<Result<(u16, String)>> {
    if route_path != "/v1/configuration/companion-apps"
        && !route_path.starts_with("/v1/configuration/companion-apps/")
    {
        return None;
    }
    if request.body.len() > MAX_COMPANION_DOC_BYTES {
        return Some(structured_error(
            413,
            json!({ "code": "companion_config_too_large" }),
        ));
    }

    match (request.method.as_str(), route_path) {
        ("GET", "/v1/configuration/companion-apps") => {
            let _guard = companion_lock()
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let mut apps = Vec::new();
            if let Ok(entries) = fs::read_dir(companion_dir(config_root)) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|e| e.to_str()) == Some("json") {
                        if let Ok(bytes) = fs::read(&path) {
                            if let Ok(doc) =
                                serde_json::from_slice::<CompanionAppConfigDocument>(&bytes)
                            {
                                apps.push(json!({
                                    "appId": doc.app_id,
                                    "displayName": doc.display_name,
                                    "schemaVersion": doc.schema_version,
                                    "revision": doc.revision,
                                    "updatedAtMs": doc.updated_at_ms,
                                }));
                            }
                        }
                    }
                }
            }
            apps.sort_by(|a, b| a["appId"].as_str().cmp(&b["appId"].as_str()));
            Some(
                serde_json::to_string(&json!({ "apps": apps }))
                    .map(|s| (200, s))
                    .map_err(Into::into),
            )
        }
        ("PUT", path)
            if path_id_with_suffix(path, "/v1/configuration/companion-apps/", "/schema")
                .is_some() =>
        {
            let app_id = path_id_with_suffix(path, "/v1/configuration/companion-apps/", "/schema")
                .expect("checked");
            if !is_safe_companion_app_id(app_id) {
                return Some(invalid_request("invalid companion appId"));
            }
            let input = match serde_json::from_str::<RegisterCompanionSchemaRequest>(&request.body)
            {
                Ok(input) => input,
                Err(e) => return Some(invalid_request(e.to_string())),
            };
            if input.display_name.trim().is_empty() || !input.default_config.is_object() {
                return Some(invalid_request(
                    "displayName is required and defaultConfig must be a JSON object",
                ));
            }
            let _guard = companion_lock()
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let existing = match read_doc(config_root, app_id) {
                Ok(doc) => doc,
                Err(error) => return Some(Err(error)),
            };
            let doc = match existing {
                Some(mut prev) => {
                    prev.display_name = input.display_name.trim().to_owned();
                    prev.schema_version = input.schema_version;
                    prev.sections = input.sections;
                    prev.default_config = input.default_config;
                    prev.updated_at_ms = unix_time_millis();
                    prev
                }
                None => CompanionAppConfigDocument {
                    app_id: app_id.to_owned(),
                    display_name: input.display_name.trim().to_owned(),
                    schema_version: input.schema_version,
                    revision: 1,
                    updated_at_ms: unix_time_millis(),
                    sections: input.sections,
                    config: input.default_config.clone(),
                    default_config: input.default_config,
                },
            };
            if let Err(error) = write_doc(config_root, &doc) {
                return Some(Err(error));
            }
            Some(
                serde_json::to_string(&json!({ "document": doc }))
                    .map(|s| (200, s))
                    .map_err(Into::into),
            )
        }
        ("GET", path) if path_id(path, "/v1/configuration/companion-apps/").is_some() => {
            let app_id = path_id(path, "/v1/configuration/companion-apps/").expect("checked");
            if !is_safe_companion_app_id(app_id) {
                return Some(invalid_request("invalid companion appId"));
            }
            let _guard = companion_lock()
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            match read_doc(config_root, app_id) {
                Ok(Some(doc)) => Some(
                    serde_json::to_string(&json!({ "document": doc }))
                        .map(|s| (200, s))
                        .map_err(Into::into),
                ),
                Ok(None) => Some(structured_error(
                    404,
                    json!({ "code": "companion_app_not_found" }),
                )),
                Err(error) => Some(Err(error)),
            }
        }
        ("PUT", path) if path_id(path, "/v1/configuration/companion-apps/").is_some() => {
            let app_id = path_id(path, "/v1/configuration/companion-apps/").expect("checked");
            if !is_safe_companion_app_id(app_id) {
                return Some(invalid_request("invalid companion appId"));
            }
            let input = match serde_json::from_str::<UpdateCompanionConfigRequest>(&request.body) {
                Ok(input) => input,
                Err(e) => return Some(invalid_request(e.to_string())),
            };
            if !input.config.is_object() {
                return Some(invalid_request("config must be a JSON object"));
            }
            let _guard = companion_lock()
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let Some(mut doc) = (match read_doc(config_root, app_id) {
                Ok(doc) => doc,
                Err(error) => return Some(Err(error)),
            }) else {
                return Some(structured_error(
                    404,
                    json!({ "code": "companion_app_not_found" }),
                ));
            };
            if doc.revision != input.expected_revision {
                return Some(structured_error(
                    409,
                    json!({
                        "code": "revision_conflict",
                        "message": "companion configuration was updated by another writer",
                        "currentRevision": doc.revision,
                    }),
                ));
            }
            doc.revision += 1;
            doc.config = input.config;
            doc.updated_at_ms = unix_time_millis();
            if let Err(error) = write_doc(config_root, &doc) {
                return Some(Err(error));
            }
            Some(
                serde_json::to_string(&json!({ "document": doc }))
                    .map(|s| (200, s))
                    .map_err(Into::into),
            )
        }
        _ => None,
    }
}
