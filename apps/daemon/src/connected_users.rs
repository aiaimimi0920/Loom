//! Login summaries are advisory metadata from the local Loom or a pinned peer, never credentials.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, path::Path};

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct User {
    pub issuer: String,
    pub account_id: String,
    pub name: String,
}
impl User {
    pub(crate) fn valid(&self) -> bool {
        [&self.issuer, &self.account_id, &self.name]
            .into_iter()
            .all(|s| !s.is_empty() && s.len() <= 256 && !s.chars().any(char::is_control))
            && reqwest::Url::parse(&self.issuer).is_ok_and(|url| {
                (url.scheme() == "https"
                    || (url.scheme() == "http"
                        && matches!(url.host_str(), Some("127.0.0.1" | "[::1]" | "localhost"))))
                    && url.origin().ascii_serialization() == self.issuer
            })
    }
    pub(crate) fn id(&self) -> String {
        format!(
            "user:{:x}",
            Sha256::digest(format!("{}\n{}", self.issuer, self.account_id))
        )
    }
}
pub(crate) fn local(root: &Path) -> anyhow::Result<Vec<User>> {
    let mut users = Vec::new();
    if let Some(user) = crate::naccount::user_summary(root).map_err(|e| anyhow::anyhow!(e.code))? {
        users.push(user);
    }
    if let Some(user) =
        crate::account_login::user_summary(root).map_err(|e| anyhow::anyhow!(e.code))?
    {
        users.push(user);
    }
    anyhow::ensure!(users.iter().all(User::valid), "account_summary_invalid");
    Ok(users)
}
pub(crate) fn initialize(value: &mut Value, root: &Path) -> anyhow::Result<()> {
    let ids: Vec<String> = value["targets"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|t| t["route"] == "shared_loom")
        .filter_map(|t| t["deviceId"].as_str().map(str::to_owned))
        .collect();
    value["friends"] = json!({"status":"complete", "users":[]});
    append(value, local(root)?, &ids);
    Ok(())
}
pub(crate) fn append(value: &mut Value, users: Vec<User>, targets: &[String]) {
    let Some(rows) = value["friends"]["users"].as_array_mut() else {
        return;
    };
    for user in users {
        let id = user.id();
        if let Some(row) = rows.iter_mut().find(|row| row["userId"] == id) {
            let mut ids: BTreeSet<String> = row["targetIds"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|id| id.as_str().map(str::to_owned))
                .collect();
            ids.extend(targets.iter().cloned());
            row["targetIds"] = json!(ids);
        } else if rows.len() < 34 {
            rows.push(
                json!({"userId":id,"issuer":user.issuer,"accountId":user.account_id,
                "name":user.name,"targetIds":targets}),
            );
        }
    }
}
