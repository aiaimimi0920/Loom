include!("projection_delivery.rs");

const PROJECTION_ROUTES: &[&str] = &[
    "/v1/projections/targets",
    "/v1/projections/inbox",
    "/v1/projections/receipt",
    "/v1/projections/create",
    "/v1/projections/inspect",
    "/v1/projections/accept",
    "/v1/projections/update",
    "/v1/projections/read",
    "/v1/projections/unlink",
];

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProjectionCreate {
    envelope: ProjectionEnvelope,
    snapshot: ProjectionSnapshot,
    target_device_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProjectionInspect {
    envelope: ProjectionEnvelope,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProjectionAccept {
    envelope: ProjectionEnvelope,
    expected_revision: u64,
    expected_digest: String,
    receiver_unit_id: String,
    confirmed: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProjectionUpdate {
    projection_id: String,
    source_session_id: String,
    prior_revision: u64,
    revision: u64,
    digest: String,
    snapshot: ProjectionSnapshot,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProjectionRead {
    projection_id: String,
    known_revision: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProjectionUnlink {
    projection_id: String,
}

fn parse_projection_body<T: serde::de::DeserializeOwned>(
    body: &str,
) -> std::result::Result<T, ProjectionError> {
    if body.len() > loom_protocol::projection::MAX_PROJECTION_HTTP_BYTES {
        return Err(ProjectionError::new(413, "projection_request_budget"));
    }
    serde_json::from_str(body).map_err(|_| ProjectionError::new(400, "projection_invalid_request"))
}

fn projection_source_authorized(
    registry: &DeviceRegistryStore,
    record: &ProjectionRecord,
) -> std::result::Result<(), ProjectionError> {
    let source = registry
        .authorized_keyed_device(&record.envelope.source.device_id)
        .map_err(|_| ProjectionError::new(403, "projection_source_revoked"))?;
    if source.session_epoch != record.source_epoch {
        return Err(ProjectionError::new(403, "projection_source_revoked"));
    }
    Ok(())
}

fn projection_response(record: &ProjectionRecord, image: bool) -> Value {
    json!({ "envelope": record.envelope, "revision": record.revision, "digest": record.digest,
        "width": record.snapshot.width, "height": record.snapshot.height,
        "snapshot": if image { Some(&record.snapshot) } else { None },
        "linked": !record.unlinked, "receiverDeviceId": record.receiver, "receiverUnitId": record.receiver_unit_id,
        "delivery": record.delivery })
}

fn handle_projection_route(
    path: &str,
    body: &str,
    actor: Option<&str>,
    registry: &SharedDeviceRegistryStore,
) -> Result<(u16, String)> {
    let result = (|| -> std::result::Result<Value, ProjectionError> {
        let actor =
            actor.ok_or_else(|| ProjectionError::new(403, "projection_pairing_required"))?;
        // Authenticated projection work is serialized without queuing behind itself.
        static WORK: Mutex<()> = Mutex::new(());
        let _work = WORK
            .try_lock()
            .map_err(|_| ProjectionError::new(429, "projection_busy"))?;
        let mut registry = registry
            .lock()
            .map_err(|_| ProjectionError::new(503, "projection_unavailable"))?;
        let device = registry
            .authorized_keyed_device(actor)
            .map_err(|_| ProjectionError::new(403, "projection_pairing_required"))?
            .clone();
        let now = unix_time_millis();
        match path {
            "/v1/projections/targets" | "/v1/projections/inbox" | "/v1/projections/receipt" => {
                handle_projection_delivery(path, body, &device, &mut registry, now)
            }
            "/v1/projections/create" => {
                let input: ProjectionCreate = parse_projection_body(body)?;
                if input.envelope.source.device_id != actor {
                    return Err(ProjectionError::new(403, "projection_source_mismatch"));
                }
                verify_projection_signature(
                    &input.envelope,
                    device.public_key.as_deref().unwrap_or_default(),
                )?;
                validate_projection_snapshot(&input.snapshot, &input.envelope.content.digest)?;
                let id = input.envelope.projection_id.clone();
                let delivery = projection_delivery_target(
                    &registry,
                    actor,
                    input.target_device_id.as_deref(),
                    now,
                )?;
                registry.projections.create(
                    input.envelope,
                    input.snapshot,
                    device.session_epoch,
                    now,
                    delivery,
                )?;
                Ok(projection_response(registry.projections.get(&id)?, false))
            }
            "/v1/projections/inspect" => {
                let input: ProjectionInspect = parse_projection_body(body)?;
                let record = registry.projections.invitation(&input.envelope, now)?;
                projection_target_authorized(record, actor, device.session_epoch)?;
                projection_source_authorized(&registry, record)?;
                let source = registry
                    .authorized_keyed_device(&input.envelope.source.device_id)
                    .map_err(|_| ProjectionError::new(403, "projection_source_revoked"))?;
                verify_projection_signature(
                    &input.envelope,
                    source.public_key.as_deref().unwrap_or_default(),
                )?;
                let mut response = projection_response(record, true);
                response["sourceName"] = json!(source.name);
                Ok(response)
            }
            "/v1/projections/accept" => {
                let input: ProjectionAccept = parse_projection_body(body)?;
                if !input.confirmed
                    || !loom_protocol::projection::projection_identifier_valid(
                        &input.receiver_unit_id,
                    )
                {
                    return Err(ProjectionError::new(
                        400,
                        "projection_confirmation_required",
                    ));
                }
                projection_source_authorized(
                    &registry,
                    registry.projections.get(&input.envelope.projection_id)?,
                )?;
                let record = registry.projections.accept(
                    &input.envelope,
                    actor,
                    device.session_epoch,
                    &input.receiver_unit_id,
                    input.expected_revision,
                    &input.expected_digest,
                    now,
                )?;
                Ok(projection_response(&record, true))
            }
            "/v1/projections/update" => {
                let input: ProjectionUpdate = parse_projection_body(body)?;
                let record = registry.projections.get(&input.projection_id)?;
                projection_source_authorized(&registry, record)?;
                if record.envelope.source.device_id != actor {
                    return Err(ProjectionError::new(403, "projection_source_mismatch"));
                }
                validate_projection_snapshot(&input.snapshot, &input.digest)?;
                registry.projections.update(
                    &input.projection_id,
                    actor,
                    &input.source_session_id,
                    input.prior_revision,
                    input.revision,
                    input.digest,
                    input.snapshot,
                    now,
                )?;
                Ok(projection_response(
                    registry.projections.get(&input.projection_id)?,
                    false,
                ))
            }
            "/v1/projections/read" => {
                let input: ProjectionRead = parse_projection_body(body)?;
                if input.known_revision > loom_protocol::projection::MAX_PROJECTION_REVISION {
                    return Err(ProjectionError::new(400, "projection_invalid_revision"));
                }
                let record = registry.projections.get(&input.projection_id)?;
                projection_source_authorized(&registry, record)?;
                if record.envelope.source.device_id != actor
                    && (record.receiver.as_deref() != Some(actor)
                        || record.receiver_epoch != Some(device.session_epoch))
                {
                    return Err(ProjectionError::new(403, "projection_access_denied"));
                }
                if record.unlinked {
                    if record
                        .delivery
                        .as_ref()
                        .is_some_and(|delivery| delivery.status == DeliveryStatus::Rejected)
                    {
                        return Err(ProjectionError::new(410, "projection_rejected"));
                    }
                    return Err(ProjectionError::new(410, "projection_unlinked"));
                }
                if record.receiver.is_none() && record.envelope.expires_at_ms <= now {
                    return Err(ProjectionError::new(410, "projection_invitation_expired"));
                }
                Ok(projection_response(
                    record,
                    record.revision != input.known_revision,
                ))
            }
            "/v1/projections/unlink" => {
                let input: ProjectionUnlink = parse_projection_body(body)?;
                registry
                    .projections
                    .unlink(&input.projection_id, actor, device.session_epoch)?;
                Ok(json!({ "unlinked": true }))
            }
            _ => Err(ProjectionError::new(404, "projection_route_missing")),
        }
    })();
    match result {
        Ok(value) => Ok((200, serde_json::to_string(&value)?)),
        Err(error) => structured_error(
            error.status,
            json!({ "code": error.code, "message": error.code }),
        ),
    }
}
