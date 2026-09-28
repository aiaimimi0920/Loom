use crate::ProjectionError;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct DeviceRef {
    pub device_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub peer_id: Option<String>,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Decision {
    Auto,
    Confirm,
    Reject,
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AllowList {
    pub devices: Vec<DeviceRef>,
    pub groups: Vec<String>,
    pub users: Vec<String>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Group {
    pub group_id: String,
    pub name: String,
    pub members: Vec<DeviceRef>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Rule {
    pub device_id: String,
    pub policy: Decision,
    pub whitelist: AllowList,
    pub blacklist: Vec<DeviceRef>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Document {
    pub storage_version: u32,
    pub revision: u64,
    pub groups: Vec<Group>,
    pub rules: Vec<Rule>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Update {
    pub expected_revision: u64,
    pub groups: Vec<Group>,
    pub rules: Vec<Rule>,
}

fn identifier(value: &str) -> bool {
    loom_protocol::projection::projection_identifier_valid(value)
}
fn devices_valid(devices: &[DeviceRef]) -> bool {
    devices.len() <= 64
        && devices.iter().collect::<BTreeSet<_>>().len() == devices.len()
        && devices.iter().all(|device| {
            identifier(&device.device_id)
                && device.peer_id.as_ref().is_none_or(|id| {
                    id.len() == 69
                        && id.starts_with("loom-")
                        && id[5..]
                            .bytes()
                            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
                })
        })
}
fn identifiers_valid(values: &[String]) -> bool {
    values.len() <= 64
        && values.iter().all(|value| identifier(value))
        && values.iter().collect::<BTreeSet<_>>().len() == values.len()
}
pub(super) fn validate(document: &Document) -> Result<(), ProjectionError> {
    let group_ids: BTreeSet<_> = document
        .groups
        .iter()
        .map(|group| &group.group_id)
        .collect();
    let targets: BTreeSet<_> = document.rules.iter().map(|rule| &rule.device_id).collect();
    let valid = document.storage_version == 1
        && document.revision <= 9_007_199_254_740_991
        && document.groups.len() <= 32
        && document.rules.len() <= 64
        && group_ids.len() == document.groups.len()
        && targets.len() == document.rules.len()
        && document.groups.iter().all(|group| {
            identifier(&group.group_id)
                && !group.name.trim().is_empty()
                && group.name.len() <= 128
                && !group.name.chars().any(char::is_control)
                && devices_valid(&group.members)
        })
        && document.rules.iter().all(|rule| {
            identifier(&rule.device_id)
                && devices_valid(&rule.blacklist)
                && devices_valid(&rule.whitelist.devices)
                && identifiers_valid(&rule.whitelist.groups)
                && identifiers_valid(&rule.whitelist.users)
                && rule
                    .whitelist
                    .groups
                    .iter()
                    .all(|id| group_ids.contains(id))
        });
    if !valid {
        return Err(ProjectionError::new(400, "projection_settings_invalid"));
    }
    Ok(())
}
