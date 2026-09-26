// Shared-Loom delivery uses paired device identity; official identity is not consulted.
#[derive(Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum DeliveryStatus {
    AwaitingConfirmation,
    Accepted,
    Displayed,
    Rejected,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProjectionDelivery {
    target_device_id: String,
    target_epoch: u64,
    status: DeliveryStatus,
}

#[derive(Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ProjectionReceivePolicy {
    Confirm,
    Auto,
    Disabled,
}

struct ProjectionPresence {
    epoch: u64,
    seen: u64,
    policy: ProjectionReceivePolicy,
}
const PROJECTION_PRESENCE_TTL: u64 = 30_000;

fn projection_delivery_record_valid(record: &ProjectionRecord) -> bool {
    let Some(target) = &record.delivery else {
        return true;
    };
    loom_protocol::projection::projection_identifier_valid(&target.target_device_id)
        && target.target_device_id != record.envelope.source.device_id
        && match target.status {
            DeliveryStatus::AwaitingConfirmation => record.receiver.is_none(),
            DeliveryStatus::Rejected => record.receiver.is_none() && record.unlinked,
            DeliveryStatus::Accepted | DeliveryStatus::Displayed => {
                record.receiver.as_deref() == Some(target.target_device_id.as_str())
                    && record.receiver_epoch == Some(target.target_epoch)
            }
        }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProjectionInbox {
    policy: ProjectionReceivePolicy,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProjectionReceipt {
    projection_id: String,
    status: DeliveryStatus,
}

fn projection_target_authorized(
    record: &ProjectionRecord,
    actor: &str,
    epoch: u64,
) -> std::result::Result<(), ProjectionError> {
    if record
        .delivery
        .as_ref()
        .is_some_and(|target| target.target_device_id != actor || target.target_epoch != epoch)
    {
        return Err(ProjectionError::new(403, "projection_access_denied"));
    }
    Ok(())
}

fn projection_delivery_target(
    registry: &DeviceRegistryStore,
    source: &str,
    target: Option<&str>,
    now: u64,
) -> std::result::Result<Option<ProjectionDelivery>, ProjectionError> {
    let Some(target) = target else {
        return Ok(None);
    };
    if source == target {
        return Err(ProjectionError::new(409, "projection_same_device"));
    }
    let device = registry
        .authorized_keyed_device(target)
        .map_err(|_| ProjectionError::new(403, "projection_target_unavailable"))?;
    if !registry
        .projections
        .presence
        .get(target)
        .is_some_and(|presence| {
            presence.epoch == device.session_epoch
                && now.saturating_sub(presence.seen) < PROJECTION_PRESENCE_TTL
                && presence.policy != ProjectionReceivePolicy::Disabled
        })
    {
        return Err(ProjectionError::new(409, "projection_target_offline"));
    }
    Ok(Some(ProjectionDelivery {
        target_device_id: target.to_owned(),
        target_epoch: device.session_epoch,
        status: DeliveryStatus::AwaitingConfirmation,
    }))
}

fn handle_projection_delivery(
    path: &str,
    body: &str,
    device: &ManagedDevice,
    registry: &mut DeviceRegistryStore,
    now: u64,
) -> std::result::Result<Value, ProjectionError> {
    registry
        .projections
        .presence
        .retain(|_, value| now.saturating_sub(value.seen) < PROJECTION_PRESENCE_TTL);
    match path {
        "/v1/projections/targets" => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Empty {}
            let _: Empty = parse_projection_body(body)?;
            let targets: Vec<_> = registry
                .projections
                .presence
                .iter()
                .filter_map(|(id, presence)| {
                    if id == &device.id || presence.policy == ProjectionReceivePolicy::Disabled {
                        return None;
                    }
                    let target = registry.authorized_keyed_device(id).ok()?;
                    (target.session_epoch == presence.epoch).then(|| {
                        json!({ "deviceId": id,
                    "name": target.name, "policy": presence.policy, "route": "shared_loom" })
                    })
                })
                .take(MAX_PROJECTIONS)
                .collect();
            Ok(
                json!({ "targets": targets, "capabilities": { "sharedLoom": true,
                "offlinePeers": false, "officialAccount": false, "officialRelay": false } }),
            )
        }
        "/v1/projections/inbox" => {
            let input: ProjectionInbox = parse_projection_body(body)?;
            if input.policy == ProjectionReceivePolicy::Disabled {
                registry.projections.presence.remove(&device.id);
                return Ok(json!({ "invitations": [] }));
            }
            if registry.projections.presence.len() >= MAX_PROJECTIONS
                && !registry.projections.presence.contains_key(&device.id)
            {
                return Err(ProjectionError::new(429, "projection_presence_full"));
            }
            registry.projections.presence.insert(
                device.id.clone(),
                ProjectionPresence {
                    epoch: device.session_epoch,
                    seen: now,
                    policy: input.policy,
                },
            );
            let invitations: Vec<_> = registry.projections.records.values().filter(|record| {
                !record.unlinked && (record.receiver.is_some() || record.envelope.expires_at_ms > now)
                    && projection_source_authorized(registry, record).is_ok()
                    && record.delivery.as_ref().is_some_and(|target|
                        target.target_device_id == device.id && target.target_epoch == device.session_epoch
                        && matches!(target.status, DeliveryStatus::AwaitingConfirmation | DeliveryStatus::Accepted))
            }).map(|record| json!({ "envelope": record.envelope, "revision": record.revision,
                "digest": record.digest, "delivery": record.delivery,
                "sourceName": registry.devices.get(&record.envelope.source.device_id).map(|source| &source.name) }))
                .take(MAX_PROJECTIONS).collect();
            Ok(json!({ "invitations": invitations, "offlinePeers": true }))
        }
        "/v1/projections/receipt" => {
            let input: ProjectionReceipt = parse_projection_body(body)?;
            let record = registry.projections.get(&input.projection_id)?;
            projection_source_authorized(registry, record)?;
            projection_target_authorized(record, &device.id, device.session_epoch)?;
            let target = record
                .delivery
                .as_ref()
                .ok_or_else(|| ProjectionError::new(409, "projection_not_targeted"))?;
            if target.status == input.status {
                return Ok(json!({ "recorded": true }));
            }
            let permitted = match input.status {
                DeliveryStatus::Displayed => {
                    target.status == DeliveryStatus::Accepted
                        && !record.unlinked
                        && record.receiver.as_deref() == Some(device.id.as_str())
                        && record.receiver_epoch == Some(device.session_epoch)
                }
                DeliveryStatus::Rejected => {
                    target.status == DeliveryStatus::AwaitingConfirmation
                        && record.envelope.expires_at_ms > now
                        && !record.unlinked
                }
                _ => false,
            };
            if !permitted {
                return Err(ProjectionError::new(409, "projection_invalid_receipt"));
            }
            let mut record = record.clone();
            record.delivery.as_mut().unwrap().status = input.status;
            if input.status == DeliveryStatus::Rejected {
                record.unlinked = true;
            }
            registry.projections.commit(record)?;
            Ok(json!({ "recorded": true }))
        }
        _ => Err(ProjectionError::new(404, "projection_route_missing")),
    }
}
