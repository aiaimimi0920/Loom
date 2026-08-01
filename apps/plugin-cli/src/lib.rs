use std::fs::{self, File};
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, bail, Context, Result};
use loom_plugin_security::{
    generate_signing_key, read_signing_key, sign_package, verify_package_signature,
    write_signing_key, TrustStore,
};
use loom_protocol::{
    is_safe_package_id, is_safe_publisher_id, schemas, validate_framework_manifest_contract,
    ArtRuntimeManifest, FrameworkExecuteRequest, FrameworkExecuteResponse,
    FrameworkExecutionContext, FrameworkPackageManifest, PackageSignature, PackageTrustStatus,
    PublisherIdentity, PublisherTrustRecord, ART_RUNTIME_PROTOCOL_VERSION,
    FRAMEWORK_PROTOCOL_VERSION,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use zip::write::SimpleFileOptions;

#[cfg(test)]
use std::io::Read;

const MAX_PACKAGE_FILES: usize = 4096;
const MAX_PACKAGE_BYTES: u64 = 512 * 1024 * 1024;
const MAX_CONFORMANCE_OUTPUT_BYTES: usize = 8 * 1024 * 1024;
const CONFORMANCE_TIMEOUT: Duration = Duration::from_secs(30);
static CONFORMANCE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub fn run<I, S, W>(args: I, writer: &mut W) -> Result<()>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
    W: Write,
{
    let args = args
        .into_iter()
        .map(|value| value.as_ref().to_owned())
        .collect::<Vec<_>>();
    if args.len() <= 1 || has_flag(&args, "--help") || has_flag(&args, "-h") {
        writer.write_all(help_text().as_bytes())?;
        return Ok(());
    }
    if has_flag(&args, "--version") || has_flag(&args, "-V") {
        writeln!(writer, "loom-plugin {}", env!("CARGO_PKG_VERSION"))?;
        return Ok(());
    }

    match args[1..]
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["validate", path] => {
            let report = validate_path_with_trust_store(Path::new(path), None)?;
            writeln!(writer, "{report}")?;
        }
        ["validate", path, "--trust-store", store] => {
            let report = validate_path_with_trust_store(Path::new(path), Some(Path::new(store)))?;
            writeln!(writer, "{report}")?;
        }
        ["pack", source, output] => {
            let report = pack_directory(Path::new(source), Path::new(output))?;
            writeln!(writer, "{report}")?;
        }
        ["schema", name] => {
            writer.write_all(schema(name)?.as_bytes())?;
            writer.write_all(b"\n")?;
        }
        ["keygen", path, key_id] => {
            let key = generate_signing_key(*key_id);
            write_signing_key(Path::new(path), &key)?;
            writeln!(writer, "generated Ed25519 key `{key_id}` at {path}")?;
        }
        ["sign", directory, key_path, publisher_id] => {
            let status =
                sign_plugin_package(Path::new(directory), Path::new(key_path), publisher_id)?;
            writeln!(writer, "{status}")?;
        }
        ["trust", "add", store_path, publisher_id, key_path] => {
            trust_publisher(Path::new(store_path), publisher_id, Path::new(key_path))?;
            writeln!(writer, "trusted publisher `{publisher_id}`")?;
        }
        ["trust", "revoke", store_path, publisher_id, key_id] => {
            revoke_publisher(Path::new(store_path), publisher_id, key_id)?;
            writeln!(writer, "revoked publisher `{publisher_id}` key `{key_id}`")?;
        }
        ["init", "framework", directory, id] => {
            init_framework(Path::new(directory), id)?;
            writeln!(writer, "initialized framework `{id}` at {directory}")?;
        }
        ["init", "art", directory, id, framework] => {
            init_art(Path::new(directory), id, framework)?;
            writeln!(writer, "initialized Art `{id}` at {directory}")?;
        }
        ["conformance", executable, framework_id, art_dir] => {
            let report = run_conformance(Path::new(executable), framework_id, Path::new(art_dir))?;
            writeln!(writer, "{report}")?;
        }
        command => bail!(
            "unsupported command `{}`\n\n{}",
            command.join(" "),
            help_text()
        ),
    }
    Ok(())
}

fn has_flag(args: &[String], flag: &str) -> bool {
    args.iter().skip(1).any(|value| value == flag)
}

