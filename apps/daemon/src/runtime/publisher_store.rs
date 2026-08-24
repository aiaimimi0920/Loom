// Publisher identity persistence and bounded Art store catalog/package access.
const fn publisher_identity_schema_version() -> u32 {
    1
}

fn publisher_identity_path(control_plane_root: &Path) -> PathBuf {
    control_plane_root.join(PUBLISHER_IDENTITY_FILE)
}

fn load_publisher_identity(
    control_plane_root: &Path,
) -> std::result::Result<Option<LocalPublisherIdentity>, String> {
    let path = publisher_identity_path(control_plane_root);
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|error| format!("用户签名身份无效：{error}")),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("无法读取用户签名身份：{error}")),
    }
}

fn save_publisher_identity(
    control_plane_root: &Path,
    identity: &LocalPublisherIdentity,
) -> std::result::Result<(), String> {
    // The previous sequence wrote a fixed-name temporary, deleted the live file, then renamed —
    // leaving a window with no identity file on disk at all, which is the one outcome a temporary
    // exists to prevent. `write_json_atomically` replaces the file in one step, so a reader always
    // sees either the old identity or the new one, and the unique temporary name means two
    // concurrent callers cannot overwrite each other's partial bytes.
    let path = publisher_identity_path(control_plane_root);
    write_json_atomically(&path, identity).map_err(|error| format!("{error:#}"))
}

fn save_current_signing_key(
    control_plane_root: &Path,
    key: &SigningKeyDocument,
) -> std::result::Result<(), String> {
    let value = serde_json::to_string(key).map_err(|error| error.to_string())?;
    CredentialStore::new(control_plane_root)
        .upsert(CredentialInput {
            name: PUBLISHER_PRIVATE_KEY_CREDENTIAL.to_owned(),
            value,
            value_type: CredentialValueType::Json,
            scope: CredentialScope::default(),
            expires_at: None,
        })
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn load_current_signing_key(
    control_plane_root: &Path,
) -> std::result::Result<Option<SigningKeyDocument>, String> {
    let details = CredentialStore::new(control_plane_root)
        .reveal(
            PUBLISHER_PRIVATE_KEY_CREDENTIAL,
            &CredentialScope::default(),
        )
        .map_err(|error| error.to_string())?;
    details
        .map(|details| {
            serde_json::from_str(&details.value)
                .map_err(|error| format!("用户私钥记录无效：{error}"))
        })
        .transpose()
}

fn publisher_key_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("key-{nanos}")
}

fn ensure_local_publisher_identity(
    control_plane_root: &Path,
) -> std::result::Result<(LocalPublisherIdentity, SigningKeyDocument), String> {
    let stored_identity = load_publisher_identity(control_plane_root)?;
    let stored_key = load_current_signing_key(control_plane_root)?;
    if let (Some(mut identity), Some(key)) = (stored_identity, stored_key) {
        if key.key_id == identity.current_key_id && key.public_key == identity.public_key {
            if identity.user_id != DEFAULT_TEST_PUBLISHER_ID {
                identity.user_id = DEFAULT_TEST_PUBLISHER_ID.to_owned();
                save_publisher_identity(control_plane_root, &identity)?;
            }
            return Ok((identity, key));
        }
    }

    let key = generate_signing_key(publisher_key_id());
    let identity = LocalPublisherIdentity {
        schema_version: publisher_identity_schema_version(),
        user_id: DEFAULT_TEST_PUBLISHER_ID.to_owned(),
        current_key_id: key.key_id.clone(),
        public_key: key.public_key.clone(),
    };
    save_current_signing_key(control_plane_root, &key)?;
    save_publisher_identity(control_plane_root, &identity)?;
    Ok((identity, key))
}

fn reset_local_publisher_identity(
    control_plane_root: &Path,
    identity: &LocalPublisherIdentity,
) -> std::result::Result<(LocalPublisherIdentity, SigningKeyDocument), String> {
    let key = generate_signing_key(publisher_key_id());
    let next_identity = LocalPublisherIdentity {
        schema_version: publisher_identity_schema_version(),
        user_id: identity.user_id.clone(),
        current_key_id: key.key_id.clone(),
        public_key: key.public_key.clone(),
    };
    save_current_signing_key(control_plane_root, &key)?;
    save_publisher_identity(control_plane_root, &next_identity)?;
    Ok((next_identity, key))
}

