//! 仅下载已获分发资格的包；不解压、安装或执行载荷。
use super::{client, error, json_response, Result, ORIGIN};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

const MAX_BYTES: u64 = 8 * 1024 * 1024;
pub(super) const LARGE_MAX_BYTES: u64 = 96_000_000;

#[cfg(test)]
#[path = "worker_integration.rs"]
mod worker_integration;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Input {
    publication_id: String,
    #[serde(default)]
    task_id: Option<String>,
}

struct PartialFile(PathBuf);
impl Drop for PartialFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn invalid() -> super::super::naccount::Error {
    error(502, "asset_library_download_invalid")
}

fn metadata(value: &Value, id: &str) -> Result<(u64, String)> {
    let limit =
        if value["kind"] == "application" && value["policy"] == "application-large-zip-format-v1" {
            LARGE_MAX_BYTES
        } else {
            MAX_BYTES
        };
    let size = value["size"]
        .as_u64()
        .filter(|n| *n > 0 && *n <= limit)
        .ok_or_else(invalid)?;
    let sha = value["sha256"]
        .as_str()
        .filter(|s| {
            s.len() == 64
                && s.bytes()
                    .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
        })
        .ok_or_else(invalid)?;
    if value["id"] != id {
        return Err(invalid());
    }
    Ok((size, sha.into()))
}

fn checked_directory(root: &Path) -> Result<PathBuf> {
    let dir = root.join("asset-library-downloads");
    fs::create_dir_all(&dir).map_err(|_| error(500, "asset_library_save_failed"))?;
    let meta = fs::symlink_metadata(&dir).map_err(|_| error(500, "asset_library_save_failed"))?;
    let mut linked = meta.file_type().is_symlink();
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        linked |= meta.file_attributes() & 0x400 != 0;
    }
    if !meta.is_dir() || linked {
        return Err(error(500, "asset_library_save_failed"));
    }
    Ok(dir)
}

pub(super) fn save(
    mut reader: impl Read,
    dir: &Path,
    size: u64,
    expected_sha: &str,
) -> Result<PathBuf> {
    let name = uuid::Uuid::new_v4();
    let partial_path = dir.join(format!("{name}.partial"));
    let destination = dir.join(format!("{name}.zip"));
    let partial;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&partial_path)
        .map_err(|_| error(500, "asset_library_save_failed"))?;
    partial = PartialFile(partial_path);
    {
        let mut hash = Sha256::new();
        let mut total = 0u64;
        let mut buffer = [0u8; 64 * 1024];
        loop {
            let count = reader
                .read(&mut buffer)
                .map_err(|_| error(503, "asset_library_download_interrupted"))?;
            if count == 0 {
                break;
            }
            total += count as u64;
            if total > size || total > LARGE_MAX_BYTES {
                return Err(invalid());
            }
            hash.update(&buffer[..count]);
            file.write_all(&buffer[..count])
                .map_err(|_| error(500, "asset_library_save_failed"))?;
        }
        if total != size || format!("{:x}", hash.finalize()) != expected_sha {
            return Err(invalid());
        }
        file.sync_all()
            .map_err(|_| error(500, "asset_library_save_failed"))?;
    }
    drop(file);
    // hard_link 不覆盖既有文件；只有完成摘要校验的包才获得最终文件名。
    fs::hard_link(&partial.0, &destination).map_err(|_| error(500, "asset_library_save_failed"))?;
    Ok(destination)
}

pub(super) fn handle(root: &Path, body: &str) -> Result<Value> {
    let input: Input =
        serde_json::from_str(body).map_err(|_| error(400, "asset_library_request_invalid"))?;
    let id = uuid::Uuid::parse_str(&input.publication_id)
        .map_err(|_| error(400, "asset_library_request_invalid"))?;
    if id.get_version_num() != 4 || id.to_string() != input.publication_id {
        return Err(error(400, "asset_library_request_invalid"));
    }
    let client = client()?;
    let task_id = input
        .task_id
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let (token, registration) = crate::naccount::with_access_token(root, |token| {
        let registration = super::download_control::register(root, &task_id, &token)?;
        Ok((token, registration))
    })?;
    download_controlled(
        root,
        &input.publication_id,
        client,
        token,
        ORIGIN,
        Some(registration.state.clone()),
    )
}

