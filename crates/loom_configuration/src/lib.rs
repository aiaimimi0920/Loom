#![forbid(unsafe_code)]

mod adapters;
mod app;
mod document;
mod error;
mod html;
mod metadata;
mod store;

pub use adapters::{
    built_in_registry, ConfigAdapter, ConfigRegistry, HookVoiceAdapter, TalkVoiceAdapter,
    TeaConfigAdapter, VoiceConfig,
};
pub use app::{ManagedAppId, ManagedAppSet};
pub use document::{ManagedConfigDocument, ManagedDocumentMetadata};
pub use error::{ManagedConfigError, ManagedConfigErrorCode, ValidationError};
pub use html::{render_app_settings_page, render_settings_index};
pub use metadata::{UiField, UiFieldKind, UiFieldOption, UiSection};
pub use store::{default_configuration_root, FileDocumentStore};

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn app_ids_parse_case_insensitively() {
        assert_eq!("tea".parse::<ManagedAppId>().unwrap(), ManagedAppId::Tea);
        assert_eq!("HOOK".parse::<ManagedAppId>().unwrap(), ManagedAppId::Hook);
        assert_eq!("Talk".parse::<ManagedAppId>().unwrap(), ManagedAppId::Talk);
        assert!("gateway".parse::<ManagedAppId>().is_err());
    }

    #[test]
    fn managed_app_set_parses_comma_separated_env() {
        let set = ManagedAppSet::parse(" hook, tea ,,talk ");
        assert!(set.contains(ManagedAppId::Tea));
        assert!(set.contains(ManagedAppId::Hook));
        assert!(set.contains(ManagedAppId::Talk));
        assert!(!ManagedAppSet::parse("").contains(ManagedAppId::Tea));
    }

    #[test]
    fn new_document_starts_at_revision_one() {
        let document = ManagedConfigDocument::new(
            ManagedAppId::Tea,
            1,
            json!({
                "notifications_enabled": true,
                "human_ticket_default_approval_policy": "human_before_execute",
                "hook_ticket_default_approval_policy": "plan_only"
            }),
        );

        assert_eq!(document.document_version, 1);
        assert_eq!(document.app, ManagedAppId::Tea);
        assert_eq!(document.schema_version, 1);
        assert_eq!(document.revision, 1);
        assert_eq!(document.source_of_truth, "loom");
        assert!(document.updated_at.ends_with('Z'));
    }

    #[test]
    fn document_update_requires_matching_revision() {
        let mut document = ManagedConfigDocument::new(ManagedAppId::Tea, 1, json!({ "a": true }));
        let error = document
            .replace_config(99, json!({ "a": false }))
            .expect_err("stale revision fails");
        assert_eq!(error.code(), ManagedConfigErrorCode::RevisionConflict);

        document
            .replace_config(1, json!({ "a": false }))
            .expect("matching revision updates");
        assert_eq!(document.revision, 2);
        assert_eq!(document.config, json!({ "a": false }));
    }
}
