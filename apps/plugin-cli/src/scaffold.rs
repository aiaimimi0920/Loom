// Framework and Art package skeleton creation.
fn init_framework(directory: &Path, id: &str, publisher: &str) -> Result<()> {
    if !is_safe_package_id(id) || !is_safe_publisher_id(publisher) {
        bail!("framework or publisher id is not safe: {publisher}/{id}");
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
        "publisher": { "id": publisher },
        "platforms": ["windows-x64"],
        "entry": {
            "kind": "process",
            "command": format!("runtime/{id}.exe"),
            "args": [],
            "processModel": "per_execution"
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
    write_bytes_atomic(
        &directory.join("runtime").join("README.txt"),
        b"Place the framework process entry declared by framework.manifest.json here.\n",
    )?;
    Ok(())
}

fn init_art(directory: &Path, id: &str, framework: &str, publisher: &str) -> Result<()> {
    if !is_safe_package_id(id)
        || !is_safe_package_reference(framework)
        || !is_safe_publisher_id(publisher)
    {
        bail!("Art, framework, and publisher ids must be safe package ids");
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
                "packageSecurity": {
                    "version": "0.1.0",
                    "publisher": { "id": publisher }
                },
                "art": { "qualifiedId": format!("{publisher}/{id}") },
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
    write_bytes_atomic(
        &directory.join("runtime").join("README.txt"),
        b"Place the Art runtime entry declared by art.runtime.json here.\n",
    )?;
    Ok(())
}

fn init_capability(directory: &Path, id: &str, publisher: &str) -> Result<()> {
    if !is_safe_package_id(id) || !is_safe_publisher_id(publisher) {
        bail!("capability or publisher id is not safe: {publisher}/{id}");
    }
    ensure_empty_directory(directory)?;
    fs::create_dir_all(directory.join("runtime"))?;
    let command_id = format!("{publisher}/{id}.run");
    write_pretty_json(
        directory.join("capability.manifest.json"),
        &json!({
            "schemaVersion": 1,
            "kind": "capability",
            "id": id,
            "name": id,
            "description": "Third-party Loom capability",
            "version": "0.1.0",
            "publisher": { "id": publisher, "keyId": "replace-with-key-id" },
            "hostCompatibility": {
                "loomCapabilityApi": { "minimum": "1.0", "requiredFeatures": ["commands.v1"] },
                "hookExtensionApi": { "minimum": "1.0", "requiredFeatures": ["commands.v1"] }
            },
            "entrypoints": {
                "service": {
                    "targets": {
                        "windows-x64": { "command": format!("runtime/{id}.exe"), "args": [] }
                    },
                    "processModel": "on_demand"
                }
            },
            "activationEvents": [format!("onCommand:{command_id}")],
            "contributes": {
                "commands": [{ "id": command_id, "title": format!("Run {id}") }]
            },
            "permissions": [],
            "resources": { "memoryMiB": 128, "maxProcesses": 1, "timeoutSeconds": 30 },
            "dependencies": [],
            "signature": {
                "algorithm": "ed25519",
                "keyId": "replace-with-key-id",
                "file": "signature.json"
            }
        }),
    )?;
    write_bytes_atomic(
        &directory.join("runtime").join("README.txt"),
        b"Place the framed-JSON capability runtime declared by capability.manifest.json here.\n",
    )?;
    Ok(())
}

fn ensure_empty_directory(directory: &Path) -> Result<()> {
    match fs::symlink_metadata(directory) {
        Ok(metadata) => {
            if !metadata.file_type().is_dir() || is_reparse_or_symlink(&metadata) {
                bail!("package destination must be a real directory: {}", directory.display());
            }
            if fs::read_dir(directory)?.next().is_some() {
                bail!("directory is not empty: {}", directory.display());
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir_all(directory)?;
        }
        Err(error) => return Err(error.into()),
    }
    ensure_real_directory(directory, "package destination")?;
    Ok(())
}

fn write_pretty_json(path: PathBuf, value: &Value) -> Result<()> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    write_bytes_atomic(&path, &bytes).with_context(|| format!("write {}", path.display()))
}