fn help_text() -> &'static str {
    concat!(
        "Usage: loom-plugin <COMMAND>\n",
        "\n",
        "Commands:\n",
        "  init framework <DIR> <ID>              Create a framework package skeleton\n",
        "  init art <DIR> <ID> <FRAMEWORK>         Create an Art package skeleton\n",
        "  validate <PATH> [--trust-store <STORE>] Validate a package directory or manifest\n",
        "  pack <SOURCE_DIR> <OUTPUT_ZIP>          Validate and build a deterministic package ZIP\n",
        "  conformance <EXE> <FRAMEWORK> <ART_DIR> Run the v1 process contract against a runtime\n",
        "  schema <NAME>                           Print an embedded public JSON Schema\n",
        "  keygen <KEY_FILE> <KEY_ID>              Generate an Ed25519 signing key\n",
        "  sign <PACKAGE_DIR> <KEY_FILE> <PUBLISHER> Sign a framework or Art package\n",
        "  trust add <STORE> <PUBLISHER> <KEY_FILE> Trust a publisher key\n",
        "  trust revoke <STORE> <PUBLISHER> <KEY_ID> Revoke a publisher key\n",
        "\n",
        "Schema names: framework-manifest, execute-request, execute-response, authoring, art-runtime\n",
    )
}

fn schema(name: &str) -> Result<&'static str> {
    match name {
        "framework-manifest" => Ok(schemas::FRAMEWORK_MANIFEST_V1),
        "execute-request" => Ok(schemas::FRAMEWORK_EXECUTE_REQUEST_V1),
        "execute-response" => Ok(schemas::FRAMEWORK_EXECUTE_RESPONSE_V1),
        "authoring" => Ok(schemas::FRAMEWORK_AUTHORING_V1),
        "art-runtime" => Ok(schemas::ART_RUNTIME_V1),
        _ => bail!("unknown schema `{name}`"),
    }
}

fn validate_path_with_trust_store(path: &Path, trust_store_path: Option<&Path>) -> Result<String> {
    let trust_store = trust_store_path
        .map(TrustStore::load)
        .transpose()?
        .unwrap_or_default();
    validate_path_with_payload(path, false, &trust_store)
}

fn validate_path_with_payload(
    path: &Path,
    require_payload: bool,
    trust_store: &TrustStore,
) -> Result<String> {
    if path.is_dir() {
        if path.join("framework.manifest.json").is_file() {
            return validate_framework_package(path, require_payload, trust_store);
        }
        if path.join("manifest.json").is_file() || path.join("art.runtime.json").is_file() {
            return validate_art_package(path, require_payload, trust_store);
        }
        bail!("directory contains neither a framework nor an Art package manifest");
    }
    match path.file_name().and_then(|value| value.to_str()) {
        Some("framework.manifest.json") => validate_framework_package(
            path.parent()
                .ok_or_else(|| anyhow!("framework manifest has no parent directory"))?,
            require_payload,
            trust_store,
        ),
        Some("art.runtime.json") | Some("manifest.json") => validate_art_package(
            path.parent()
                .ok_or_else(|| anyhow!("Art manifest has no parent directory"))?,
            require_payload,
            trust_store,
        ),
        _ => bail!("unsupported manifest path: {}", path.display()),
    }
}

fn validate_framework_package(
    directory: &Path,
    require_payload: bool,
    trust_store: &TrustStore,
) -> Result<String> {
    let path = directory.join("framework.manifest.json");
    let manifest: FrameworkPackageManifest = read_json(&path)?;
    validate_framework_manifest_contract(&manifest).map_err(|error| anyhow!(error))?;
    validate_relative_package_path(directory, &manifest.entry.command, require_payload)
        .context("validate framework entry")?;
    if manifest.entry.kind != "process" {
        bail!("framework entry.kind must be `process`");
    }
    if let Some(signature) = &manifest.signature {
        validate_relative_package_path(directory, &signature.file, true)
            .context("validate signature file")?;
        let publisher_key = manifest
            .publisher
            .as_ref()
            .and_then(|publisher| publisher.key_id.as_deref());
        if publisher_key != Some(signature.key_id.as_str()) {
            bail!("signature keyId must match publisher.keyId");
        }
    }
    let trust = verify_package_signature(
        directory,
        manifest.publisher.as_ref(),
        manifest.signature.as_ref(),
        trust_store,
    )?;
    reject_revoked_package(&trust)?;
    Ok(format!(
        "framework package valid: {} {} ({}, trust={trust:?})",
        manifest.qualified_id(),
        manifest.version,
        FRAMEWORK_PROTOCOL_VERSION
    ))
}

