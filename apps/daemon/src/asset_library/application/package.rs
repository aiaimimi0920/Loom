//! 应用包只验证封装和启动入口，不运行程序、不执行安装脚本。
use anyhow::{ensure, Result};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs,
    io::{Cursor, Read, Seek, SeekFrom},
    path::Path,
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    schema: String,
    kind: String,
    files: Vec<File>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    path: String,
    size: u64,
    sha256: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Application {
    pub schema_version: u32,
    pub version: String,
    pub platform: String,
    pub entry: String,
}

pub(super) fn unpack(bytes: &[u8], stage: &Path, label: &str) -> Result<Application> {
    ensure!(bytes.len() <= 8 * 1024 * 1024, "application_package_budget");
    validate_archive(Cursor::new(bytes), false)?;
    let extracted = loom_security::archive::extract_zip_securely(bytes, stage)?;
    validate_extracted(extracted, stage, label, false)
}

pub(super) fn unpack_file(mut file: &fs::File, stage: &Path, label: &str) -> Result<Application> {
    ensure!(
        file.metadata()?.len() <= 96_000_000,
        "application_package_budget"
    );
    file.seek(SeekFrom::Start(0))?;
    validate_archive(&mut file, true)?;
    let extracted = loom_security::archive::extract_application_zip_file(file, stage)?;
    validate_extracted(extracted, stage, label, true)
}

fn validate_archive(reader: impl Read + Seek, large: bool) -> Result<()> {
    let mut zip = zip::ZipArchive::new(reader)?;
    ensure!(
        (2..=if large { 65 } else { 33 }).contains(&zip.len()),
        "application_file_count"
    );
    let mut total = 0;
    for index in 0..zip.len() {
        let entry = zip.by_index(index)?;
        let name = entry.name()?;
        ensure!(
            entry.size() > 0
                && entry.size()
                    <= if large {
                        128 * 1024 * 1024
                    } else {
                        1024 * 1024
                    },
            "application_file_budget"
        );
        ensure!(
            !name.contains('\\') && name.is_ascii(),
            "application_path_invalid"
        );
        if name == "manifest.json" || name == "application.json" {
            ensure!(entry.size() <= 32768, "application_manifest_budget");
        }
        total += entry.size();
        ensure!(
            total
                <= if large {
                    256 * 1024 * 1024
                } else {
                    8 * 1024 * 1024
                },
            "application_package_budget"
        );
    }
    Ok(())
}

pub(super) fn digest_file(mut file: &fs::File) -> Result<String> {
    file.seek(SeekFrom::Start(0))?;
    let mut digest = Sha256::new();
    let mut buffer = [0u8; 65536];
    let mut total = 0u64;
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        total += count as u64;
        ensure!(total <= 256 * 1024 * 1024, "application_file_budget");
        digest.update(&buffer[..count]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn validate_extracted(
    extracted: Vec<String>,
    stage: &Path,
    label: &str,
    large: bool,
) -> Result<Application> {
    let manifest: Manifest = serde_json::from_slice(&fs::read(stage.join("manifest.json"))?)?;
    ensure!(
        manifest.schema == "neuro-software-package-v1" && manifest.kind == "application",
        "application_manifest_invalid"
    );
    ensure!(
        (1..=if large { 64 } else { 32 }).contains(&manifest.files.len()),
        "application_file_count"
    );
    let actual: BTreeSet<_> = extracted.into_iter().collect();
    let mut expected = BTreeSet::from(["manifest.json".to_owned()]);
    for file in &manifest.files {
        ensure!(
            actual.contains(&file.path) && expected.insert(file.path.clone()),
            "application_file_binding"
        );
        let payload = fs::File::open(stage.join(&file.path))?;
        ensure!(
            payload.metadata()?.len() == file.size && digest_file(&payload)? == file.sha256,
            "application_file_digest"
        );
    }
    ensure!(actual == expected, "application_unlisted_file");
    let app: Application = serde_json::from_slice(&fs::read(stage.join("application.json"))?)?;
    ensure!(
        app.schema_version == 1 && app.version == label && !label.is_empty(),
        "application_version_binding"
    );
    ensure!(
        cfg!(all(windows, target_arch = "x86_64")) && app.platform == "windows-x86_64",
        "application_platform_unsupported"
    );
    ensure!(
        app.entry != "manifest.json" && actual.contains(&app.entry) && app.entry.ends_with(".exe"),
        "application_entry_invalid"
    );
    let mut magic = [0u8; 2];
    fs::File::open(stage.join(&app.entry))?.read_exact(&mut magic)?;
    ensure!(&magic == b"MZ", "application_entry_invalid");
    Ok(app)
}
