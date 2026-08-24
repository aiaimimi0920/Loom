// Publisher identity allocation, key registration, lookup and authenticated rotation.
use std::path::Path;

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};

use crate::filesystem::read_optional_regular_file;
use crate::model::{
    PublisherDirectoryEntry, PublisherKeyStatus, PublisherPublicKey, PublisherRotationRequest,
    StoreError,
};
use crate::persistence::{lock_store, write_json_atomic};
use crate::validation::is_safe_art_id;

pub const PUBLISHER_DIRECTORY_FILE: &str = "publisher-directory.json";
const PUBLISHER_DIRECTORY_SCHEMA_VERSION: u32 = 1;
const FIRST_PUBLISHER_NUMBER: u64 = 10_000_000_000;
const LAST_PUBLISHER_NUMBER: u64 = 39_999_999_999;
const MAX_PUBLISHER_DIRECTORY_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct PublisherDirectory {
    #[serde(default = "publisher_directory_schema_version")]
    schema_version: u32,
    #[serde(default = "first_publisher_number")]
    next_numeric: u64,
    #[serde(default)]
    publishers: std::collections::BTreeMap<String, PublisherDirectoryEntry>,
}

impl Default for PublisherDirectory {
    fn default() -> Self {
        Self {
            schema_version: publisher_directory_schema_version(),
            next_numeric: first_publisher_number(),
            publishers: std::collections::BTreeMap::new(),
        }
    }
}

const fn publisher_directory_schema_version() -> u32 {
    PUBLISHER_DIRECTORY_SCHEMA_VERSION
}

const fn first_publisher_number() -> u64 {
    FIRST_PUBLISHER_NUMBER
}

fn load_publisher_directory(root: &Path) -> Result<PublisherDirectory, StoreError> {
    let directory = match read_optional_regular_file(
        root,
        &root.join(PUBLISHER_DIRECTORY_FILE),
        MAX_PUBLISHER_DIRECTORY_BYTES,
    )? {
        Some(bytes) => serde_json::from_slice(&bytes)?,
        None => PublisherDirectory::default(),
    };
    if directory.schema_version != PUBLISHER_DIRECTORY_SCHEMA_VERSION {
        return Err(StoreError::UnsupportedPublisherDirectorySchema(
            directory.schema_version,
        ));
    }
    Ok(directory)
}

fn write_publisher_directory(
    root: &Path,
    directory: &PublisherDirectory,
) -> Result<(), StoreError> {
    write_json_atomic(root, PUBLISHER_DIRECTORY_FILE, directory)
}

fn validate_publisher_key(key_id: &str, public_key: &str) -> Result<(), StoreError> {
    if !is_safe_art_id(key_id) {
        return Err(StoreError::InvalidPublisherKeyId(key_id.to_owned()));
    }
    let decoded = BASE64
        .decode(public_key.as_bytes())
        .map_err(|_| StoreError::InvalidPublisherPublicKey)?;
    let bytes: [u8; 32] = decoded
        .try_into()
        .map_err(|_| StoreError::InvalidPublisherPublicKey)?;
    VerifyingKey::from_bytes(&bytes).map_err(|_| StoreError::InvalidPublisherPublicKey)?;
    Ok(())
}

pub fn is_platform_publisher_id(value: &str) -> bool {
    (value.len() == 13
        && value.starts_with("NU")
        && value[2..].bytes().all(|byte| byte.is_ascii_digit()))
        || (value.len() == 11
            && value.starts_with('L')
            && value[1..].bytes().all(|byte| byte.is_ascii_digit()))
}

pub fn publisher_rotation_message(
    user_id: &str,
    current_key_id: &str,
    new_key_id: &str,
    new_public_key: &str,
) -> String {
    format!("loom.publisher.rotate.v1\n{user_id}\n{current_key_id}\n{new_key_id}\n{new_public_key}")
}

pub fn register_publisher(
    root: &Path,
    key_id: &str,
    public_key: &str,
) -> Result<PublisherDirectoryEntry, StoreError> {
    register_publisher_with_id(root, None, key_id, public_key)
}

