pub mod tea;
pub mod voice;

use serde_json::Value;
use std::collections::BTreeMap;

use crate::{ManagedAppId, ManagedConfigError, UiSection};

pub use tea::TeaConfigAdapter;
pub use voice::{HookVoiceAdapter, TalkVoiceAdapter, VoiceConfig};

pub trait ConfigAdapter: Send + Sync {
    fn app(&self) -> ManagedAppId;
    fn display_name(&self) -> &'static str;
    fn schema_version(&self) -> u32;
    fn default_config(&self) -> Value;
    fn normalize_and_validate(&self, value: Value) -> Result<Value, ManagedConfigError>;
    fn ui_sections(&self, value: &Value) -> Vec<UiSection>;
}

#[derive(Default)]
pub struct ConfigRegistry {
    adapters: BTreeMap<ManagedAppId, Box<dyn ConfigAdapter>>,
}

impl ConfigRegistry {
    pub fn new(adapters: Vec<Box<dyn ConfigAdapter>>) -> Self {
        let adapters = adapters
            .into_iter()
            .map(|adapter| (adapter.app(), adapter))
            .collect();
        Self { adapters }
    }

    pub fn get(&self, app: ManagedAppId) -> Option<&dyn ConfigAdapter> {
        self.adapters.get(&app).map(Box::as_ref)
    }

    pub fn apps(&self) -> Vec<ManagedAppId> {
        self.adapters.keys().copied().collect()
    }
}

pub fn built_in_registry() -> ConfigRegistry {
    ConfigRegistry::new(vec![
        Box::new(TeaConfigAdapter),
        Box::new(HookVoiceAdapter),
        Box::new(TalkVoiceAdapter),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn registry_contains_tea_hook_and_talk() {
        let registry = built_in_registry();
        assert!(registry.get(ManagedAppId::Tea).is_some());
        assert!(registry.get(ManagedAppId::Hook).is_some());
        assert!(registry.get(ManagedAppId::Talk).is_some());
    }

    #[test]
    fn tea_adapter_validates_policy_values() {
        let adapter = TeaConfigAdapter;
        let valid = adapter
            .normalize_and_validate(json!({
                "notifications_enabled": true,
                "human_ticket_default_approval_policy": "human_before_execute",
                "hook_ticket_default_approval_policy": "plan_only"
            }))
            .expect("valid Tea config");
        assert_eq!(valid["notifications_enabled"], true);

        let error = adapter
            .normalize_and_validate(json!({
                "notifications_enabled": true,
                "human_ticket_default_approval_policy": "bad",
                "hook_ticket_default_approval_policy": "plan_only"
            }))
            .expect_err("invalid policy fails");
        assert_eq!(
            error.code(),
            crate::ManagedConfigErrorCode::InvalidConfiguration
        );
        assert_eq!(
            error.validation_errors()[0].field,
            "human_ticket_default_approval_policy"
        );
    }

    #[test]
    fn voice_adapter_validates_provider_requirements() {
        let adapter = HookVoiceAdapter;
        let mut config = adapter.default_config();
        config["provider"]["mock_transcript"] = json!("");
        let error = adapter
            .normalize_and_validate(config)
            .expect_err("empty mock transcript fails");
        assert_eq!(
            error.validation_errors()[0].field,
            "provider.mock_transcript"
        );
    }

    #[test]
    fn hook_and_talk_keep_separate_app_ids_with_shared_voice_shape() {
        let hook = HookVoiceAdapter;
        let talk = TalkVoiceAdapter;
        assert_eq!(hook.app(), ManagedAppId::Hook);
        assert_eq!(talk.app(), ManagedAppId::Talk);
        assert_eq!(hook.schema_version(), talk.schema_version());
        assert!(hook.default_config().get("voice_mode").is_some());
        assert!(talk.default_config().get("voice_mode").is_some());
    }
}
