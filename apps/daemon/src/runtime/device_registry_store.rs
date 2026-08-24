// Managed-device persistence, device sessions, authentication, and attachment validation.
type SharedDeviceRegistryStore = Arc<Mutex<DeviceRegistryStore>>;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ManagedDeviceKind {
    Computer,
    Tablet,
    Phone,
    Other,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManagedDevice {
    id: String,
    name: String,
    kind: ManagedDeviceKind,
    address: String,
    approval: String,
    created_at: u64,
    last_seen_at: Option<u64>,
    #[serde(default)]
    is_local: bool,
    #[serde(default = "default_managed_device_enabled")]
    enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    public_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    key_fingerprint: Option<String>,
    #[serde(default)]
    session_epoch: u64,
}

fn default_managed_device_enabled() -> bool {
    true
}

#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeviceRegistryDocument {
    devices: Vec<ManagedDevice>,
}

struct DeviceRegistryStore {
    path: PathBuf,
    devices: BTreeMap<String, ManagedDevice>,
    challenges: BTreeMap<String, DeviceSessionChallenge>,
    sessions: BTreeMap<String, ActiveDeviceSession>,
}

const DEVICE_CHALLENGE_TTL_MILLIS: u64 = 60_000;
const DEVICE_SESSION_TTL_MILLIS: u64 = 15 * 60_000;
const DEVICE_SESSION_MAX_NONCES: usize = 65_536;

#[derive(Clone)]
struct DeviceSessionChallenge {
    challenge_id: String,
    device_id: String,
    challenge: String,
    expires_at_ms: u64,
}

struct ActiveDeviceSession {
    device_id: String,
    expires_at_ms: u64,
    session_epoch: u64,
    used_nonces: BTreeSet<String>,
}

#[derive(Debug)]
struct DeviceAuthError {
    status: u16,
    code: &'static str,
    message: String,
}

impl DeviceAuthError {
    fn new(status: u16, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status,
            code,
            message: message.into(),
        }
    }
}

