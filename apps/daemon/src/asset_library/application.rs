//! 便携应用按资源 UUID 隔离，原子切换活动记录，更新不覆盖或删除旧版程序。
mod package;
use super::InstallPackage;
use anyhow::{ensure, Result};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
};

static INSTALL_LOCK: Mutex<()> = Mutex::new(());
struct Stage(PathBuf);
impl Drop for Stage {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn directory(root: &Path, parts: &[&str]) -> Result<PathBuf> {
    let mut path = root.to_path_buf();
    for part in std::iter::once("").chain(parts.iter().copied()) {
        if !part.is_empty() {
            path.push(part);
        }
        match fs::create_dir(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.into()),
        }
        let meta = fs::symlink_metadata(&path)?;
        ensure!(
            meta.is_dir() && !loom_security::metadata_has_link_semantics(&meta),
            "application_directory_invalid"
        );
    }
    Ok(path)
}

pub(crate) fn install(root: &Path, package: &InstallPackage) -> Result<Value> {
    let _guard = INSTALL_LOCK
        .lock()
        .map_err(|_| anyhow::anyhow!("application_install_lock"))?;
    ensure!(package.kind == "application", "application_kind_invalid");
    let resource = package.download["resourceId"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("application_resource_required"))?;
    let id = uuid::Uuid::parse_str(resource)?;
    ensure!(
        id.get_version_num() == 4 && id.to_string() == resource,
        "application_resource_invalid"
    );
    let label = package.download["label"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("application_version_required"))?;
    let digest = match &package.archive {
        Some(file) => package::digest_file(file)?,
        None => format!("{:x}", Sha256::digest(&package.bytes)),
    };
    ensure!(
        package.download["sha256"] == digest,
        "application_digest_changed"
    );
    // 当前软件封装未定义签名；必须遵守宿主策略，绝不伪装已签名或受信任。
    loom_plugin_security::TrustStore::load(&root.join("plugin-trust.json"))?
        .effective_policy()
        .enforce(loom_protocol::PackageTrustStatus::Unsigned)?;
    let app_root = directory(root, &["applications", resource])?;
    let versions = directory(&app_root, &["versions"])?;
    let stage_path = app_root.join(format!("stage-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&stage_path)?;
    let stage = Stage(stage_path);
    let app = match &package.archive {
        Some(file) => {
            ensure!(
                package.download["policy"] == "application-large-zip-format-v1",
                "application_policy_invalid"
            );
            package::unpack_file(file, &stage.0, label)?
        }
        None => package::unpack(&package.bytes, &stage.0, label)?,
    };
    // 每次安装独占目录，避免把曾被本地修改的历史文件当成新下载的可信字节。
    let target = versions.join(uuid::Uuid::new_v4().simple().to_string());
    fs::rename(&stage.0, &target)?;
    let state = json!({"resourceId":resource, "publicationId":package.download["publicationId"],
        "version":app.version, "sha256":digest, "directory":target,
        "entry":target.join(app.entry), "trustStatus":"unsigned", "scanStatus":"not_scanned"});
    let active = app_root.join("active.json");
    if let Ok(meta) = fs::symlink_metadata(&active) {
        ensure!(
            meta.is_file() && !loom_security::metadata_has_link_semantics(&meta),
            "application_active_invalid"
        );
    }
    crate::write_json_atomically(&active, &state)?;
    Ok(state)
}
