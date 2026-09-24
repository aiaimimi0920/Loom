use super::{CapabilityCommandContribution, CapabilityPackageManifest, CapabilityValidationError};

pub(super) fn validate(
    command: &CapabilityCommandContribution,
    manifest: &CapabilityPackageManifest,
    namespace: &str,
) -> Result<(), CapabilityValidationError> {
    if command.input_context.is_some()
        && !command
            .permissions
            .iter()
            .any(|permission| permission == "hook.unit.attachments.read")
    {
        return Err(CapabilityValidationError::InvalidPermission(
            "inputContext requires hook.unit.attachments.read".to_owned(),
        ));
    }
    if let Some(type_id) = &command.toggle_attachment_type {
        if command.input_context.is_none()
            || !command
                .permissions
                .iter()
                .any(|permission| permission == "hook.unit.attachments.write")
            || !manifest
                .contributes
                .data_types
                .iter()
                .any(|item| item.id == *type_id)
            || !type_id.starts_with(namespace)
        {
            return Err(CapabilityValidationError::InvalidNamespace(type_id.clone()));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn declared_context_cannot_read_without_permission_or_toggle_foreign_data() {
        let bytes = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../capability-packages/text-translation/capability.manifest.json"
        ));
        let mut manifest = crate::parse_capability_manifest(bytes).unwrap();
        manifest.contributes.commands[0]
            .permissions
            .retain(|permission| permission != "hook.unit.attachments.read");
        assert!(crate::validate_capability_manifest(&manifest).is_err());
        let mut manifest: CapabilityPackageManifest = serde_json::from_slice(bytes).unwrap();
        manifest.contributes.commands[0].toggle_attachment_type =
            Some("neuro.official/ocr.result.v1".to_owned());
        assert!(crate::validate_capability_manifest(&manifest).is_err());
    }
}
