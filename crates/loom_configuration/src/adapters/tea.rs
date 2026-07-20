use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::ConfigAdapter;
use crate::{
    ManagedAppId, ManagedConfigError, UiField, UiFieldKind, UiFieldOption, UiSection,
    ValidationError,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeaManagedConfig {
    pub notifications_enabled: bool,
    pub human_ticket_default_approval_policy: String,
    pub hook_ticket_default_approval_policy: String,
}

impl Default for TeaManagedConfig {
    fn default() -> Self {
        Self {
            notifications_enabled: true,
            human_ticket_default_approval_policy: "human_before_execute".to_string(),
            hook_ticket_default_approval_policy: "plan_only".to_string(),
        }
    }
}

pub struct TeaConfigAdapter;

impl ConfigAdapter for TeaConfigAdapter {
    fn app(&self) -> ManagedAppId {
        ManagedAppId::Tea
    }

    fn display_name(&self) -> &'static str {
        "Tea"
    }

    fn schema_version(&self) -> u32 {
        1
    }

    fn default_config(&self) -> Value {
        serde_json::to_value(TeaManagedConfig::default()).expect("serialize Tea default config")
    }

    fn normalize_and_validate(&self, value: Value) -> Result<Value, ManagedConfigError> {
        let config: TeaManagedConfig = serde_json::from_value(value).map_err(|error| {
            ManagedConfigError::invalid(vec![ValidationError::new("$", error.to_string())])
        })?;
        validate_policy(
            "human_ticket_default_approval_policy",
            &config.human_ticket_default_approval_policy,
        )?;
        validate_policy(
            "hook_ticket_default_approval_policy",
            &config.hook_ticket_default_approval_policy,
        )?;
        serde_json::to_value(config).map_err(|error| {
            ManagedConfigError::invalid(vec![ValidationError::new("$", error.to_string())])
        })
    }

    fn ui_sections(&self, value: &Value) -> Vec<UiSection> {
        vec![UiSection {
            title: "Tea defaults".to_string(),
            fields: vec![
                UiField {
                    path: "notifications_enabled".to_string(),
                    label: "Notifications and UI hints".to_string(),
                    kind: UiFieldKind::Boolean,
                    options: Vec::new(),
                    value: value.get("notifications_enabled").cloned(),
                },
                policy_field(
                    value,
                    "human_ticket_default_approval_policy",
                    "Human ticket approval",
                ),
                policy_field(
                    value,
                    "hook_ticket_default_approval_policy",
                    "Hook ticket approval",
                ),
            ],
        }]
    }
}

fn validate_policy(field: &str, value: &str) -> Result<(), ManagedConfigError> {
    if policy_options().iter().any(|candidate| candidate == &value) {
        Ok(())
    } else {
        Err(ManagedConfigError::invalid(vec![ValidationError::new(
            field,
            format!("unsupported approval policy: {value}"),
        )]))
    }
}

fn policy_field(value: &Value, path: &str, label: &str) -> UiField {
    UiField {
        path: path.to_string(),
        label: label.to_string(),
        kind: UiFieldKind::Select,
        options: policy_options()
            .into_iter()
            .map(|value| UiFieldOption {
                value: value.to_string(),
                label: value.to_string(),
            })
            .collect(),
        value: value.get(path).cloned(),
    }
}

fn policy_options() -> [&'static str; 4] {
    [
        "human_before_execute",
        "human_before_completion",
        "manual_only",
        "plan_only",
    ]
}
