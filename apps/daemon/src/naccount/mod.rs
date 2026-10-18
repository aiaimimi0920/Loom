//! NAccount 原生 OAuth 会话；不替换 Platform 投屏设备身份。
mod store;
mod transport;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use rand_core::{OsRng, RngCore};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    path::Path,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};
use store::{Account, Store};

pub(crate) const ISSUER: &str = "https://naccount-auth.aiaimimi.com";
pub(crate) const CALLBACK: &str = "http://127.0.0.1:8765/v1/naccount/callback";
const DEFAULT_CLIENT_ID: &str = "E2C4ea99CC5Fee924b9023D76986397f7593282D25AF979d727D26eaB09A0c10";
static LOCK: Mutex<()> = Mutex::new(());
#[derive(Debug)]
pub(crate) struct Error {
    pub status: u16,
    pub code: &'static str,
}
pub(crate) type Result<T> = std::result::Result<T, Error>;
pub(crate) fn error(status: u16, code: &'static str) -> Error {
    Error { status, code }
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
fn random() -> String {
    let mut bytes = [0; 32];
    OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}
fn client_id() -> Result<String> {
    let value =
        std::env::var("LOOM_NACCOUNT_CLIENT_ID").unwrap_or_else(|_| DEFAULT_CLIENT_ID.into());
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(error(503, "naccount_not_configured"));
    }
    Ok(value)
}
fn authorization(account: &Account) -> String {
    let mut url =
        reqwest::Url::parse(&format!("{ISSUER}/oauth2/v1/authorize")).expect("fixed issuer");
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(account.verifier.as_bytes()));
    url.query_pairs_mut().extend_pairs([
        ("client_id", account.client_id.as_str()),
        ("response_type", "code"),
        ("redirect_uri", CALLBACK),
        ("state", &account.state),
        ("code_challenge", &challenge),
        ("code_challenge_method", "S256"),
        ("scope", "openid profile offline_access"),
        ("authorize_method", "redirect"),
        ("policy", "sign_in_or_sign_up"),
    ]);
    url.into()
}
fn view(account: Option<&Account>) -> Value {
    match account {
        Some(a) if !a.access_token.is_empty() => {
            json!({"status":"signed_in", "principal":a.principal, "name":a.name})
        }
        Some(a) => {
            json!({"status":"pending", "requestId":a.state, "authorizationUrl":authorization(a)})
        }
        None => json!({"status":"signed_out", "configured":client_id().is_ok()}),
    }
}
fn accept(account: &mut Account, token: Value) -> Result<()> {
    let access = token["access_token"]
        .as_str()
        .filter(|s| !s.is_empty() && s.len() <= 8192)
        .ok_or(error(502, "naccount_response_invalid"))?;
    let user = transport::profile(access)?;
    let principal = user["authId"]
        .as_str()
        .filter(|s| !s.is_empty() && s.len() <= 200)
        .ok_or(error(502, "naccount_response_invalid"))?;
    if !account.principal.is_empty() && account.principal != principal {
        return Err(error(401, "naccount_identity_changed"));
    }
    account.principal = principal.into();
    account.name = user["email"]
        .as_str()
        .unwrap_or(principal)
        .chars()
        .take(200)
        .collect();
    account.access_token = access.into();
    if let Some(refresh) = token["refresh_token"].as_str() {
        if refresh.is_empty() || refresh.len() > 8192 {
            return Err(error(502, "naccount_response_invalid"));
        }
        account.refresh_token = refresh.into();
    }
    account.verifier.clear();
    account.state.clear();
    Ok(())
}
pub(crate) fn handle(root: &Path, action: &str) -> Result<Value> {
    let _lock = LOCK.lock().map_err(|_| error(503, "naccount_busy"))?;
    let store = Store::new(root);
    let mut current = store.read()?;
    if current
        .as_ref()
        .is_some_and(|a| a.access_token.is_empty() && now().saturating_sub(a.started) >= 600)
    {
        store.clear()?;
        current = None;
    }
    match action {
        "status" => Ok(view(current.as_ref())),
        "start" => {
            if current.is_some() {
                return Err(error(409, "naccount_logout_required"));
            }
            let account = Account {
                client_id: client_id()?,
                state: random(),
                verifier: random(),
                started: now(),
                ..Default::default()
            };
            store.save(&account)?;
            Ok(view(Some(&account)))
        }
        "refresh" => {
            crate::asset_library::cancel_account_downloads(root);
            let a = current.as_mut().ok_or(error(401, "naccount_signed_out"))?;
            if a.refresh_token.is_empty() {
                store.clear()?;
                return Ok(view(None));
            }
            let result = transport::token(&[
                ("grant_type", "refresh_token"),
                ("refresh_token", &a.refresh_token),
            ])
            .and_then(|token| accept(a, token));
            if result.as_ref().is_err_and(|e| e.status == 401) {
                store.clear()?;
                return Ok(view(None));
            }
            result?;
            store.save(a)?;
            Ok(view(Some(a)))
        }
        "logout" => {
            crate::asset_library::cancel_account_downloads(root);
            let revoked = current.as_ref().is_none_or(|a| {
                a.refresh_token.is_empty() || transport::revoke(&a.client_id, &a.refresh_token)
            });
            store.clear()?;
            Ok(
                json!({"status":"signed_out", "remoteRevoked":revoked, "configured":client_id().is_ok()}),
            )
        }
        _ => Err(error(404, "naccount_route_unknown")),
    }
}
fn callback_code(path: &str, account: &Account, time: u64) -> Result<String> {
    let url = reqwest::Url::parse(&format!("http://127.0.0.1:8765{path}"))
        .map_err(|_| error(400, "naccount_callback_invalid"))?;
    let pairs: Vec<_> = url.query_pairs().collect();
    let values = |key: &str| {
        pairs
            .iter()
            .filter(|(name, _)| name == key)
            .map(|(_, value)| value.to_string())
            .collect::<Vec<_>>()
    };
    let states = values("state");
    let mut codes = values("code");
    if url.path() != "/v1/naccount/callback"
        || !values("error").is_empty()
        || states.len() != 1
        || codes.len() != 1
        || states[0] != account.state
        || account.state.is_empty()
        || codes[0].is_empty()
        || codes[0].len() > 4096
        || time < account.started
        || time - account.started >= 600
        || !account.access_token.is_empty()
    {
        return Err(error(400, "naccount_callback_invalid"));
    }
    Ok(codes.remove(0))
}
pub(crate) fn callback(root: &Path, path: &str) -> Result<Value> {
    let _lock = LOCK.lock().map_err(|_| error(503, "naccount_busy"))?;
    let store = Store::new(root);
    let mut a = store
        .read()?
        .ok_or(error(400, "naccount_callback_invalid"))?;
    let code = callback_code(path, &a, now())?;
    // 授权码消费结果未知时要求重新登录，不重复兑换。
    store.clear()?;
    let token = transport::token(&[
        ("grant_type", "authorization_code"),
        ("code", &code),
        ("redirect_uri", CALLBACK),
        ("code_verifier", &a.verifier),
    ])?;
    accept(&mut a, token)?;
    store.save(&a)?;
    Ok(json!({"status":"signed_in", "message":"登录成功，请返回 Loom。"}))
}
pub(crate) fn access_token(root: &Path) -> Result<String> {
    let a = Store::new(root)
        .read()?
        .ok_or(error(401, "naccount_signed_out"))?;
    if a.access_token.is_empty() {
        return Err(error(401, "naccount_signed_out"));
    }
    Ok(a.access_token)
}

