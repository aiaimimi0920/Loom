use crate::{error, validation, Content, Envelope, EnvelopeSignature, Result, Source, PROTOCOL};
use base64::{
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
    Engine as _,
};
use ed25519_dalek::{Signer as _, SigningKey};
use rand_core::{OsRng, RngCore as _};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest as _, Sha256};
use std::sync::Arc;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AccountSession {
    pub protocol: String,
    pub device_id: String,
    pub account_id: String,
    pub username: String,
    pub device_name: String,
    pub public_key: String,
    pub expires_at_ms: u64,
}

// No Serialize or Debug: signing material cannot enter an HTTP/UI response.
#[derive(Clone)]
pub struct Identity {
    pub(crate) origin: String,
    pub(crate) session: AccountSession,
    pub(crate) key: Arc<SigningKey>,
}

impl Identity {
    pub fn new(origin: String, session: AccountSession, key: SigningKey, now: u64) -> Result<Self> {
        if validation::origin(&origin).as_deref() != Ok(origin.as_str())
            || session.protocol != "neuro.loom-account.v1"
            || uuid::Uuid::parse_str(&session.device_id).is_err()
            || !validation::identifier(&session.account_id)
            || session.public_key != STANDARD.encode(key.verifying_key().as_bytes())
            || session.expires_at_ms <= now
            || !validation::revision(session.expires_at_ms)
        {
            return Err(error(401, "account_identity_invalid"));
        }
        Ok(Self {
            origin,
            session,
            key: Arc::new(key),
        })
    }

    pub fn session(&self) -> &AccountSession {
        &self.session
    }
    pub fn origin(&self) -> &str {
        &self.origin
    }

    pub fn proof(&self, payload: &str, now: u64) -> Result<Value> {
        if self.session.expires_at_ms <= now {
            return Err(error(401, "device_session_unavailable"));
        }
        if payload.len() > 12 * 1024 {
            return Err(error(413, "projection_request_budget"));
        }
        let nonce = random_hex();
        let digest = format!("{:x}", Sha256::digest(payload.as_bytes()));
        let message = format!(
            "neuro.loom-account.v1\nprojection\n{}\n{now}\n{nonce}\n{digest}",
            self.session.device_id
        );
        Ok(
            json!({"deviceId":self.session.device_id,"timestampMs":now,"nonce":nonce,"payload":payload,
            "signature":STANDARD.encode(self.key.sign(message.as_bytes()).to_bytes())}),
        )
    }

    pub fn invitation(
        &self,
        unit_id: &str,
        kind: &str,
        digest: &str,
        now: u64,
    ) -> Result<Envelope> {
        if self.session.expires_at_ms <= now {
            return Err(error(401, "device_session_unavailable"));
        }
        let mut envelope = Envelope {
            protocol: PROTOCOL.to_owned(),
            projection_id: format!("projection:{}", random_hex()),
            server_origin: self.origin.clone(),
            source: Source {
                device_id: self.session.device_id.clone(),
                account_id: self.session.account_id.clone(),
                public_key: self.session.public_key.clone(),
                session_id: uuid::Uuid::new_v4().to_string(),
                unit_id: unit_id.to_owned(),
                revision: 1,
            },
            content: Content {
                kind: kind.to_owned(),
                digest: digest.to_owned(),
            },
            expires_at_ms: (now + 300_000).min(self.session.expires_at_ms),
            nonce: random_hex(),
            signature: EnvelopeSignature {
                algorithm: "ed25519".to_owned(),
                key_id: self.session.device_id.clone(),
                value: String::new(),
            },
        };
        envelope.signature.value = URL_SAFE_NO_PAD.encode(
            self.key
                .sign(envelope.signature_message().as_bytes())
                .to_bytes(),
        );
        envelope.validate(&self.origin)?;
        Ok(envelope)
    }
}

fn random_hex() -> String {
    let mut bytes = [0; 16];
    OsRng.fill_bytes(&mut bytes);
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
