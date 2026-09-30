//! Groups expose only members present in this caller's bounded target directory.
use super::{DeviceRef, Document};
use serde_json::{json, Value};
use std::collections::BTreeMap;

pub(crate) fn append_groups(value: &mut Value, document: &Document) {
    let targets: BTreeMap<DeviceRef, String> = value["targets"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|target| {
            let id = target["deviceId"].as_str()?;
            let identity = match target["route"].as_str()? {
                "shared_loom" => DeviceRef {
                    device_id: id.to_owned(),
                    peer_id: None,
                },
                "offline_peer" if target["deliveryAvailable"] == true => DeviceRef {
                    device_id: target["remoteDeviceId"].as_str()?.to_owned(),
                    peer_id: Some(target["peerId"].as_str()?.to_owned()),
                },
                _ => return None,
            };
            Some((identity, id.to_owned()))
        })
        .collect();
    value["groups"] = json!(document
        .groups
        .iter()
        .map(|group| {
            let ids: Vec<_> = group
                .members
                .iter()
                .filter_map(|member| targets.get(member))
                .collect();
            json!({
                "groupId": group.group_id,
                "name": group.name,
                "targetIds": ids,
                "unavailableCount": group.members.len() - ids.len(),
            })
        })
        .collect::<Vec<_>>());
    value["settingsRevision"] = json!(document.revision);
    value["friends"] = json!({
        "status": "unavailable", "reason": "official_account_not_implemented"
    });
}
