use base64::{
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
    Engine as _,
};
use ed25519_dalek::{Signer as _, SigningKey};
use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest as _, Sha256};

pub(super) const PROTOCOL: &str = "neuro.loom-account.v1";
pub(super) const GRANT_MS: u64 = 600_000;
pub(super) const SESSION_MS: u64 = 30 * 24 * 60 * 60_000;

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Session {
    pub protocol: String,
    pub device_id: String,
    pub account_id: String,
    pub username: String,
    pub device_name: String,
    pub public_key: String,
    pub expires_at_ms: u64,
}

// This type is persisted only in the private native store, never serialized to an API.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Account {
    pub origin: String,
    pub request_id: String,
    pub device_name: String,
    pub seed: String,
    pub verifier: String,
    pub grant_expires_at_ms: u64,
    pub next_poll_at_ms: u64,
    pub session: Option<Session>,
}

impl Account {
    pub fn new(origin: String, device_name: String, now: u64) -> Self {
        Self {
            origin,
            device_name,
            request_id: random_hex::<32>(),
            seed: STANDARD.encode(SigningKey::generate(&mut OsRng).to_bytes()),
            verifier: URL_SAFE_NO_PAD.encode(random_bytes::<32>()),
            grant_expires_at_ms: now + GRANT_MS,
            next_poll_at_ms: 0,
            session: None,
        }
    }

    pub(super) fn key(&self) -> super::Result<SigningKey> {
        let bytes = STANDARD
            .decode(&self.seed)
            .map_err(|_| super::error(500, "account_store_invalid"))?;
        let seed: [u8; 32] = bytes
            .try_into()
            .map_err(|_| super::error(500, "account_store_invalid"))?;
        Ok(SigningKey::from_bytes(&seed))
    }

    pub fn public_key(&self) -> super::Result<String> {
        Ok(STANDARD.encode(self.key()?.verifying_key().as_bytes()))
    }

    pub fn challenge(&self) -> String {
        URL_SAFE_NO_PAD.encode(Sha256::digest(self.verifier.as_bytes()))
    }

    pub fn exchange(&self) -> super::Result<Value> {
        let message = format!(
            "{PROTOCOL}\nexchange\n{}\n{}",
            self.request_id,
            self.challenge()
        );
        Ok(
            json!({"requestId": self.request_id, "codeVerifier": self.verifier,
            "signature": STANDARD.encode(self.key()?.sign(message.as_bytes()).to_bytes())}),
        )
    }

    pub fn proof(&self, action: &str, now: u64) -> super::Result<Value> {
        let session = self
            .session
            .as_ref()
            .ok_or(super::error(401, "account_signed_out"))?;
        let nonce = random_hex::<16>();
        let message = format!(
            "{PROTOCOL}\n{action}\n{}\n{now}\n{nonce}",
            session.device_id
        );
        Ok(
            json!({"deviceId": session.device_id, "timestampMs": now, "nonce": nonce,
            "signature": STANDARD.encode(self.key()?.sign(message.as_bytes()).to_bytes())}),
        )
    }

    pub fn view(&self) -> super::Result<Value> {
        if let Some(session) = &self.session {
            return Ok(json!({"status":"signed_in", "origin":self.origin, "session":session}));
        }
        let key = self.public_key()?;
        let mut url = reqwest::Url::parse(&self.origin)
            .map_err(|_| super::error(500, "account_store_invalid"))?;
        url.set_path("/loom/authorize");
        url.query_pairs_mut().extend_pairs([
            ("requestId", self.request_id.as_str()),
            ("codeChallenge", &self.challenge()),
            ("publicKey", &key),
            ("deviceName", &self.device_name),
            ("expiresAtMs", &self.grant_expires_at_ms.to_string()),
        ]);
        let fingerprint = format!(
            "{:x}",
            Sha256::digest(self.key()?.verifying_key().as_bytes())
        );
        Ok(
            json!({"status":"pending", "origin":self.origin, "requestId":self.request_id,
            "authorizationUrl":url.as_str(), "fingerprint":&fingerprint[..16],
            "expiresAtMs":self.grant_expires_at_ms}),
        )
    }

    pub fn accept(&mut self, value: Value, now: u64) -> super::Result<()> {
        let session: Session = serde_json::from_value(value)
            .map_err(|_| super::error(502, "account_response_invalid"))?;
        if session.protocol != PROTOCOL
            || uuid::Uuid::parse_str(&session.device_id).is_err()
            || session.account_id.is_empty()
            || session.account_id.len() > 160
            || session.username.len() > 640
            || session.device_name != self.device_name
            || session.public_key != self.public_key()?
            || session.expires_at_ms <= now
            || session.expires_at_ms > now + SESSION_MS + 60_000
        {
            return Err(super::error(502, "account_response_invalid"));
        }
        if self.session.as_ref().is_some_and(|old| {
            old.device_id != session.device_id || old.account_id != session.account_id
        }) {
            return Err(super::error(502, "account_identity_changed"));
        }
        self.session = Some(session);
        self.verifier.clear();
        Ok(())
    }
}

fn random_bytes<const N: usize>() -> [u8; N] {
    let mut bytes = [0; N];
    OsRng.fill_bytes(&mut bytes);
    bytes
}

fn random_hex<const N: usize>() -> String {
    random_bytes::<N>()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