fn validate_art_package(
    directory: &Path,
    require_payload: bool,
    trust_store: &TrustStore,
) -> Result<String> {
    let runtime_path = directory.join("art.runtime.json");
    let runtime: ArtRuntimeManifest = read_json(&runtime_path)?;
    if runtime.protocol_version != ART_RUNTIME_PROTOCOL_VERSION {
        bail!(
            "unsupported Art runtime protocol: {}",
            runtime.protocol_version
        );
    }
    validate_relative_package_path(directory, &runtime.entry.command, require_payload)
        .context("validate Art runtime entry")?;

    let manifest_path = directory.join("manifest.json");
    let manifest: Value = read_json(&manifest_path)?;
    let id = manifest
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("Art manifest id is required"))?;
    if !is_safe_package_id(id) {
        bail!("Art manifest id is not a safe package id: {id}");
    }
    let execution = manifest
        .get("execution")
        .and_then(Value::as_object)
        .ok_or_else(|| anyhow!("Art manifest execution is required"))?;
    if execution.get("type").and_then(Value::as_str) != Some("framework_art") {
        bail!("Art package execution.type must be `framework_art`");
    }
    let framework = execution
        .get("framework")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("Art package execution.framework is required"))?;
    if !is_safe_package_reference(framework) {
        bail!("Art framework id is not safe: {framework}");
    }
    let (publisher, signature) = art_security_metadata(&manifest)?;
    let trust = verify_package_signature(
        directory,
        publisher.as_ref(),
        signature.as_ref(),
        trust_store,
    )?;
    reject_revoked_package(&trust)?;
    Ok(format!(
        "Art package valid: {id} -> {framework} (trust={trust:?})"
    ))
}

fn art_security_metadata(
    manifest: &Value,
) -> Result<(Option<PublisherIdentity>, Option<PackageSignature>)> {
    let security = manifest
        .get("metadata")
        .and_then(Value::as_object)
        .and_then(|metadata| metadata.get("packageSecurity"));
    let publisher = security
        .and_then(|security| security.get("publisher"))
        .map(|value| serde_json::from_value(value.clone()))
        .transpose()?;
    let signature = security
        .and_then(|security| security.get("signature"))
        .map(|value| serde_json::from_value(value.clone()))
        .transpose()?;
    Ok((publisher, signature))
}

fn reject_revoked_package(trust: &PackageTrustStatus) -> Result<()> {
    if *trust == PackageTrustStatus::Revoked {
        bail!("package signature belongs to a revoked publisher key");
    }
    Ok(())
}

fn sign_plugin_package(directory: &Path, key_path: &Path, publisher_id: &str) -> Result<String> {
    if !is_safe_publisher_id(publisher_id) {
        bail!("publisher id is not safe: {publisher_id}");
    }
    let key = read_signing_key(key_path)?;
    let signature = json!({
        "algorithm": "ed25519",
        "keyId": key.key_id.clone(),
        "file": "signature.json"
    });
    if directory.join("framework.manifest.json").is_file() {
        let path = directory.join("framework.manifest.json");
        let mut manifest: Value = read_json(&path)?;
        let object = manifest
            .as_object_mut()
            .ok_or_else(|| anyhow!("framework manifest must be an object"))?;
        object.insert(
            "publisher".to_owned(),
            json!({ "id": publisher_id, "keyId": key.key_id.clone() }),
        );
        object.insert("signature".to_owned(), signature);
        write_pretty_json(path, &manifest)?;
    } else if directory.join("manifest.json").is_file() {
        let path = directory.join("manifest.json");
        let mut manifest: Value = read_json(&path)?;
        let object = manifest
            .as_object_mut()
            .ok_or_else(|| anyhow!("Art manifest must be an object"))?;
        let metadata = object
            .entry("metadata".to_owned())
            .or_insert_with(|| json!({}));
        let metadata = metadata
            .as_object_mut()
            .ok_or_else(|| anyhow!("Art metadata must be an object"))?;
        metadata.insert(
            "packageSecurity".to_owned(),
            json!({
                "publisher": { "id": publisher_id, "keyId": key.key_id.clone() },
                "signature": signature
            }),
        );
        write_pretty_json(path, &manifest)?;
    } else {
        bail!("package directory has no framework.manifest.json or manifest.json");
    }
    let document = sign_package(directory, "signature.json", &key)?;
    Ok(format!(
        "package signed: publisher={publisher_id}, keyId={}, digest={}",
        document.key_id, document.digest
    ))
}

