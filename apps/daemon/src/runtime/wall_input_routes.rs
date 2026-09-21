// Fixed paired-device endpoints; wall placement grants no generic Surface/viewer privileges.
fn route_wall_input(
    request: &ParsedHttpRequest,
    path: &str,
    walls: &SharedWallStore,
    sessions: &SharedLiveSessionStore,
    devices: &SharedDeviceRegistryStore,
    actor: Option<&str>,
) -> std::result::Result<(u16, String), WallStoreError> {
    let actor = wall_presenter_actor(actor)?;
    if request.body.len() > 4096 {
        return Err(WallStoreError::new(
            413,
            "wall_input_too_large",
            "wall input body exceeds 4096 bytes",
        ));
    }
    sessions.prune_wall_controllers(walls, devices);
    if path == "/v1/walls/control/acquire" {
        let input: WallControlAcquire = parse_wall_body(&request.body)?;
        let token_hash = sha256_bytes(
            request
                .authorization_credential("Device")
                .unwrap_or_default()
                .as_bytes(),
        );
        return walls.with_input_target(
            actor,
            &input.binding,
            None,
            Some(input.pixel),
            TileInputCapability::Pointer,
            |target| sessions.acquire_wall_controller(actor, &token_hash, &input, target),
        );
    }
    if path == "/v1/walls/input" {
        let input: WallInputRequest = parse_wall_body(&request.body)?;
        let result = (|| {
            let owner = sessions.wall_controller(actor, &input.control)?;
            if !owner.device_valid(devices) {
                return Err(wall_control_invalid());
            }
            walls.with_input_target(
                actor,
                &input.control.binding,
                Some(&owner.placement_id),
                input.event.pixel(),
                input.event.capability(),
                |target| sessions.forward_wall_input(actor, &input, target),
            )
        })();
        // A rejected edge is never retried. Revoke the exact owner so no up edge is needed to recover.
        if result.is_err() {
            sessions.release_wall_controller(actor, &input.control);
        }
        return result;
    }
    let reference: WallControlReference = parse_wall_body(&request.body)?;
    if path == "/v1/walls/control/release" {
        sessions.release_wall_controller(actor, &reference);
        return wall_accepted();
    }
    let result = (|| {
        let owner = sessions.wall_controller(actor, &reference)?;
        if !owner.device_valid(devices) {
            return Err(wall_control_invalid());
        }
        walls.with_input_target(
            actor,
            &reference.binding,
            Some(&owner.placement_id),
            None,
            TileInputCapability::Pointer,
            |target| sessions.renew_wall_controller(actor, &reference, target),
        )
    })();
    if result.is_err() {
        sessions.release_wall_controller(actor, &reference);
    }
    result
}
