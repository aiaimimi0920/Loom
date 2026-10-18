//! 安装只接受发布 ID 和用户所选摘要，绝不接受客户端提供的本机文件路径。
use super::{download, error, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Seek, SeekFrom},
    path::Path,
};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Input {
    publication_id: String,
    kind: String,
    sha256: String,
    #[serde(default)]
    task_id: Option<String>,
}

pub(crate) struct InstallPackage {
    pub(crate) kind: String,
    pub(crate) bytes: Vec<u8>,
    pub(crate) archive: Option<fs::File>,
    pub(crate) download: Value,
}

pub(crate) fn prepare_install(root: &Path, body: &str) -> Result<InstallPackage> {
    let input = parse_input(body)?;
    // 重新取票并下载，沿用当前授权、撤销、大小和摘要校验，不把旧缓存视为新授权。
    let receipt = download::handle(
        root,
        &json!({"publicationId":input.publication_id, "taskId":input.task_id}).to_string(),
    )?;
    checked_package(&input, receipt)
}

fn parse_input(body: &str) -> Result<Input> {
    if body.len() > 4096 {
        return Err(error(413, "asset_library_request_budget"));
    }
    let input: Input =
        serde_json::from_str(body).map_err(|_| error(400, "asset_library_request_invalid"))?;
    if !matches!(
        input.kind.as_str(),
        "art" | "capability" | "mcp" | "application"
    ) || input.sha256.len() != 64
        || !input
            .sha256
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
    {
        return Err(error(400, "asset_library_install_selection_invalid"));
    }
    Ok(input)
}

fn checked_package(input: &Input, receipt: Value) -> Result<InstallPackage> {
    let policy_matches = if input.kind == "application" {
        receipt["policy"] == "application-zip-format-v1"
            || receipt["policy"] == "application-large-zip-format-v1"
    } else {
        receipt["policy"] == format!("loom-{}-zip-format-v1", input.kind)
            || (input.kind != "mcp"
                && receipt["policy"] == format!("loom-{}-zip-clamav-v1", input.kind))
    };
    if receipt["publicationId"] != input.publication_id
        || receipt["kind"] != input.kind
        || receipt["sha256"] != input.sha256
        || !policy_matches
    {
        return Err(error(409, "asset_library_install_selection_changed"));
    }
    let invalid = || error(409, "asset_library_install_file_changed");
    let large =
        input.kind == "application" && receipt["policy"] == "application-large-zip-format-v1";
    let limit = if large {
        download::LARGE_MAX_BYTES
    } else {
        8 * 1024 * 1024
    };
    let size = receipt["size"]
        .as_u64()
        .filter(|n| *n > 0 && *n <= limit)
        .ok_or_else(invalid)?;
    // 路径仅来自本进程下载回执；读取后再次验摘要，防止保存与安装间内容被改写。
    let path = receipt["path"].as_str().ok_or_else(invalid)?;
    let metadata = fs::symlink_metadata(path).map_err(|_| invalid())?;
    if !metadata.is_file() || loom_security::metadata_has_link_semantics(&metadata) {
        return Err(invalid());
    }
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // 持有句柄直到安装结束；Windows 拒绝其他句柄写入和删除该下载文件。
        options.share_mode(1);
    }
    let mut file = options.open(path).map_err(|_| invalid())?;
    let opened = file.metadata().map_err(|_| invalid())?;
    if !opened.is_file()
        || loom_security::metadata_has_link_semantics(&opened)
        || opened.len() != size
    {
        return Err(invalid());
    }
    if large {
        let mut hash = Sha256::new();
        let mut total = 0u64;
        let mut buffer = [0u8; 65536];
        loop {
            let count = file.read(&mut buffer).map_err(|_| invalid())?;
            if count == 0 {
                break;
            }
            total += count as u64;
            if total > size {
                return Err(invalid());
            }
            hash.update(&buffer[..count]);
        }
        if total != size || format!("{:x}", hash.finalize()) != input.sha256 {
            return Err(invalid());
        }
        file.seek(SeekFrom::Start(0)).map_err(|_| invalid())?;
        return Ok(InstallPackage {
            kind: input.kind.clone(),
            bytes: Vec::new(),
            archive: Some(file),
            download: receipt,
        });
    }
    let mut bytes = Vec::with_capacity(size as usize);
    file.take(size + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| invalid())?;
    if bytes.len() as u64 != size || format!("{:x}", Sha256::digest(&bytes)) != input.sha256 {
        return Err(invalid());
    }
    Ok(InstallPackage {
        kind: input.kind.clone(),
        bytes,
        archive: None,
        download: receipt,
    })
}

#[cfg(test)]
#[path = "install_tests.rs"]
mod tests;

/// 仅测试二进制允许回环 trial；生产服务地址、身份和门禁不可配置为此路径。
#[cfg(test)]
pub(crate) fn prepare_trial_install(config: &Value) -> Result<InstallPackage> {
    assert_eq!(config["mode"], "local-trial-only");
    let origin = config["origin"].as_str().unwrap();
    let url = reqwest::Url::parse(origin).unwrap();
    assert_eq!(url.scheme(), "http");
    assert_eq!(url.host_str(), Some("127.0.0.1"));
    let input = parse_input(
        &json!({"publicationId":config["publicationId"],
        "kind":config["kind"], "sha256":config["sha256"]})
        .to_string(),
    )?;
    let client = reqwest::blocking::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .unwrap();
    let receipt = download::download(
        Path::new(config["downloadRoot"].as_str().unwrap()),
        &input.publication_id,
        client,
        config["token"].as_str().unwrap().into(),
        origin,
    )?;
    checked_package(&input, receipt)
}