fn trust_publisher(store_path: &Path, publisher_id: &str, key_path: &Path) -> Result<()> {
    if !is_safe_publisher_id(publisher_id) {
        bail!("publisher id is not safe: {publisher_id}");
    }
    let key = read_signing_key(key_path)?;
    let mut store = TrustStore::load(store_path)?;
    store.trust(PublisherTrustRecord {
        publisher_id: publisher_id.to_owned(),
        key_id: key.key_id,
        public_key: key.public_key,
        revoked: false,
    });
    store.write_atomic(store_path)?;
    Ok(())
}

fn revoke_publisher(store_path: &Path, publisher_id: &str, key_id: &str) -> Result<()> {
    if !is_safe_publisher_id(publisher_id) || key_id.trim().is_empty() {
        bail!("publisher id or key id is invalid");
    }
    let mut store = TrustStore::load(store_path)?;
    if !store.revoke(publisher_id, key_id) {
        bail!("publisher key was not found in trust store");
    }
    store.write_atomic(store_path)?;
    Ok(())
}

fn validate_relative_package_path(
    directory: &Path,
    value: &str,
    require_payload: bool,
) -> Result<()> {
    let path = Path::new(value);
    if value.trim().is_empty()
        || path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        bail!("path must remain inside the package: {value}");
    }
    if require_payload && !directory.join(path).is_file() {
        bail!("package path does not exist: {value}");
    }
    Ok(())
}

fn is_safe_package_reference(reference: &str) -> bool {
    reference
        .split_once('/')
        .map(|(publisher, id)| {
            is_safe_publisher_id(publisher) && !id.contains('/') && is_safe_package_id(id)
        })
        .unwrap_or_else(|| is_safe_package_id(reference))
}

fn read_json<T>(path: &Path) -> Result<T>
where
    T: serde::de::DeserializeOwned,
{
    let bytes = fs::read(path).with_context(|| format!("read {}", path.display()))?;
    serde_json::from_slice(&bytes).with_context(|| format!("parse {}", path.display()))
}

fn pack_directory(source: &Path, output: &Path) -> Result<String> {
    validate_path_with_payload(source, true, &TrustStore::default())?;
    let files = collect_package_files(source)?;
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    let file = File::create(output).with_context(|| format!("create {}", output.display()))?;
    let mut archive = zip::ZipWriter::new(file);
    let options = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o644);
    for path in &files {
        let relative = path.strip_prefix(source).expect("collected under source");
        let name = relative.to_string_lossy().replace('\\', "/");
        archive.start_file(&name, options)?;
        archive.write_all(&fs::read(path)?)?;
    }
    archive.finish()?.sync_all()?;
    let bytes = fs::read(output)?;
    let digest = hex_digest(&bytes);
    fs::write(
        output.with_extension(format!(
            "{}sha256",
            output
                .extension()
                .and_then(|value| value.to_str())
                .map(|value| format!("{value}."))
                .unwrap_or_default()
        )),
        format!(
            "{digest}  {}\n",
            output.file_name().unwrap_or_default().to_string_lossy()
        ),
    )?;
    Ok(format!(
        "package built: {} files, {} bytes, sha256={digest}",
        files.len(),
        bytes.len()
    ))
}