#[cfg(test)]
pub(super) fn download(
    root: &Path,
    publication_id: &str,
    client: reqwest::blocking::Client,
    token: String,
    origin: &str,
) -> Result<Value> {
    download_controlled(root, publication_id, client, token, origin, None)
}

fn download_controlled(
    root: &Path,
    publication_id: &str,
    client: reqwest::blocking::Client,
    token: String,
    origin: &str,
    state: Option<std::sync::Arc<super::aria2_native::State>>,
) -> Result<Value> {
    let id = publication_id;
    let publication = json_response(
        client
            .get(format!("{origin}/v1/catalog/{id}"))
            .bearer_auth(&token),
    )?;
    let (size, sha) = metadata(&publication, publication_id)?;
    let ticket = json_response(
        client
            .post(format!("{origin}/v1/publications/{id}/tickets"))
            .bearer_auth(&token)
            .json(&json!({})),
    )?;
    let path = format!("/v1/publications/{id}/content");
    let credential = ticket["ticket"]
        .as_str()
        .filter(|s| s.len() == 64 && s.bytes().all(|c| c.is_ascii_hexdigit()))
        .ok_or_else(invalid)?;
    if ticket["publicationId"] != publication_id || ticket["contentPath"] != path {
        return Err(invalid());
    }
    let directory = checked_directory(root)?;
    let use_aria2 = !cfg!(test) || std::env::var_os("LOOM_TEST_LIBARIA2").is_some();
    let path = if use_aria2 {
        super::aria2::download(
            client.clone(),
            &directory,
            &format!("{origin}{path}"),
            &token,
            credential,
            size,
            &sha,
            state.clone(),
        )?
    } else {
        let response = client
            .get(format!("{origin}{path}"))
            .bearer_auth(token)
            .header("X-Download-Ticket", credential)
            .timeout(std::time::Duration::from_secs(60))
            .send()
            .map_err(|_| error(503, "asset_library_unavailable"))?;
        if response.status().as_u16() != 200 {
            return Err(error(
                response.status().as_u16(),
                "asset_library_download_rejected",
            ));
        }
        if response
            .headers()
            .get("content-type")
            .and_then(|h| h.to_str().ok())
            != Some("application/zip")
            || response.content_length().is_some_and(|n| n != size)
        {
            return Err(invalid());
        }
        save(response, &directory, size, &sha)?
    };
    if state
        .as_ref()
        .is_some_and(|state| !super::download_control::finish(state))
    {
        let _ = fs::remove_file(&path);
        return Err(error(409, "asset_library_download_cancelled"));
    }
    Ok(
        json!({"publicationId":id.to_string(), "path":path, "size":size, "sha256":sha,
            "downloadEngine":if use_aria2 { "libaria2" } else { "test-http" },
            "kind":publication["kind"], "policy":publication["policy"],
            "resourceId":publication["resourceId"], "label":publication["label"]}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn download_http_sequence_uses_private_ticket_and_verified_destination() {
        use std::{net::TcpListener, thread, time::Duration};
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let id = uuid::Uuid::new_v4().to_string();
        let server_id = id.clone();
        let bytes = b"verified archive fixture";
        let sha = format!("{:x}", Sha256::digest(bytes));
        let server = thread::spawn(move || {
            let ticket = "a".repeat(64);
            for step in 0..3 {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut request = Vec::new();
                let mut one = [0u8];
                while !request.ends_with(b"\r\n\r\n") {
                    assert!(request.len() < 8192);
                    stream.read_exact(&mut one).unwrap();
                    request.push(one[0]);
                }
                let request = String::from_utf8(request).unwrap().to_lowercase();
                assert!(request.contains("authorization: bearer fixture-access\r\n"));
                let (body, content_type) = match step {
                    0 => {
                        assert!(request.starts_with(&format!("get /v1/catalog/{server_id} ")));
                        (
                            json!({"id":server_id, "size":bytes.len(), "sha256":sha}).to_string(),
                            "application/json",
                        )
                    }
                    1 => {
                        assert!(request
                            .starts_with(&format!("post /v1/publications/{server_id}/tickets ")));
                        let mut body = [0; 2];
                        stream.read_exact(&mut body).unwrap();
                        assert_eq!(&body, b"{}");
                        (
                            json!({"publicationId":server_id, "ticket":ticket,
                            "contentPath":format!("/v1/publications/{server_id}/content")})
                            .to_string(),
                            "application/json",
                        )
                    }
                    _ => {
                        assert!(request
                            .starts_with(&format!("get /v1/publications/{server_id}/content ")));
                        assert!(request.contains(&format!("x-download-ticket: {ticket}\r\n")));
                        (
                            String::from_utf8(bytes.to_vec()).unwrap(),
                            "application/zip",
                        )
                    }
                };
                write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
            }
        });
        let root =
            std::env::temp_dir().join(format!("loom-cloud-protocol-{}", uuid::Uuid::new_v4()));
        let client = reqwest::blocking::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap();
        let result = download(&root, &id, client, "fixture-access".into(), &origin).unwrap();
        server.join().unwrap();
        assert_eq!(result["publicationId"], id);
        let path = PathBuf::from(result["path"].as_str().unwrap());
        assert_eq!(fs::read(&path).unwrap(), bytes);
        assert!(!result.to_string().contains("fixture-access"));
        fs::remove_file(path).unwrap();
        fs::remove_dir(root.join("asset-library-downloads")).unwrap();
        fs::remove_dir(root).unwrap();
    }
    #[test]
    fn download_metadata_is_bounded_and_bound_to_publication() {
        let value = json!({"id":"one", "size":3, "sha256":"a".repeat(64)});
        assert!(metadata(&value, "one").is_ok());
        assert!(metadata(&value, "two").is_err());
        for size in [0, MAX_BYTES + 1] {
            assert!(metadata(
                &json!({"id":"one", "size":size, "sha256":"a".repeat(64)}),
                "one"
            )
            .is_err());
        }
    }
    #[test]
    fn download_large_budget_requires_application_and_exact_policy() {
        let mut value = json!({"id":"one", "kind":"application",
            "policy":"application-large-zip-format-v1", "size":LARGE_MAX_BYTES,
            "sha256":"a".repeat(64)});
        assert!(metadata(&value, "one").is_ok());
        value["size"] = json!(LARGE_MAX_BYTES + 1);
        assert!(metadata(&value, "one").is_err());
        value["size"] = json!(MAX_BYTES + 1);
        for kind in ["art", "capability", "mcp"] {
            value["kind"] = json!(kind);
            assert!(metadata(&value, "one").is_err());
        }
        value["kind"] = json!("application");
        value["policy"] = json!("application-zip-format-v1");
        assert!(metadata(&value, "one").is_err());
    }
    #[test]
    fn download_only_publishes_verified_bytes_and_cleans_partial_files() {
        let dir =
            std::env::temp_dir().join(format!("loom-cloud-download-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&dir).unwrap();
        let bytes = b"zip fixture";
        let sha = format!("{:x}", Sha256::digest(bytes));
        assert!(save(&bytes[..], &dir, 1, &sha).is_err());
        assert!(save(&bytes[..], &dir, bytes.len() as u64 + 1, &sha).is_err());
        assert!(save(&bytes[..], &dir, bytes.len() as u64, &"0".repeat(64)).is_err());
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 0);
        let path = save(&bytes[..], &dir, bytes.len() as u64, &sha).unwrap();
        assert_eq!(fs::read(&path).unwrap(), bytes);
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 1);
        fs::remove_file(path).unwrap();
        fs::remove_dir(dir).unwrap();
    }
}
