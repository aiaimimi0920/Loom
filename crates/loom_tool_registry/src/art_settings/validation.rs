use loom_protocol::is_safe_package_id;

use super::model::{
    ArtSettingsError, ArtSettingsFile, ArtUserSettings, MAX_ART_SETTINGS_COUNT,
    MAX_ART_SETTINGS_DEPTH, MAX_ART_SETTING_ENTRIES, MAX_ART_SETTING_TEXT_BYTES,
    MAX_ART_SETTING_VALUE_BYTES,
};

pub(super) fn validate_file(file: &ArtSettingsFile) -> Result<(), ArtSettingsError> {
    if file.arts.len() > MAX_ART_SETTINGS_COUNT {
        return Err(ArtSettingsError::InvalidDocument(format!(
            "contains more than {MAX_ART_SETTINGS_COUNT} Arts"
        )));
    }
    for (art_id, settings) in &file.arts {
        validate_art_reference(art_id)?;
        validate_settings(settings)?;
    }
    Ok(())
}

pub(super) fn validate_art_reference(value: &str) -> Result<(), ArtSettingsError> {
    let valid = value
        .split_once('/')
        .map(|(publisher, id)| is_safe_package_id(publisher) && is_safe_package_id(id))
        .unwrap_or_else(|| is_safe_package_id(value));
    if valid {
        Ok(())
    } else {
        Err(ArtSettingsError::InvalidArtId(value.to_owned()))
    }
}

pub(super) fn validate_settings(settings: &ArtUserSettings) -> Result<(), ArtSettingsError> {
    for (label, count) in [
        ("defaults", settings.defaults.len()),
        ("valueBindings", settings.value_bindings.len()),
        ("credentialBindings", settings.credential_bindings.len()),
    ] {
        if count > MAX_ART_SETTING_ENTRIES {
            return Err(ArtSettingsError::InvalidDocument(format!(
                "{label} contains more than {MAX_ART_SETTING_ENTRIES} entries"
            )));
        }
    }
    for key in settings
        .defaults
        .keys()
        .chain(settings.value_bindings.keys())
        .chain(settings.credential_bindings.keys())
    {
        if !is_safe_package_id(key) {
            return Err(ArtSettingsError::InvalidSettingKey(key.clone()));
        }
    }
    for (key, value) in &settings.defaults {
        loom_security::json::ensure_within_limits(
            value,
            &format!("Art default `{key}`"),
            MAX_ART_SETTING_VALUE_BYTES,
            MAX_ART_SETTINGS_DEPTH,
        )
        .map_err(ArtSettingsError::InvalidDocument)?;
    }
    for credential in settings.value_bindings.values() {
        if !is_safe_package_id(credential) {
            return Err(ArtSettingsError::InvalidCredentialName(credential.clone()));
        }
    }
    for credential in settings.credential_bindings.values() {
        if !is_safe_package_id(credential) {
            return Err(ArtSettingsError::InvalidCredentialName(credential.clone()));
        }
    }
    if let Some(source) = &settings.source {
        if source.store.trim().is_empty()
            || source.store.len() > MAX_ART_SETTING_TEXT_BYTES
            || !is_safe_package_id(&source.art_id)
        {
            return Err(ArtSettingsError::InvalidSource(source.store.clone()));
        }
        if let Some(identity) = &source.qualified_id {
            validate_art_reference(identity)?;
        }
    }
    for (label, value) in [
        ("name", settings.name.as_deref()),
        ("description", settings.description.as_deref()),
    ] {
        if value.is_some_and(|value| value.len() > MAX_ART_SETTING_TEXT_BYTES) {
            return Err(ArtSettingsError::InvalidDocument(format!(
                "{label} exceeds {MAX_ART_SETTING_TEXT_BYTES} bytes"
            )));
        }
    }
    Ok(())
}