fn collect_package_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut pending = vec![root.to_path_buf()];
    let mut files = Vec::new();
    let mut total = 0u64;
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(&directory)
            .with_context(|| format!("read package directory {}", directory.display()))?
        {
            let entry = entry?;
            let file_type = entry.file_type()?;
            if file_type.is_symlink() {
                bail!(
                    "symbolic links are not allowed in plugin packages: {}",
                    entry.path().display()
                );
            }
            if file_type.is_dir() {
                pending.push(entry.path());
            } else if file_type.is_file() {
                total = total.saturating_add(entry.metadata()?.len());
                if total > MAX_PACKAGE_BYTES {
                    bail!("package exceeds {MAX_PACKAGE_BYTES} uncompressed bytes");
                }
                files.push(entry.path());
                if files.len() > MAX_PACKAGE_FILES {
                    bail!("package exceeds {MAX_PACKAGE_FILES} files");
                }
            }
        }
    }
    files.sort_by(|left, right| {
        left.to_string_lossy()
            .to_ascii_lowercase()
            .cmp(&right.to_string_lossy().to_ascii_lowercase())
    });
    for pair in files.windows(2) {
        let left = pair[0]
            .strip_prefix(root)?
            .to_string_lossy()
            .replace('\\', "/")
            .to_ascii_lowercase();
        let right = pair[1]
            .strip_prefix(root)?
            .to_string_lossy()
            .replace('\\', "/")
            .to_ascii_lowercase();
        if left == right {
            bail!("case-insensitive package path collision: {left}");
        }
    }
    Ok(files)
}

fn init_framework(directory: &Path, id: &str) -> Result<()> {
    if !is_safe_package_id(id) {
        bail!("framework id is not a safe package id: {id}");
    }
    ensure_empty_directory(directory)?;
    fs::create_dir_all(directory.join("runtime"))?;
    let manifest = json!({
        "id": id,
        "name": id,
        "description": "Third-party Loom framework",
        "version": "0.1.0",
        "protocolVersion": FRAMEWORK_PROTOCOL_VERSION,
        "supportedProtocolVersions": [FRAMEWORK_PROTOCOL_VERSION],
        "platforms": ["windows-x64"],
        "entry": {
            "kind": "process",
            "command": format!("runtime/{id}.exe"),
            "args": []
        },
        "permissionPolicy": {},
        "resources": {
            "timeoutSeconds": 120,
            "memoryMiB": 512,
            "maxProcesses": 4,
            "stdoutMiB": 8,
            "stderrMiB": 8
        },
        "artExecution": {
            "requestSchema": "loom.art.execute.v1",
            "responseSchema": "loom.art.result.v1"
        }
    });
    write_pretty_json(directory.join("framework.manifest.json"), &manifest)?;
    fs::write(
        directory.join("runtime").join("README.txt"),
        "Place the framework process entry declared by framework.manifest.json here.\n",
    )?;
    Ok(())
}

fn init_art(directory: &Path, id: &str, framework: &str) -> Result<()> {
    if !is_safe_package_id(id) || !is_safe_package_reference(framework) {
        bail!("Art and framework ids must be safe package ids");
    }
    ensure_empty_directory(directory)?;
    fs::create_dir_all(directory.join("runtime"))?;
    write_pretty_json(
        directory.join("manifest.json"),
        &json!({
            "id": id,
            "name": id,
            "description": "Third-party Loom Art",
            "enabled": true,
            "execution": { "type": "framework_art", "framework": framework },
            "inputs": [],
            "outputs": [],
            "params": [],
            "metadata": {
                "dependencies": { "framework": framework }
            }
        }),
    )?;
    write_pretty_json(
        directory.join("art.runtime.json"),
        &json!({
            "protocolVersion": ART_RUNTIME_PROTOCOL_VERSION,
            "entry": { "command": "runtime/main.exe", "args": [] }
        }),
    )?;
    fs::write(
        directory.join("runtime").join("README.txt"),
        "Place the Art runtime entry declared by art.runtime.json here.\n",
    )?;
    Ok(())
}

fn ensure_empty_directory(directory: &Path) -> Result<()> {
    if directory.exists() && fs::read_dir(directory)?.next().is_some() {
        bail!("directory is not empty: {}", directory.display());
    }
    fs::create_dir_all(directory)?;
    Ok(())
}

fn write_pretty_json(path: PathBuf, value: &Value) -> Result<()> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    fs::write(&path, bytes).with_context(|| format!("write {}", path.display()))
}