// 与退出/刷新使用同一把锁，使凭据读取和短时任务注册不可被账号切换穿插。
pub(crate) fn with_access_token<T>(
    root: &Path,
    action: impl FnOnce(String) -> Result<T>,
) -> Result<T> {
    let _lock = LOCK.lock().map_err(|_| error(503, "naccount_busy"))?;
    action(access_token(root)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn naccount_pkce_and_callback_contract() {
        let a = Account {
            client_id: "loom".into(),
            state: random(),
            verifier: random(),
            started: 100,
            ..Default::default()
        };
        let url = reqwest::Url::parse(&authorization(&a)).unwrap();
        assert_eq!(url.origin().ascii_serialization(), ISSUER);
        assert!(url
            .query_pairs()
            .any(|(k, v)| k == "redirect_uri" && v == CALLBACK));
        let path = format!("/v1/naccount/callback?state={}&code=one", a.state);
        assert_eq!(callback_code(&path, &a, 101).unwrap(), "one");
        assert!(callback_code(&path, &a, 700).is_err());
        assert!(callback_code(&format!("{path}&code=two"), &a, 101).is_err());
        assert!(callback_code("/v1/naccount/callback?state=wrong&code=x", &a, 101).is_err());
        let visible = view(Some(&a)).to_string();
        assert!(!visible.contains(&a.verifier));
    }
}
