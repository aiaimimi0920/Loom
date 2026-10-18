//! 控制项仅在请求生命期存在；绑定本机数据根和当前账号令牌摘要，不保存票据。
use super::{aria2_native::State, error, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{atomic::Ordering, Arc, Mutex, OnceLock},
};

type Key = (PathBuf, String);
struct Entry {
    owner: [u8; 32],
    state: Arc<State>,
}
static TASKS: OnceLock<Mutex<HashMap<Key, Entry>>> = OnceLock::new();
fn tasks() -> &'static Mutex<HashMap<Key, Entry>> {
    TASKS.get_or_init(|| Mutex::new(HashMap::new()))
}
pub(super) struct Registration {
    key: Key,
    pub(super) state: Arc<State>,
}
impl Drop for Registration {
    fn drop(&mut self) {
        if let Ok(mut tasks) = tasks().lock() {
            tasks.remove(&self.key);
        }
    }
}
fn valid_id(id: &str) -> bool {
    uuid::Uuid::parse_str(id)
        .is_ok_and(|value| value.get_version_num() == 4 && value.to_string() == id)
}
pub(super) fn register(root: &Path, id: &str, token: &str) -> Result<Registration> {
    if !valid_id(id) {
        return Err(error(400, "asset_library_task_invalid"));
    }
    let key = (root.to_path_buf(), id.to_owned());
    let mut tasks = tasks()
        .lock()
        .map_err(|_| error(503, "asset_library_engine_busy"))?;
    if tasks.contains_key(&key) {
        return Err(error(409, "asset_library_task_exists"));
    }
    if tasks.len() >= 8 {
        return Err(error(503, "asset_library_engine_busy"));
    }
    let state = Arc::new(State::default());
    tasks.insert(
        key.clone(),
        Entry {
            owner: Sha256::digest(token.as_bytes()).into(),
            state: state.clone(),
        },
    );
    Ok(Registration { key, state })
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Input {
    task_id: String,
    action: String,
}
pub(super) fn handle(root: &Path, body: &str) -> Result<Value> {
    crate::naccount::with_access_token(root, |token| control(root, body, &token))
}
pub(super) fn cancel_root(root: &Path) {
    if let Ok(tasks) = tasks().lock() {
        for (_, entry) in tasks.iter().filter(|((task_root, _), _)| task_root == root) {
            let _ =
                entry
                    .state
                    .control
                    .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                        if current == 3 {
                            None
                        } else {
                            Some(1)
                        }
                    });
        }
    }
}
pub(super) fn finish(state: &State) -> bool {
    state
        .control
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
            if current == 1 {
                None
            } else {
                Some(3)
            }
        })
        .is_ok()
}
pub(super) fn control(root: &Path, body: &str, token: &str) -> Result<Value> {
    let input: Input =
        serde_json::from_str(body).map_err(|_| error(400, "asset_library_task_invalid"))?;
    if !valid_id(&input.task_id) {
        return Err(error(400, "asset_library_task_invalid"));
    }
    let tasks = tasks()
        .lock()
        .map_err(|_| error(503, "asset_library_engine_busy"))?;
    let entry = tasks
        .get(&(root.to_path_buf(), input.task_id.clone()))
        .filter(|entry| entry.owner == <[u8; 32]>::from(Sha256::digest(token.as_bytes())))
        .ok_or_else(|| error(404, "asset_library_task_not_found"))?;
    let target = match input.action.as_str() {
        "status" => None,
        "pause" => Some(2),
        "resume" => Some(0),
        "cancel" => Some(1),
        _ => return Err(error(400, "asset_library_task_invalid")),
    };
    if let Some(target) = target {
        // 取消是终态；迟到的继续/暂停不能复活请求。
        let _ = entry
            .state
            .control
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                if current == 1 || current == 3 {
                    None
                } else {
                    Some(target)
                }
            });
    }
    let current = entry.state.control.load(Ordering::Acquire);
    if current == 3 {
        return Err(error(404, "asset_library_task_not_found"));
    }
    Ok(json!({"taskId":input.task_id, "control":current,
        "total":entry.state.total.load(Ordering::Acquire), "completed":entry.state.completed.load(Ordering::Acquire),
        "speed":entry.state.speed.load(Ordering::Acquire)}))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn control_is_owned_bounded_and_cancel_is_terminal() {
        let id = uuid::Uuid::new_v4().to_string();
        let root = Path::new("control-fixture");
        let registration = register(root, &id, "account-one").unwrap();
        let others: Vec<_> = (0..7)
            .map(|_| register(root, &uuid::Uuid::new_v4().to_string(), "account-one").unwrap())
            .collect();
        assert!(register(root, &uuid::Uuid::new_v4().to_string(), "account-one").is_err());
        drop(others);
        assert!(register(root, &id, "account-one").is_err());
        let body = |action| json!({"taskId":id,"action":action}).to_string();
        assert!(control(root, &body("status"), "account-two").is_err());
        assert!(control(Path::new("another-root"), &body("cancel"), "account-one").is_err());
        assert_eq!(
            control(root, &body("pause"), "account-one").unwrap()["control"],
            2
        );
        assert_eq!(
            control(root, &body("resume"), "account-one").unwrap()["control"],
            0
        );
        control(root, &body("cancel"), "account-one").unwrap();
        assert_eq!(
            control(root, &body("resume"), "account-one").unwrap()["control"],
            1
        );
        assert!(control(root, &body("unknown"), "account-one").is_err());
        assert!(!finish(&registration.state));
        drop(registration);
        assert!(control(root, &body("status"), "account-one").is_err());
        let registration = register(root, &id, "account-one").unwrap();
        assert!(finish(&registration.state));
        assert!(control(root, &body("cancel"), "account-one").is_err());
        cancel_root(root);
        assert_eq!(registration.state.control.load(Ordering::Acquire), 3);
        drop(registration);
        let registration = register(root, &id, "account-one").unwrap();
        cancel_root(root);
        assert_eq!(registration.state.control.load(Ordering::Acquire), 1);
    }
}