pub fn register_publisher_with_id(
    root: &Path,
    requested_user_id: Option<&str>,
    key_id: &str,
    public_key: &str,
) -> Result<PublisherDirectoryEntry, StoreError> {
    validate_publisher_key(key_id, public_key)?;
    let _lock = lock_store(root)?;
    let mut directory = load_publisher_directory(root)?;
    if let Some(requested_user_id) = requested_user_id {
        if !is_platform_publisher_id(requested_user_id) {
            return Err(StoreError::InvalidPublisherId(requested_user_id.to_owned()));
        }
        if let Some(existing) = directory.publishers.get(requested_user_id) {
            if existing
                .keys
                .iter()
                .any(|key| key.key_id == key_id && key.public_key == public_key)
            {
                return Ok(existing.clone());
            }
            return Err(StoreError::PublisherKeyConflict {
                publisher: requested_user_id.to_owned(),
                key_id: key_id.to_owned(),
            });
        }
        let entry = PublisherDirectoryEntry {
            user_id: requested_user_id.to_owned(),
            keys: vec![PublisherPublicKey {
                key_id: key_id.to_owned(),
                public_key: public_key.to_owned(),
                status: PublisherKeyStatus::Active,
                created_at: unix_timestamp(),
            }],
        };
        directory
            .publishers
            .insert(requested_user_id.to_owned(), entry.clone());
        write_publisher_directory(root, &directory)?;
        return Ok(entry);
    }
    if let Some(existing) = directory.publishers.values().find(|publisher| {
        publisher
            .keys
            .iter()
            .any(|key| key.public_key == public_key)
    }) {
        return Ok(existing.clone());
    }
    let used = directory
        .publishers
        .keys()
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    let mut numeric = directory.next_numeric.max(FIRST_PUBLISHER_NUMBER);
    let user_id = loop {
        if numeric > LAST_PUBLISHER_NUMBER {
            return Err(StoreError::PublisherIdExhausted);
        }
        let candidate = format!("NU{numeric:011}");
        numeric += 1;
        if !used.contains(&candidate) {
            break candidate;
        }
    };
    let entry = PublisherDirectoryEntry {
        user_id: user_id.clone(),
        keys: vec![PublisherPublicKey {
            key_id: key_id.to_owned(),
            public_key: public_key.to_owned(),
            status: PublisherKeyStatus::Active,
            created_at: unix_timestamp(),
        }],
    };
    directory.schema_version = PUBLISHER_DIRECTORY_SCHEMA_VERSION;
    directory.next_numeric = numeric;
    directory.publishers.insert(user_id, entry.clone());
    write_publisher_directory(root, &directory)?;
    Ok(entry)
}

pub fn read_publisher(
    root: &Path,
    user_id: &str,
) -> Result<Option<PublisherDirectoryEntry>, StoreError> {
    if !is_platform_publisher_id(user_id) {
        return Err(StoreError::InvalidPublisherId(user_id.to_owned()));
    }
    Ok(load_publisher_directory(root)?
        .publishers
        .get(user_id)
        .cloned())
}

pub fn rotate_publisher_key(
    root: &Path,
    user_id: &str,
    request: &PublisherRotationRequest,
) -> Result<PublisherDirectoryEntry, StoreError> {
    if !is_platform_publisher_id(user_id) {
        return Err(StoreError::InvalidPublisherId(user_id.to_owned()));
    }
    validate_publisher_key(&request.new_key_id, &request.new_public_key)?;
    let _lock = lock_store(root)?;
    let mut directory = load_publisher_directory(root)?;
    let publisher = directory
        .publishers
        .get_mut(user_id)
        .ok_or_else(|| StoreError::PublisherNotFound(user_id.to_owned()))?;
    if publisher
        .keys
        .iter()
        .any(|key| key.key_id == request.new_key_id)
    {
        return Err(StoreError::PublisherKeyConflict {
            publisher: user_id.to_owned(),
            key_id: request.new_key_id.clone(),
        });
    }
    let current = publisher
        .keys
        .iter()
        .find(|key| {
            key.key_id == request.current_key_id && key.status == PublisherKeyStatus::Active
        })
        .ok_or_else(|| StoreError::PublisherActiveKeyMissing(user_id.to_owned()))?;
    let current_bytes = BASE64
        .decode(current.public_key.as_bytes())
        .map_err(|_| StoreError::InvalidPublisherPublicKey)?;
    let current_bytes: [u8; 32] = current_bytes
        .try_into()
        .map_err(|_| StoreError::InvalidPublisherPublicKey)?;
    let verifying_key = VerifyingKey::from_bytes(&current_bytes)
        .map_err(|_| StoreError::InvalidPublisherPublicKey)?;
    let signature_bytes = BASE64
        .decode(request.signature.as_bytes())
        .map_err(|_| StoreError::PublisherRotationSignature)?;
    let signature = Signature::from_slice(&signature_bytes)
        .map_err(|_| StoreError::PublisherRotationSignature)?;
    verifying_key
        .verify(
            publisher_rotation_message(
                user_id,
                &request.current_key_id,
                &request.new_key_id,
                &request.new_public_key,
            )
            .as_bytes(),
            &signature,
        )
        .map_err(|_| StoreError::PublisherRotationSignature)?;
    for key in &mut publisher.keys {
        if key.status == PublisherKeyStatus::Active {
            key.status = PublisherKeyStatus::Retired;
        }
    }
    publisher.keys.push(PublisherPublicKey {
        key_id: request.new_key_id.clone(),
        public_key: request.new_public_key.clone(),
        status: PublisherKeyStatus::Active,
        created_at: unix_timestamp(),
    });
    let entry = publisher.clone();
    write_publisher_directory(root, &directory)?;
    Ok(entry)
}

fn unix_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