fn fetch_remote_publisher(
    store: &str,
    user_id: &str,
) -> std::result::Result<RemotePublisher, String> {
    if !is_platform_publisher_id(user_id) {
        return Err("用户 ID 格式无效".to_owned());
    }
    let url = format!("{}/publishers/{user_id}", store.trim_end_matches('/'));
    let client = art_store_client().map_err(|error| error.to_string())?;
    let policy = user_configured_outbound_policy();
    let bytes = get_bounded(&client, &url, &policy, MAX_PUBLISHER_DIRECTORY_BYTES)
        .map_err(|error| format!("获取发布者信息失败：{error}"))?;
    let response: RemotePublisherResponse =
        serde_json::from_slice(&bytes).map_err(|error| format!("发布者信息无效：{error}"))?;
    if response.publisher.user_id != user_id {
        return Err("发布者信息中的用户 ID 不匹配".to_owned());
    }
    Ok(response.publisher)
}

fn sync_trusted_publisher_from_store(
    store: &str,
    user_id: &str,
    framework_registry: &FrameworkRegistry,
) -> std::result::Result<(), String> {
    let publisher = fetch_remote_publisher(store, user_id)?;
    if publisher.keys.is_empty() {
        return Err("该用户没有可验证的公钥".to_owned());
    }
    let records = publisher.keys.into_iter().map(|key| PublisherTrustRecord {
        publisher_id: user_id.to_owned(),
        key_id: key.key_id,
        public_key: key.public_key,
        revoked: key.status == RemotePublisherKeyStatus::Revoked,
    });
    framework_registry
        .trust_publisher_directory(user_id, records)
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn post_art_store_json<T: Serialize, R: serde::de::DeserializeOwned>(
    store: &str,
    path: &str,
    payload: &T,
) -> std::result::Result<R, String> {
    let url = format!("{}{}", store.trim_end_matches('/'), path);
    let parsed_url = reqwest::Url::parse(&url).map_err(|error| error.to_string())?;
    let policy = user_configured_outbound_policy();
    validate_outbound_url(&parsed_url, &policy)?;
    let client = art_store_client().map_err(|error| error.to_string())?;
    let mut response = client
        .post(parsed_url)
        .json(payload)
        .send()
        .map_err(|error| error.to_string())?;
    let status = response.status();
    let mut bytes = Vec::new();
    response
        .by_ref()
        .take((MAX_PUBLISHER_DIRECTORY_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() > MAX_PUBLISHER_DIRECTORY_BYTES {
        return Err("Art 商店返回的发布者信息过大".to_owned());
    }
    if !status.is_success() {
        let message = String::from_utf8_lossy(&bytes);
        return Err(format!("Art 商店返回 HTTP {status}: {message}"));
    }
    serde_json::from_slice(&bytes).map_err(|error| error.to_string())
}

fn ensure_remote_publisher_registered(
    store: &str,
    identity: &LocalPublisherIdentity,
    key: &SigningKeyDocument,
) -> std::result::Result<RemotePublisher, String> {
    let response: RemotePublisherResponse = post_art_store_json(
        store,
        "/publishers/register",
        &json!({
            "userId": identity.user_id,
            "keyId": key.key_id,
            "publicKey": key.public_key
        }),
    )?;
    if response.publisher.user_id != identity.user_id
        || !response.publisher.keys.iter().any(|remote| {
            remote.key_id == key.key_id
                && remote.public_key == key.public_key
                && remote.status == RemotePublisherKeyStatus::Active
        })
    {
        return Err("Art 商店返回了无效的用户身份".to_owned());
    }
    Ok(response.publisher)
}

fn fetch_remote_art_store_catalog(
    store: &str,
) -> std::result::Result<RemoteArtStoreCatalog, String> {
    let url = format!("{}/catalog", store.trim_end_matches('/'));
    let client = art_store_client().map_err(|error| error.to_string())?;
    let policy = user_configured_outbound_policy();
    let bytes = get_bounded(&client, &url, &policy, MAX_ART_STORE_CATALOG_BYTES)
        .map_err(|error| format!("获取 art 商店目录失败：{error}"))?;
    serde_json::from_slice(&bytes).map_err(|error| format!("art 商店目录无效：{error}"))
}

// Proxy the remote art store catalog (GET {store}/catalog).
fn fetch_art_store_catalog(path: &str) -> Result<(u16, String)> {
    if query_value(path, "store").is_some() {
        return structured_error(
            400,
            json!({
                "code": "custom_art_store_not_supported",
                "message": "Loom 不支持选择第三方 Art 商店"
            }),
        );
    }
    let Some(store) = resolve_art_store_url() else {
        return structured_error(
            400,
            json!({ "code": "art_store_not_configured", "message": "Loom 官方 Art 服务暂不可用" }),
        );
    };
    match fetch_remote_art_store_catalog(&store) {
        Ok(catalog) => Ok((200, serde_json::to_string(&catalog)?)),
        Err(error) => structured_error(
            502,
            json!({ "code": "art_store_unavailable", "message": error, "url": store }),
        ),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InstallFromStoreRequest {
    art_id: String,
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    store: Option<String>,
    /// Optional caller-provided root package digest. Dependencies use the
    /// store's adjacent `.sha256` sidecar.
    #[serde(default)]
    sha256: Option<String>,
}

fn sha256_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn normalize_sha256(value: &str) -> Option<String> {
    let digest = value.split_whitespace().next()?.trim().to_ascii_lowercase();
    (digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit())).then_some(digest)
}

fn resolve_art_store_package_version(
    catalog: Option<&RemoteArtStoreCatalog>,
    id: &str,
    requested: Option<&str>,
) -> std::result::Result<String, String> {
    if let Some(version) = requested {
        if semver::Version::parse(version).is_err() {
            return Err(format!("store package `{id}` target version is invalid"));
        }
        return Ok(version.to_owned());
    }
    let entry = catalog
        .and_then(|catalog| catalog.arts.iter().find(|entry| entry.id == id))
        .ok_or_else(|| format!("store catalog does not contain Art `{id}`"))?;
    if semver::Version::parse(&entry.latest_version).is_err()
        || !entry
            .versions
            .iter()
            .any(|version| version.version == entry.latest_version)
    {
        return Err(format!(
            "store catalog has no valid latest version for Art `{id}`"
        ));
    }
    Ok(entry.latest_version.clone())
}

fn fetch_art_store_package(
    client: &reqwest::blocking::Client,
    policy: &OutboundPolicy,
    store: &str,
    id: &str,
    version: Option<&str>,
    expected_sha256: Option<&str>,
) -> std::result::Result<Vec<u8>, loom_tool_registry::install::ArtInstallError> {
    if !is_safe_package_id(id) {
        return Err(loom_tool_registry::install::ArtInstallError::InvalidArtId(
            id.to_owned(),
        ));
    }
    let catalog = version
        .is_none()
        .then(|| fetch_remote_art_store_catalog(store))
        .transpose()
        .map_err(|error| {
            loom_tool_registry::install::ArtInstallError::InvalidPackage(format!(
                "resolve latest version for `{id}` from store: {error}"
            ))
        })?;
    let version = resolve_art_store_package_version(catalog.as_ref(), id, version)
        .map_err(loom_tool_registry::install::ArtInstallError::InvalidPackage)?;
    let url = format!("{store}/arts/{id}/{version}.zip");
    let expected = if let Some(expected) = expected_sha256 {
        normalize_sha256(expected)
    } else {
        let sidecar_url = format!("{url}.sha256");
        let sidecar = get_bounded(client, &sidecar_url, policy, 4096).map_err(|error| {
            loom_tool_registry::install::ArtInstallError::InvalidPackage(format!(
                "fetch digest for `{id}` from store: {error}"
            ))
        })?;
        std::str::from_utf8(&sidecar)
            .ok()
            .and_then(normalize_sha256)
    }
    .ok_or_else(|| {
        loom_tool_registry::install::ArtInstallError::InvalidPackage(format!(
            "store package `{id}` must provide a valid sha256 digest"
        ))
    })?;
    let bytes =
        get_bounded(client, &url, policy, MAX_ART_STORE_PACKAGE_BYTES).map_err(|error| {
            loom_tool_registry::install::ArtInstallError::InvalidPackage(format!(
                "fetch `{id}` from store: {error}"
            ))
        })?;
    let actual = sha256_bytes(&bytes);
    if actual != expected {
        return Err(
            loom_tool_registry::install::ArtInstallError::InvalidPackage(format!(
                "store package `{id}` sha256 mismatch: expected {expected}, got {actual}"
            )),
        );
    }
    Ok(bytes)
}

// Install an art (and its dependents) from the remote store: fetch the root art
// zip, then recursively fetch/install dependent arts by id.
