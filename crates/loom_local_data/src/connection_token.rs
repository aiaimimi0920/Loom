//! Remote credentials carry their destination; split profile writes can never retarget a secret.
use super::*;
use std::sync::Mutex;

pub(super) static UPDATE_LOCK: Mutex<()> = Mutex::new(());
// Newlines are forbidden in legacy bearer tokens, so this marker is unambiguous.
const RECORD_PREFIX: &str = "LOOM-BOUND-TOKEN-V1\n";

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BoundToken {
    origin: String,
    token: String,
}

impl LoomConnectionStore {
    pub(super) fn read_token_for_origin(&self, origin: &str) -> Result<Option<String>, String> {
        // JSON escaping and the origin are bounded separately from the bearer itself.
        let Some(bytes) = read_bounded(&self.remote_token_path(), MAX_CONNECTION_FILE_BYTES)
            .map_err(|error| format!("Failed to read remote connection token: {error}"))?
        else {
            return Ok(None);
        };
        let raw = std::str::from_utf8(&bytes)
            .map_err(|_| "Remote connection token is not valid UTF-8.".to_owned())?
            .trim();
        if raw.is_empty() {
            return Ok(None);
        }
        if let Some(document) = raw.strip_prefix(RECORD_PREFIX) {
            let bound: BoundToken = serde_json::from_str(document)
                .map_err(|_| "Invalid remote connection credential document.".to_owned())?;
            if bound.origin != origin {
                return Ok(None);
            }
            validate_token(&bound.token)?;
            return Ok(Some(bound.token));
        }
        // Legacy plaintext belongs only to the currently persisted profile. The next
        // save always rewrites it as a bound record before changing that profile.
        let profile = self.load_profile()?;
        if profile
            .remote_url
            .as_deref()
            .map(|url| parse_loom_endpoint_url(url, true).map(|ep| ep.origin()))
            .transpose()?
            .as_deref()
            != Some(origin)
        {
            return Ok(None);
        }
        validate_token(raw)?;
        Ok(Some(raw.to_owned()))
    }

    pub(super) fn save_bound_token(
        &self,
        origin: Option<&str>,
        replacement: Option<&str>,
        clear: bool,
    ) -> Result<(), String> {
        let replacement = replacement.map(str::trim).filter(|token| !token.is_empty());
        if let Some(token) = replacement {
            validate_token(token)?;
        }
        let token = if clear {
            None
        } else if let Some(token) = replacement {
            Some(token.to_owned())
        } else {
            match origin {
                Some(origin) => self.read_token_for_origin(origin)?,
                None => None,
            }
        };
        let encoded = match (origin, token) {
            (Some(origin), Some(token)) => {
                let mut bytes = RECORD_PREFIX.as_bytes().to_vec();
                bytes.extend(
                    serde_json::to_vec(&BoundToken {
                        origin: origin.to_owned(),
                        token,
                    })
                    .map_err(|error| error.to_string())?,
                );
                bytes
            }
            // An atomic empty record also fails closed if saving the profile then fails.
            _ => Vec::new(),
        };
        if encoded.len() > MAX_CONNECTION_FILE_BYTES {
            return Err("Remote connection credential exceeds the file size limit.".to_owned());
        }
        write_private_atomic(&self.remote_token_path(), &encoded)
            .map_err(|error| format!("Failed to save remote connection token: {error}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_credentials_never_follow_a_changed_origin_and_maximum_token_roundtrips() {
        let root = std::env::temp_dir().join(format!(
            "loom-bound-token-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = LoomConnectionStore::new(&root);
        let a = "http://192.168.1.10:8765";
        let b = "http://192.168.1.11:8765";
        let update = |url: &str, token: Option<String>| LoomConnectionUpdate {
            mode: ConnectionMode::Remote,
            remote_url: Some(url.to_owned()),
            remote_token: token,
            ..Default::default()
        };
        let token = "x".repeat(MAX_TOKEN_BYTES);
        store
            .save_update(update(a, Some(token.clone())), "http://127.0.0.1:8765")
            .unwrap();
        assert_eq!(store.load_remote_token_for(a).unwrap(), Some(token));
        assert!(store.load_remote_token_for(b).is_err());
        // Reject the whole update before changing either the profile or the bearer on disk.
        let original_files = || {
            (
                std::fs::read(store.profile_path()).unwrap(),
                std::fs::read(store.remote_token_path()).unwrap(),
            )
        };
        let before = original_files();
        let oversized = LoomConnectionUpdate {
            server_label: Some("x".repeat(MAX_CONNECTION_FILE_BYTES)),
            ..update(b, Some("replacement".into()))
        };
        assert!(store
            .save_update(oversized, "http://127.0.0.1:8765")
            .is_err());
        assert_eq!(original_files(), before);
        store
            .save_update(update(b, None), "http://127.0.0.1:8765")
            .unwrap();
        assert_eq!(store.load_remote_token_for(b).unwrap(), None);
        // A previously valid opaque bearer can start with a JSON-looking character.
        write_private_atomic(&store.remote_token_path(), b"{legacy-token}\n").unwrap();
        assert_eq!(
            store.load_remote_token_for(b).unwrap().as_deref(),
            Some("{legacy-token}")
        );
        assert!(store.load_remote_token_for(a).is_err());
        // Simulate token replacement reaching disk before the profile commit.
        store
            .save_bound_token(Some(a), Some("secret-a"), false)
            .unwrap();
        assert_eq!(store.load_remote_token_for(b).unwrap(), None);
        std::fs::remove_dir_all(root).unwrap();
    }
}
