// Manifest-driven Capability Plugin settings and namespaced persistence.
fn get_capability_settings(
    qualified_id: &str,
    control_plane_root: &Path,
) -> Result<(u16, String)> {
    let registry = capability_registry(control_plane_root)?;
    let package = match settings_package(&registry, qualified_id) {
        Ok(package) => package,
        Err(error) => return capability_error_response(error),
    };
    if let Err(error) = validate_setting_definitions(&package.manifest.contributes.settings) {
        return capability_error_response(error);
    }
    let store = loom_tool_registry::capability::CapabilityConfigStore::new(control_plane_root);
    capability_response(store.read(qualified_id).map(|settings| {
        json!({
            "settings": settings,
            "fields": package.manifest.contributes.settings,
            "packageDigest": package.digest
        })
    }))
}

fn update_capability_settings(
    qualified_id: &str,
    body: &str,
    control_plane_root: &Path,
) -> Result<(u16, String)> {
    let request: UpdateCapabilityConfigRequest = match serde_json::from_str(body) {
        Ok(request) => request,
        Err(error) => {
            return capability_bad_request("invalid_capability_settings", error.to_string())
        }
    };
    let registry = capability_registry(control_plane_root)?;
    let package = match settings_package(&registry, qualified_id) {
        Ok(package) => package,
        Err(error) => return capability_error_response(error),
    };
    if request.package_digest != package.digest {
        return capability_error_response(
            loom_tool_registry::capability::CapabilityInstallError::Conflict(
                "capability package changed while editing settings".to_owned(),
            ),
        );
    }
    if let Err(error) = validate_setting_values(
        &package.manifest.contributes.settings,
        &request.values,
    ) {
        return capability_error_response(error);
    }
    let store = loom_tool_registry::capability::CapabilityConfigStore::new(control_plane_root);
    capability_response(
        store
            .write(qualified_id, request.expected_revision, request.values)
            .map(|settings| json!({ "settings": settings })),
    )
}

fn settings_package(
    registry: &loom_tool_registry::capability::CapabilityPluginRegistry,
    qualified_id: &str,
) -> std::result::Result<
    loom_tool_registry::capability::VerifiedCapabilityPackage,
    loom_tool_registry::capability::CapabilityInstallError,
> {
    use loom_tool_registry::capability::CapabilityInstallError;

    let record = registry
        .get(qualified_id)?
        .ok_or_else(|| CapabilityInstallError::NotFound(qualified_id.to_owned()))?;
    // `versions` is sorted by version *string*, so `last()` returned the lexicographically largest
    // version rather than the newest — `"1.9.0"` sorts above `"1.10.0"`. A plugin with no active
    // version therefore showed the field definitions of an arbitrary older release, and an update
    // then validated the submitted values against that release's schema.
    let digest = record
        .active_digest
        .as_deref()
        .or_else(|| record.latest_semver_digest())
        .ok_or_else(|| CapabilityInstallError::NotFound(qualified_id.to_owned()))?;
    registry.verify_installed_version(qualified_id, digest)
}

fn validate_setting_definitions(
    fields: &[loom_protocol::CapabilityContribution],
) -> std::result::Result<(), loom_tool_registry::capability::CapabilityInstallError> {
    for field in fields {
        let payload = field.payload.as_object().ok_or_else(|| invalid_setting(&field.id))?;
        let kind = payload
            .get("type")
            .and_then(Value::as_str)
            .ok_or_else(|| invalid_setting(&field.id))?;
        if !matches!(kind, "string" | "number" | "boolean" | "enum" | "json") {
            return Err(invalid_setting(&field.id));
        }
        if kind == "enum" {
            let options = payload.get("options").and_then(Value::as_array);
            if options.is_none_or(|values| {
                values.is_empty()
                    || values.len() > 128
                    || values.iter().any(|value| !value.is_string())
            }) {
                return Err(invalid_setting(&field.id));
            }
        }
    }
    Ok(())
}

fn validate_setting_values(
    fields: &[loom_protocol::CapabilityContribution],
    values: &serde_json::Map<String, Value>,
) -> std::result::Result<(), loom_tool_registry::capability::CapabilityInstallError> {
    validate_setting_definitions(fields)?;
    for (id, value) in values {
        let field = fields.iter().find(|field| field.id == *id).ok_or_else(|| {
            loom_tool_registry::capability::CapabilityInstallError::InvalidState(format!(
                "setting `{id}` is not declared by the capability package"
            ))
        })?;
        let payload = field.payload.as_object().expect("validated setting payload");
        let kind = payload["type"].as_str().expect("validated setting type");
        let valid = match kind {
            "string" => value.is_string(),
            "number" => value.is_number(),
            "boolean" => value.is_boolean(),
            "json" => true,
            "enum" => payload["options"]
                .as_array()
                .is_some_and(|options| options.contains(value)),
            _ => false,
        };
        if !valid {
            return Err(invalid_setting(id));
        }
    }
    Ok(())
}

fn invalid_setting(id: &str) -> loom_tool_registry::capability::CapabilityInstallError {
    loom_tool_registry::capability::CapabilityInstallError::InvalidPackage(format!(
        "setting `{id}` has an invalid field definition or value"
    ))
}
