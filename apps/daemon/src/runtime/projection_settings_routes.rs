fn projection_receive_decision(registry: &DeviceRegistryStore, target: &str, source: &str, peer: Option<&str>)
    -> std::result::Result<projection_settings::Decision, ProjectionError> {
    registry.projection_settings.decision(target, &projection_settings::DeviceRef {
        device_id: source.to_owned(), peer_id: peer.map(str::to_owned),
    }, None) // Official user identity is not yet available; never match a self-asserted user.
}

fn projection_receive_allowed(registry: &DeviceRegistryStore, target: &str, source: &str, peer: Option<&str>)
    -> std::result::Result<(), ProjectionError> {
    if projection_receive_decision(registry, target, source, peer)? == projection_settings::Decision::Reject {
        return Err(ProjectionError::new(403, "projection_receiver_rejected"));
    }
    Ok(())
}

fn route_projection_settings(request: &ParsedHttpRequest, registry: &SharedDeviceRegistryStore) -> Result<(u16, String)> {
    let result = (|| -> std::result::Result<Value, ProjectionError> {
        let mut registry = registry.lock().map_err(|_| ProjectionError::new(503, "projection_unavailable"))?;
        match request.method.as_str() {
            "GET" => {
                let mut value = json!(registry.projection_settings.view()?);
                value["targets"] = json!(registry.devices.values().filter(|device| registry.authorized_keyed_device(&device.id).is_ok())
                    .take(64).map(|device| json!({"deviceId":device.id,"name":device.name,"route":"shared_loom","policy":"confirm"})).collect::<Vec<_>>());
                Ok(value)
            }
            "PUT" => {
                if request.body.len() > 262_144 { return Err(ProjectionError::new(413, "projection_settings_capacity")); }
                let input: projection_settings::Update = parse_projection_body(&request.body)?;
                for rule in &input.rules {
                    registry.authorized_keyed_device(&rule.device_id).map_err(|_| ProjectionError::new(400, "projection_settings_target_invalid"))?;
                }
                Ok(json!(registry.projection_settings.update(input)?))
            }
            _ => Err(ProjectionError::new(405, "projection_settings_method_not_allowed")),
        }
    })();
    match result {
        Ok(value) => Ok((200, serde_json::to_string(&value)?)),
        Err(error) => structured_error(error.status, json!({"code": error.code, "message": error.code})),
    }
}
