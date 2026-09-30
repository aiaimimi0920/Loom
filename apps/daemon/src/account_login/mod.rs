//! Loom owns browser authorization and the device key. Only safe views reach its UI.
mod model;
mod store;
mod transport;

use model::Account;
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};
use store::Store;

#[derive(Debug)]
pub(crate) struct Error {
    pub status: u16,
    pub code: &'static str,
}
type Result<T> = std::result::Result<T, Error>;
fn error(status: u16, code: &'static str) -> Error {
    Error { status, code }
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Start {
    origin: String,
    device_name: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Poll {
    request_id: String,
}

pub(crate) fn handle(root: &Path, action: &str, body: &str) -> Result<Value> {
    if body.len() > 4096 {
        return Err(error(413, "account_request_budget"));
    }
    let store = Store::new(root);
    let mut current = store.read()?;
    let time = now();
    if current.as_ref().is_some_and(|a| {
        a.session
            .as_ref()
            .map_or(a.grant_expires_at_ms, |s| s.expires_at_ms)
            <= time
    }) {
        store.clear()?;
        current = None;
    }
    match action {
        "start" => {
            if current.is_some() {
                return Err(error(409, "account_logout_required"));
            }
            let input: Start =
                serde_json::from_str(body).map_err(|_| error(400, "account_request_invalid"))?;
            let name = input.device_name.trim();
            if name.is_empty()
                || name.encode_utf16().count() > 80
                || name.chars().any(char::is_control)
            {
                return Err(error(400, "account_device_name_invalid"));
            }
            let account = Account::new(transport::origin(&input.origin)?, name.to_owned(), time);
            store.save(&account)?;
            account.view()
        }
        "status" => current
            .as_ref()
            .map_or(Ok(json!({"status":"signed_out"})), Account::view),
        "poll" => {
            let input: Poll =
                serde_json::from_str(body).map_err(|_| error(400, "account_request_invalid"))?;
            let account = current.as_mut().ok_or(error(401, "account_signed_out"))?;
            if account.request_id != input.request_id {
                return Err(error(409, "account_request_stale"));
            }
            if account.session.is_some() || time < account.next_poll_at_ms {
                return account.view();
            }
            account.next_poll_at_ms = time + 5_000;
            store.save(account)?;
            let response = transport::post(&account.origin, "exchange", account.exchange()?)?;
            match response.get("status").and_then(Value::as_str) {
                Some("pending") => {}
                Some("signed_in") => {
                    account.accept(response["session"].clone(), time)?;
                    store.save(account)?;
                }
                _ => return Err(error(502, "account_response_invalid")),
            }
            account.view()
        }
        "refresh" => {
            let Some(account) = current.as_mut() else {
                return Ok(json!({"status":"signed_out"}));
            };
            if account.session.is_none() {
                return account.view();
            }
            let response =
                match transport::post(&account.origin, "status", account.proof("status", time)?) {
                    Err(Error { status: 401, .. }) => {
                        store.clear()?;
                        return Ok(json!({"status":"signed_out"}));
                    }
                    result => result?,
                };
            if response["status"] != "signed_in" {
                return Err(error(502, "account_response_invalid"));
            }
            account.accept(response["session"].clone(), time)?;
            store.save(account)?;
            account.view()
        }
        "logout" => {
            let mut remote_revoked = true;
            if let Some(account) = current.as_ref().filter(|a| a.session.is_some()) {
                remote_revoked = match transport::post(
                    &account.origin,
                    "revoke",
                    account.proof("revoke", time)?,
                ) {
                    Ok(value) => value["status"] == "signed_out",
                    Err(Error { status: 401, .. }) => true,
                    Err(_) => false,
                };
            }
            store.clear()?;
            Ok(json!({"status":"signed_out", "remoteRevoked":remote_revoked}))
        }
        _ => Err(error(404, "account_route_unknown")),
    }
}

pub(crate) fn projection_identity(root: &Path) -> Result<loom_projection::Identity> {
    let account = Store::new(root)
        .read()?
        .ok_or(error(401, "account_signed_out"))?;
    let session = account
        .session
        .clone()
        .ok_or(error(401, "account_signed_out"))?;
    let key = account.key()?;
    let identity = loom_projection::Identity::new(
        account.origin,
        loom_projection::AccountSession {
            protocol: session.protocol,
            device_id: session.device_id,
            account_id: session.account_id,
            username: session.username,
            device_name: session.device_name,
            public_key: session.public_key,
            expires_at_ms: session.expires_at_ms,
        },
        key,
        now(),
    )
    .map_err(|identity_error| error(identity_error.status, identity_error.code))?;
    Ok(identity)
}

#[cfg(test)]
mod tests;
