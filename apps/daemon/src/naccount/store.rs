use super::{error, Result};
use loom_tool_registry::credentials::{
    CredentialInput, CredentialScope, CredentialStore, CredentialValueType,
};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Default, Serialize, Deserialize)]
pub(super) struct Account {
    pub client_id: String,
    pub state: String,
    pub verifier: String,
    pub started: u64,
    pub access_token: String,
    pub refresh_token: String,
    pub principal: String,
    pub name: String,
}

pub(super) struct Store(CredentialStore);
impl Store {
    pub fn new(root: &Path) -> Self {
        Self(CredentialStore::new(root.join("naccount-login")))
    }
    pub fn read(&self) -> Result<Option<Account>> {
        self.0
            .reveal("naccount", &CredentialScope::default())
            .map_err(|_| error(500, "naccount_store_unavailable"))?
            .map(|record| {
                serde_json::from_str(&record.value)
                    .map_err(|_| error(500, "naccount_store_invalid"))
            })
            .transpose()
    }
    pub fn save(&self, account: &Account) -> Result<()> {
        if !cfg!(windows) {
            return Err(error(501, "naccount_storage_unavailable"));
        }
        self.0
            .upsert(CredentialInput {
                name: "naccount".into(),
                value: serde_json::to_string(account)
                    .map_err(|_| error(500, "naccount_store_invalid"))?,
                value_type: CredentialValueType::Json,
                scope: CredentialScope::default(),
                expires_at: None,
            })
            .map_err(|_| error(500, "naccount_store_unavailable"))?;
        Ok(())
    }
    pub fn clear(&self) -> Result<()> {
        self.0
            .delete("naccount", &CredentialScope::default())
            .map_err(|_| error(500, "naccount_store_unavailable"))?;
        Ok(())
    }
}