impl DeviceRegistryStore {
    /// Open the device registry, refusing to start when the file exists but cannot be read.
    ///
    /// This is deliberately fallible. The registry holds each paired device's `public_key`,
    /// `key_fingerprint` and `session_epoch`, and `session_epoch` is the revocation counter: a
    /// loader that treated unparsable bytes as "no devices" would insert the synthetic local host,
    /// persist that, and thereby discard every revocation on record — a device revoked by bumping
    /// its epoch could then be re-paired. An absent file is a legitimately empty registry; a file
    /// that is present and unreadable is an operator problem, so it fails closed with the path in
    /// the message rather than quietly resetting the authorizations.
    fn new(path: PathBuf, local_addr: SocketAddr) -> Result<Self> {
        let mut devices: BTreeMap<String, ManagedDevice> = match fs::read(&path) {
            Ok(bytes) => serde_json::from_slice::<DeviceRegistryDocument>(&bytes)
                .with_context(|| {
                    format!(
                        "parse device registry `{}` — it exists but is unreadable, so refusing to \
                         start rather than discarding the paired devices and their revocation \
                         counters; move the file aside to start with an empty registry",
                        path.display()
                    )
                })?
                .devices
                .into_iter()
                .map(|device| (device.id.clone(), device))
                .collect(),
            Err(error) if error.kind() == ErrorKind::NotFound => BTreeMap::new(),
            Err(error) => {
                return Err(anyhow::Error::new(error).context(format!(
                    "read device registry `{}` — refusing to start rather than discarding the \
                     paired devices and their revocation counters",
                    path.display()
                )))
            }
        };
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        let local_id = "device-000-local".to_owned();
        let created_at = devices
            .get(&local_id)
            .map(|device| device.created_at)
            .unwrap_or(now);
        let local_name = std::env::var("COMPUTERNAME")
            .or_else(|_| std::env::var("HOSTNAME"))
            .ok()
            .map(|name| name.trim().to_owned())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| "Loom 主机".to_owned());
        devices.insert(
            local_id.clone(),
            ManagedDevice {
                id: local_id,
                name: local_name,
                kind: ManagedDeviceKind::Computer,
                address: local_addr.to_string(),
                approval: "approved".to_owned(),
                created_at,
                last_seen_at: Some(now),
                is_local: true,
                enabled: true,
                public_key: None,
                key_fingerprint: None,
                session_epoch: 1,
            },
        );
        let store = Self {
            path,
            devices,
            challenges: BTreeMap::new(),
            sessions: BTreeMap::new(),
        };
        if let Err(error) = store.persist() {
            eprintln!("loom device registry could not persist the local host: {error:#}");
        }
        Ok(store)
    }

    fn persist(&self) -> Result<()> {
        let document = DeviceRegistryDocument {
            devices: self.devices.values().cloned().collect(),
        };
        write_json_atomically(&self.path, &document)
            .with_context(|| format!("write device registry `{}`", self.path.display()))
    }

    fn create_session_challenge(
        &mut self,
        device_id: &str,
    ) -> std::result::Result<DeviceSessionChallengeResponse, DeviceAuthError> {
        self.cleanup_expired_device_auth();
        let device_id = self.authorized_keyed_device(device_id)?.id.clone();
        let now = unix_time_millis();
        let mut random = [0_u8; 32];
        OsRng.fill_bytes(&mut random);
        let challenge = BASE64_URL.encode(random);
        let challenge_id = format!("challenge:{}", Uuid::new_v4());
        let expires_at_ms = now.saturating_add(DEVICE_CHALLENGE_TTL_MILLIS);
        self.challenges.insert(
            challenge_id.clone(),
            DeviceSessionChallenge {
                challenge_id: challenge_id.clone(),
                device_id: device_id.clone(),
                challenge: challenge.clone(),
                expires_at_ms,
            },
        );
        Ok(DeviceSessionChallengeResponse {
            protocol_version: DEVICE_SESSION_PROTOCOL_VERSION.to_owned(),
            challenge_id,
            device_id,
            challenge,
            expires_at_ms,
        })
    }

    fn issue_device_session(
        &mut self,
        input: DeviceSessionIssueRequest,
    ) -> std::result::Result<DeviceSessionIssueResponse, DeviceAuthError> {
        self.cleanup_expired_device_auth();
        validate_device_auth_identifier("device id", &input.device_id)?;
        validate_device_auth_identifier("challenge id", &input.challenge_id)?;
        validate_device_auth_nonce(&input.client_nonce)?;
        let challenge = self.challenges.remove(&input.challenge_id).ok_or_else(|| {
            DeviceAuthError::new(
                401,
                "device_challenge_invalid",
                "device session challenge is missing, expired, or already used",
            )
        })?;
        if challenge.device_id != input.device_id || challenge.challenge_id != input.challenge_id {
            return Err(DeviceAuthError::new(
                401,
                "device_challenge_mismatch",
                "device session challenge does not belong to this device",
            ));
        }
        let device = self.authorized_keyed_device(&input.device_id)?.clone();
        let public_key = decode_device_public_key(
            device
                .public_key
                .as_deref()
                .expect("authorized keyed device has a public key"),
        )?;
        let signature_bytes = BASE64.decode(input.signature.trim()).map_err(|_| {
            DeviceAuthError::new(
                401,
                "device_signature_invalid",
                "device session signature is not valid Base64",
            )
        })?;
        let signature = Signature::from_slice(&signature_bytes).map_err(|_| {
            DeviceAuthError::new(
                401,
                "device_signature_invalid",
                "device session signature must be an Ed25519 signature",
            )
        })?;
        let message = device_session_signature_message(
            &input.device_id,
            &input.challenge_id,
            &challenge.challenge,
            &input.client_nonce,
        );
        public_key
            .verify(message.as_bytes(), &signature)
            .map_err(|_| {
                DeviceAuthError::new(
                    401,
                    "device_signature_invalid",
                    "device session signature verification failed",
                )
            })?;

        let mut token_bytes = [0_u8; 32];
        OsRng.fill_bytes(&mut token_bytes);
        let token = BASE64_URL.encode(token_bytes);
        let token_hash = sha256_bytes(token.as_bytes());
        let expires_at_ms = unix_time_millis().saturating_add(DEVICE_SESSION_TTL_MILLIS);
        self.sessions.insert(
            token_hash,
            ActiveDeviceSession {
                device_id: device.id.clone(),
                expires_at_ms,
                session_epoch: device.session_epoch,
                used_nonces: BTreeSet::new(),
            },
        );
        Ok(DeviceSessionIssueResponse {
            protocol_version: DEVICE_SESSION_PROTOCOL_VERSION.to_owned(),
            device_id: device.id,
            token,
            expires_at_ms,
        })
    }

    fn authenticate_device_session(
        &mut self,
        token: &str,
        nonce: &str,
    ) -> std::result::Result<String, DeviceAuthError> {
        self.cleanup_expired_device_auth();
        validate_device_auth_nonce(nonce)?;
        let token_hash = sha256_bytes(token.as_bytes());
        let (device_id, session_epoch) = self
            .sessions
            .get(&token_hash)
            .map(|session| (session.device_id.clone(), session.session_epoch))
            .ok_or_else(|| {
                DeviceAuthError::new(
                    401,
                    "device_session_invalid",
                    "device session is missing or expired",
                )
            })?;
        let device = self.devices.get(&device_id).ok_or_else(|| {
            DeviceAuthError::new(401, "device_revoked", "device has been removed")
        })?;
        if device.approval != "approved" || !device.enabled || device.session_epoch != session_epoch
        {
            return Err(DeviceAuthError::new(
                401,
                "device_revoked",
                "device is not approved, is disabled, or its sessions were revoked",
            ));
        }
        let session = self
            .sessions
            .get_mut(&token_hash)
            .expect("device session was resolved above");
        if session.used_nonces.len() >= DEVICE_SESSION_MAX_NONCES {
            self.sessions.remove(&token_hash);
            return Err(DeviceAuthError::new(
                401,
                "device_session_nonce_capacity",
                "device session nonce budget is exhausted; create a new session",
            ));
        }
        if !session.used_nonces.insert(nonce.to_owned()) {
            return Err(DeviceAuthError::new(
                409,
                "device_request_replayed",
                "device request nonce has already been used",
            ));
        }
        Ok(device_id)
    }

    fn authorized_keyed_device(
        &self,
        device_id: &str,
    ) -> std::result::Result<&ManagedDevice, DeviceAuthError> {
        let device = self
            .devices
            .get(device_id)
            .ok_or_else(|| DeviceAuthError::new(404, "device_not_found", "device was not found"))?;
        if device.approval != "approved" || !device.enabled {
            return Err(DeviceAuthError::new(
                403,
                "device_not_authorized",
                "device is not approved and enabled",
            ));
        }
        if device.public_key.is_none() {
            return Err(DeviceAuthError::new(
                409,
                "device_key_missing",
                "device has no paired public key",
            ));
        }
        Ok(device)
    }

    fn cleanup_expired_device_auth(&mut self) {
        let now = unix_time_millis();
        self.challenges
            .retain(|_, challenge| challenge.expires_at_ms > now);
        self.sessions
            .retain(|_, session| session.expires_at_ms > now);
    }

    fn revoke_device_sessions(&mut self, device_id: &str) {
        self.sessions
            .retain(|_, session| session.device_id != device_id);
        self.challenges
            .retain(|_, challenge| challenge.device_id != device_id);
    }
}