fn run_conformance(executable: &Path, framework_id: &str, art_dir: &Path) -> Result<String> {
    if !is_safe_package_reference(framework_id) {
        bail!("framework id is not a safe package id: {framework_id}");
    }
    if !executable.is_file() || !art_dir.is_dir() {
        bail!("conformance executable and Art directory must exist");
    }
    let trust_store = TrustStore::default();
    validate_art_package(art_dir, true, &trust_store).context("validate conformance Art")?;
    let art_manifest: Value = read_json(&art_dir.join("manifest.json"))?;
    let declared_framework = art_manifest
        .pointer("/execution/framework")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("conformance Art has no framework id"))?;
    if declared_framework != framework_id {
        bail!(
            "conformance Art declares framework `{declared_framework}`, expected `{framework_id}`"
        );
    }
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let sequence = CONFORMANCE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let temp_dir = std::env::temp_dir().join(format!(
        "loom-plugin-conformance-{}-{nonce}-{sequence}",
        std::process::id(),
    ));
    fs::create_dir_all(&temp_dir)?;
    let _cleanup = RemoveDirOnDrop(temp_dir.clone());
    let request = FrameworkExecuteRequest {
        protocol_version: FRAMEWORK_PROTOCOL_VERSION.to_owned(),
        supported_protocol_versions: vec![FRAMEWORK_PROTOCOL_VERSION.to_owned()],
        framework_id: framework_id.to_owned(),
        art_id: "loom-conformance-art".to_owned(),
        art_dir: fs::canonicalize(art_dir)?,
        inputs: json!({ "input": "conformance" }),
        params: json!({}),
        disabled_params: Vec::new(),
        context: FrameworkExecutionContext {
            request_id: "loom-plugin-conformance".to_owned(),
            cache_dir: temp_dir.join("cache"),
            temp_dir: temp_dir.join("temp"),
            host_version: Some(env!("CARGO_PKG_VERSION").to_owned()),
            ..FrameworkExecutionContext::default()
        },
    };
    fs::create_dir_all(&request.context.cache_dir)?;
    fs::create_dir_all(&request.context.temp_dir)?;
    let payload = serde_json::to_vec(&request)?;
    let mut process = loom_process::ProcessSpec::new(executable.to_path_buf());
    process.args = vec!["--framework-id".to_owned(), framework_id.to_owned()];
    process.current_dir = executable.parent().map(Path::to_path_buf);
    process.limits.timeout = CONFORMANCE_TIMEOUT;
    process.limits.stdout_bytes = MAX_CONFORMANCE_OUTPUT_BYTES;
    process.limits.stderr_bytes = MAX_CONFORMANCE_OUTPUT_BYTES;
    process.limits.max_processes = Some(4);
    let output = loom_process::run_with_input(&process, &payload)
        .with_context(|| format!("run {}", executable.display()))?;
    if !output.status.success() {
        bail!(
            "framework exited with {:?}: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let response: FrameworkExecuteResponse =
        serde_json::from_slice(&output.stdout).with_context(|| {
            format!(
                "parse framework response: {}",
                String::from_utf8_lossy(&output.stdout)
            )
        })?;
    if !loom_protocol::response_status_is_success(&response.status.to_ascii_lowercase()) {
        bail!("framework returned failure status `{}`", response.status);
    }
    Ok(format!(
        "conformance passed: protocol={}, framework={}, stdoutBytes={}, stderrBytes={}",
        FRAMEWORK_PROTOCOL_VERSION,
        framework_id,
        output.stdout.len(),
        output.stderr.len()
    ))
}

struct RemoveDirOnDrop(PathBuf);

impl Drop for RemoveDirOnDrop {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[cfg(test)]
fn read_bounded(mut reader: impl Read) -> Result<Vec<u8>> {
    let mut output = Vec::new();
    reader
        .by_ref()
        .take((MAX_CONFORMANCE_OUTPUT_BYTES + 1) as u64)
        .read_to_end(&mut output)?;
    if output.len() > MAX_CONFORMANCE_OUTPUT_BYTES {
        bail!("process output exceeds {MAX_CONFORMANCE_OUTPUT_BYTES} bytes");
    }
    Ok(output)
}

fn hex_digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    fn temp_root(label: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "loom-plugin-cli-{label}-{}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos(),
            TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).expect("create test root");
        root
    }

    fn run_cli(args: &[String]) -> Result<String> {
        let mut output = Vec::new();
        run(args, &mut output)?;
        Ok(String::from_utf8(output).expect("CLI UTF-8"))
    }

    fn conformance_fixture(root: &Path) -> PathBuf {
        #[cfg(windows)]
        {
            let script = root.join("conformance-fixture.ps1");
            fs::write(
                &script,
                "$null = [Console]::In.ReadToEnd()\n[Console]::Out.Write('{\"status\":\"success\",\"output\":{\"content\":[]}}')\n",
            )
            .expect("write PowerShell fixture");
            let wrapper = root.join("conformance-fixture.cmd");
            fs::write(
                &wrapper,
                "@echo off\r\npowershell.exe -NoProfile -ExecutionPolicy Bypass -File \"%~dp0conformance-fixture.ps1\"\r\n",
            )
            .expect("write command fixture");
            wrapper
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let wrapper = root.join("conformance-fixture.sh");
            fs::write(
                &wrapper,
                "#!/bin/sh\ncat >/dev/null\nprintf '%s' '{\"status\":\"success\",\"output\":{\"content\":[]}}'\n",
            )
            .expect("write shell fixture");
            let mut permissions = fs::metadata(&wrapper).unwrap().permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&wrapper, permissions).unwrap();
            wrapper
        }
    }

    #[test]
    fn embedded_schemas_are_valid_json() {
        for schema in [
            schemas::FRAMEWORK_MANIFEST_V1,
            schemas::FRAMEWORK_EXECUTE_REQUEST_V1,
            schemas::FRAMEWORK_EXECUTE_RESPONSE_V1,
            schemas::FRAMEWORK_AUTHORING_V1,
            schemas::ART_RUNTIME_V1,
        ] {
            serde_json::from_str::<Value>(schema).expect("schema JSON");
        }
    }

    #[test]
    fn help_lists_source_independent_workflow() {
        let mut output = Vec::new();
        run(["loom-plugin", "--help"], &mut output).expect("help");
        let output = String::from_utf8(output).expect("UTF-8");
        assert!(output.contains("init framework"));
        assert!(output.contains("conformance"));
        assert!(output.contains("pack"));
    }

    #[test]
    fn unsafe_package_ids_are_rejected() {
        let root = std::env::temp_dir().join("loom-plugin-cli-unsafe-id");
        let _ = fs::remove_dir_all(&root);
        assert!(init_framework(&root, "../escape").is_err());
    }

    #[test]
    fn bounded_reader_rejects_excess_output() {
        let bytes = vec![b'x'; MAX_CONFORMANCE_OUTPUT_BYTES + 1];
        assert!(read_bounded(Cursor::new(bytes)).is_err());
    }

    #[test]
    fn cli_sign_trust_pack_install_conformance_and_revoke_e2e() {
        let root = temp_root("e2e");
        let framework_dir = root.join("framework-package");
        let art_dir = root.join("art-package");
        let key_path = root.join("publisher-key.json");
        let trust_path = root.join("plugin-trust.json");
        let framework_zip = root.join("framework.zip");
        let art_zip = root.join("art.zip");
        let framework_id = "e2e-framework";
        let qualified_framework_id = "publisher.example/e2e-framework";

        run_cli(&[
            "loom-plugin".to_owned(),
            "init".to_owned(),
            "framework".to_owned(),
            framework_dir.to_string_lossy().into_owned(),
            framework_id.to_owned(),
        ])
        .expect("init framework");
        fs::write(
            framework_dir
                .join("runtime")
                .join(format!("{framework_id}.exe")),
            b"MZ-framework-fixture",
        )
        .expect("framework payload");
        run_cli(&[
            "loom-plugin".to_owned(),
            "keygen".to_owned(),
            key_path.to_string_lossy().into_owned(),
            "publisher-key".to_owned(),
        ])
        .expect("keygen");
        run_cli(&[
            "loom-plugin".to_owned(),
            "sign".to_owned(),
            framework_dir.to_string_lossy().into_owned(),
            key_path.to_string_lossy().into_owned(),
            "publisher.example".to_owned(),
        ])
        .expect("sign framework");
        let verified = run_cli(&[
            "loom-plugin".to_owned(),
            "validate".to_owned(),
            framework_dir.to_string_lossy().into_owned(),
        ])
        .expect("validate verified framework");
        assert!(verified.contains("trust=Verified"), "{verified}");
        run_cli(&[
            "loom-plugin".to_owned(),
            "trust".to_owned(),
            "add".to_owned(),
            trust_path.to_string_lossy().into_owned(),
            "publisher.example".to_owned(),
            key_path.to_string_lossy().into_owned(),
        ])
        .expect("trust publisher");
        let trusted = run_cli(&[
            "loom-plugin".to_owned(),
            "validate".to_owned(),
            framework_dir.to_string_lossy().into_owned(),
            "--trust-store".to_owned(),
            trust_path.to_string_lossy().into_owned(),
        ])
        .expect("validate trusted framework");
        assert!(trusted.contains("trust=Trusted"), "{trusted}");
        run_cli(&[
            "loom-plugin".to_owned(),
            "pack".to_owned(),
            framework_dir.to_string_lossy().into_owned(),
            framework_zip.to_string_lossy().into_owned(),
        ])
        .expect("pack framework");
        assert!(framework_zip.is_file());
        assert!(root.join("framework.zip.sha256").is_file());

        let framework_registry = loom_tool_registry::framework::FrameworkRegistry::new(&root);
        let installed_framework = framework_registry
            .install_framework_package_from_zip(&fs::read(&framework_zip).unwrap())
            .expect("install packed framework");
        assert_eq!(installed_framework.qualified_id, qualified_framework_id);
        assert!(installed_framework.ready);

        run_cli(&[
            "loom-plugin".to_owned(),
            "init".to_owned(),
            "art".to_owned(),
            art_dir.to_string_lossy().into_owned(),
            "e2e-art".to_owned(),
            qualified_framework_id.to_owned(),
        ])
        .expect("init Art");
        fs::write(art_dir.join("runtime/main.exe"), b"MZ-art-fixture").expect("Art payload");
        run_cli(&[
            "loom-plugin".to_owned(),
            "sign".to_owned(),
            art_dir.to_string_lossy().into_owned(),
            key_path.to_string_lossy().into_owned(),
            "publisher.example".to_owned(),
        ])
        .expect("sign Art");
        let trusted_art = run_cli(&[
            "loom-plugin".to_owned(),
            "validate".to_owned(),
            art_dir.to_string_lossy().into_owned(),
            "--trust-store".to_owned(),
            trust_path.to_string_lossy().into_owned(),
        ])
        .expect("validate trusted Art");
        assert!(trusted_art.contains("trust=Trusted"), "{trusted_art}");
        run_cli(&[
            "loom-plugin".to_owned(),
            "pack".to_owned(),
            art_dir.to_string_lossy().into_owned(),
            art_zip.to_string_lossy().into_owned(),
        ])
        .expect("pack Art");

        let tool_registry = loom_tool_registry::ToolRegistry::new(root.join("tools"));
        let installed_art = loom_tool_registry::install::install_art_from_zip(
            &fs::read(&art_zip).unwrap(),
            &root,
            &framework_registry,
            &tool_registry,
        )
        .expect("install packed Art");
        assert_eq!(installed_art.tool_id, "e2e-art");
        assert_eq!(installed_art.framework, qualified_framework_id);

        let fixture = conformance_fixture(&root);
        let conformance = run_cli(&[
            "loom-plugin".to_owned(),
            "conformance".to_owned(),
            fixture.to_string_lossy().into_owned(),
            qualified_framework_id.to_owned(),
            art_dir.to_string_lossy().into_owned(),
        ])
        .expect("run conformance");
        assert!(conformance.contains("conformance passed"), "{conformance}");

        run_cli(&[
            "loom-plugin".to_owned(),
            "trust".to_owned(),
            "revoke".to_owned(),
            trust_path.to_string_lossy().into_owned(),
            "publisher.example".to_owned(),
            "publisher-key".to_owned(),
        ])
        .expect("revoke publisher");
        let revoked = run_cli(&[
            "loom-plugin".to_owned(),
            "validate".to_owned(),
            art_dir.to_string_lossy().into_owned(),
            "--trust-store".to_owned(),
            trust_path.to_string_lossy().into_owned(),
        ]);
        assert!(revoked
            .expect_err("revoked package must be rejected")
            .to_string()
            .contains("revoked"));
        fs::remove_dir_all(&root).ok();
    }
}
