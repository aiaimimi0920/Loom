use super::{error, model::Account, Result};
use loom_tool_registry::credentials::{
    CredentialInput, CredentialScope, CredentialStore, CredentialValueType,
};
use std::path::Path;

pub(super) struct Store(CredentialStore);

impl Store {
    pub fn new(root: &Path) -> Self {
        // A separate root keeps account keys outside all plugin credential APIs/grants.
        Self(CredentialStore::new(root.join("account-login")))
    }

    pub fn read(&self) -> Result<Option<Account>> {
        self.0
            .reveal("platform-account", &CredentialScope::default())
            .map_err(|_| error(500, "account_store_unavailable"))?
            .map(|record| {
                serde_json::from_str(&record.value).map_err(|_| error(500, "account_store_invalid"))
            })
            .transpose()
    }

    pub fn save(&self, account: &Account) -> Result<()> {
        if !cfg!(windows) {
            return Err(error(501, "account_secure_storage_unavailable"));
        }
        self.0
            .upsert(CredentialInput {
                name: "platform-account".to_owned(),
                value: serde_json::to_string(account)
                    .map_err(|_| error(500, "account_store_invalid"))?,
                value_type: CredentialValueType::Json,
                scope: CredentialScope::default(),
                expires_at: None,
            })
            .map_err(|_| error(500, "account_store_unavailable"))?;
        Ok(())
    }

    pub fn clear(&self) -> Result<()> {
        self.0
            .delete("platform-account", &CredentialScope::default())
            .map_err(|_| error(500, "account_store_unavailable"))?;
        Ok(())
    }
}
