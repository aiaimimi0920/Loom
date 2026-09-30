// Wall configuration uses the existing administrator/device boundary, with
// exact terminal routes and ownership checks inside the independent store.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WallEndpointRegistration {
    base_revision: u64,
    endpoint: loom_protocol::wall::TileEndpoint,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WallLayoutUpdate {
    base_revision: u64,
    layout: loom_protocol::wall::WallLayout,
    #[serde(default)]
    activation_delay_ms: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WallEndpointRemoval {
    base_revision: u64,
    endpoint_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WallLayoutRemoval {
    base_revision: u64,
    wall_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WallPresentationUpdate {
    base_revision: u64,
    wall_id: String,
    mode: crate::wall_store::WallPresentationMode,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WallEndpointConnect {
    endpoint_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WallIdentificationReport {
    endpoint_id: String,
    lease_id: String,
    request_id: String,
    outcome: crate::wall_store::WallIdentificationOutcome,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WallEndpointDisconnect {
    endpoint_id: String,
    lease_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WallEndpointHeartbeat {
    endpoint_id: String,
    lease_id: String,
    sequence: u64,
    applied_revision: Option<u64>,
    presentation: Option<crate::wall_store::WallPresentationReport>,
    scene: Option<crate::wall_store::WallSceneReport>,
}

fn wall_device_route_allowed(method: &str, path: &str) -> bool {
    (method == "GET" && path == "/v1/walls/state")
        || (method == "POST"
            && matches!(
                path,
                "/v1/walls/endpoints/register"
                    | "/v1/walls/endpoints/remove"
                    | "/v1/walls/endpoints/identify/report"
                    | "/v1/walls/connect"
                    | "/v1/walls/heartbeat"
                    | "/v1/walls/disconnect"
                    | "/v1/walls/images/read"
                    | "/v1/walls/surfaces/open"
                    | "/v1/walls/surfaces/state"
                    | "/v1/walls/surfaces/close"
                    | "/v1/walls/surfaces/image"
                    | "/v1/walls/surfaces/event"
                    | "/v1/walls/surfaces/confirmation"
                    | "/v1/walls/surfaces/cancel"
                    | "/v1/walls/control/acquire"
                    | "/v1/walls/control/renew"
                    | "/v1/walls/control/release"
                    | "/v1/walls/input"
            ))
}

fn route_walls(
    request: &ParsedHttpRequest,
    path: &str,
    walls: &SharedWallStore,
    devices: &SharedDeviceRegistryStore,
    resources: &SharedSurfaceResourceStore,
    sessions: &SharedLiveSessionStore,
    actor: Option<&str>,
) -> Option<Result<(u16, String)>> {
    if path != "/v1/walls" && !path.starts_with("/v1/walls/") {
        return None;
    }
    let result = (|| -> std::result::Result<(u16, String), WallStoreError> {
        if actor.is_some() && !wall_device_route_allowed(&request.method, path) {
            return Err(WallStoreError::new(
                403,
                "wall_admin_required",
                "wall layout management requires administrator authentication",
            ));
        }
        match (request.method.as_str(), path) {
            (
                "POST",
                "/v1/walls/control/acquire"
                | "/v1/walls/control/renew"
                | "/v1/walls/control/release"
                | "/v1/walls/input",
            ) => route_wall_input(request, path, walls, sessions, devices, actor),
            ("POST", "/v1/walls/images/read") => {
                read_wall_image(&request.body, walls, resources, actor)
            }
            ("GET", "/v1/walls/state") => {
                let mut snapshot = walls.snapshot(actor)?;
                let registry = devices.lock().map_err(|_| wall_route_unavailable())?;
                for status in &mut snapshot.endpoints {
                    if !registry
                        .devices
                        .get(&status.endpoint.device_id)
                        .is_some_and(|device| device.approval == "approved" && device.enabled)
                    {
                        status.online = false;
                        status.applied_revision = None;
                        status.scene = None;
                        status.presentation = None;
                        status.identification = None;
                    }
                }
                wall_json(&snapshot)
            }
            ("POST", "/v1/walls/endpoints/register") => {
                let input: WallEndpointRegistration = parse_wall_body(&request.body)?;
                if actor.is_some_and(|id| id != input.endpoint.device_id) {
                    return Err(WallStoreError::new(
                        403,
                        "wall_endpoint_forbidden",
                        "endpoint identity differs from authenticated device",
                    ));
                }
                authorize_wall_registration(devices, &input.endpoint.device_id)?;
                wall_json(&walls.register(input.base_revision, input.endpoint, actor)?)
            }
            ("POST", "/v1/walls/endpoints/remove") => {
                let input: WallEndpointRemoval = parse_wall_body(&request.body)?;
                wall_json(&walls.remove_endpoint(input.base_revision, &input.endpoint_id, actor)?)
            }
            ("POST", "/v1/walls/endpoints/identify") => {
                let input: WallEndpointConnect = parse_wall_body(&request.body)?;
                authorize_wall_registration(devices, &walls.endpoint_device(&input.endpoint_id)?)?;
                wall_json(&walls.identify_endpoint(&input.endpoint_id)?)
            }
            ("POST", "/v1/walls/endpoints/identify/report") => {
                let input: WallIdentificationReport = parse_wall_body(&request.body)?;
                walls.report_identification(
                    &input.endpoint_id,
                    wall_presenter_actor(actor)?,
                    &input.lease_id,
                    &input.request_id,
                    input.outcome,
                )?;
                wall_accepted()
            }
            ("PUT", "/v1/walls/layouts") => {
                let input: WallLayoutUpdate = parse_wall_body(&request.body)?;
                let snapshot = if input.activation_delay_ms == 0 {
                    walls.put_layout(input.base_revision, input.layout)?
                } else {
                    walls.put_layout_scheduled(
                        input.base_revision,
                        input.layout,
                        input.activation_delay_ms,
                    )?
                };
                wall_json(&snapshot)
            }
            ("POST", "/v1/walls/layouts/remove") => {
                let input: WallLayoutRemoval = parse_wall_body(&request.body)?;
                wall_json(&walls.remove_layout(input.base_revision, &input.wall_id)?)
            }
            ("PUT", "/v1/walls/presentation") => {
                let input: WallPresentationUpdate = parse_wall_body(&request.body)?;
                wall_json(&walls.set_presentation(
                    input.base_revision,
                    &input.wall_id,
                    input.mode,
                )?)
            }
            ("POST", "/v1/walls/connect") => {
                let input: WallEndpointConnect = parse_wall_body(&request.body)?;
                wall_json(&walls.connect(&input.endpoint_id, wall_presenter_actor(actor)?)?)
            }
            ("POST", "/v1/walls/heartbeat") => {
                let input: WallEndpointHeartbeat = parse_wall_body(&request.body)?;
                walls.heartbeat_with_scene(
                    &input.endpoint_id,
                    wall_presenter_actor(actor)?,
                    &input.lease_id,
                    input.sequence,
                    input.applied_revision,
                    input.presentation,
                    input.scene,
                )?;
                wall_accepted()
            }
            ("POST", "/v1/walls/disconnect") => {
                let input: WallEndpointDisconnect = parse_wall_body(&request.body)?;
                walls.disconnect(
                    &input.endpoint_id,
                    wall_presenter_actor(actor)?,
                    &input.lease_id,
                )?;
                wall_accepted()
            }
            _ => Err(WallStoreError::new(
                404,
                "wall_route_not_found",
                "wall route was not found",
            )),
        }
    })();
    sessions.prune_wall_controllers(walls, devices);
    Some(result.or_else(|error| {
        structured_error(
            error.status,
            json!({
                "code": error.code, "message": error.message,
            }),
        )
    }))
}

fn authorize_wall_registration(
    devices: &SharedDeviceRegistryStore,
    device_id: &str,
) -> std::result::Result<(), WallStoreError> {
    let registry = devices.lock().map_err(|_| wall_route_unavailable())?;
    let device = registry.devices.get(device_id).ok_or_else(|| {
        WallStoreError::new(
            403,
            "wall_device_unapproved",
            "endpoint requires an approved device",
        )
    })?;
    if device.approval != "approved"
        || !device.enabled
        || (!device.is_local && device.public_key.is_none())
    {
        return Err(WallStoreError::new(
            403,
            "wall_device_unapproved",
            "endpoint requires an enabled paired device",
        ));
    }
    Ok(())
}

fn wall_presenter_actor(actor: Option<&str>) -> std::result::Result<&str, WallStoreError> {
    actor.ok_or_else(|| {
        WallStoreError::new(
            401,
            "wall_device_required",
            "presentation requires a paired device session",
        )
    })
}

fn parse_wall_body<T: serde::de::DeserializeOwned>(
    body: &str,
) -> std::result::Result<T, WallStoreError> {
    if body.len() > 512 * 1024 {
        return Err(WallStoreError::new(
            413,
            "wall_request_too_large",
            "wall request exceeds size limit",
        ));
    }
    serde_json::from_str(body)
        .map_err(|_| WallStoreError::new(400, "wall_invalid", "invalid wall request JSON"))
}

fn wall_json(value: &impl Serialize) -> std::result::Result<(u16, String), WallStoreError> {
    serde_json::to_string(value)
        .map(|body| (200, body))
        .map_err(|_| wall_route_unavailable())
}

fn wall_accepted() -> std::result::Result<(u16, String), WallStoreError> {
    wall_json(
        &json!({"protocolVersion": loom_protocol::wall::WALL_PROTOCOL_VERSION, "accepted": true}),
    )
}

fn wall_route_unavailable() -> WallStoreError {
    WallStoreError::new(503, "wall_store_unavailable", "wall state is unavailable")
}
