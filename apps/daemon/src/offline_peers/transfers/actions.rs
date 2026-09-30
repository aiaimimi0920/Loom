use super::*;
pub(super) fn from<T: serde::de::DeserializeOwned>(value: Value) -> PeerResult<T> {
    serde_json::from_value(value).map_err(|_| failure(400, "projection_invalid_request"))
}
pub(super) fn receiver_action(
    record: &mut Record,
    operation: &str,
    body: Value,
) -> PeerResult<Value> {
    if operation == "unlink" {
        record.unlinked = true;
        return Ok(json!({"unlinked": true}));
    }
    if operation == "receipt" {
        let receipt: ProjectionReceipt = from(body.clone())?;
        if matches!(
            receipt.status,
            DeliveryStatus::Displayed | DeliveryStatus::Rejected
        ) && receipt.status == record.status
        {
            return Ok(json!({"recorded": true}));
        }
    }
    record.active()?;
    match operation {
        "inspect" => {
            let input: ProjectionInspect = from(body)?;
            if input.envelope != record.envelope {
                return Err(failure(403, "projection_invitation_mismatch"));
            }
            Ok(record.response(true))
        }
        "read" => {
            let input: ProjectionRead = from(body)?;
            if input.known_revision > record.revision {
                return Err(failure(409, "projection_revision_conflict"));
            }
            Ok(record.response(input.known_revision < record.revision))
        }
        "accept" => {
            let input: ProjectionAccept = from(body)?;
            if input.envelope != record.envelope {
                return Err(failure(403, "projection_invitation_mismatch"));
            }
            if !input.confirmed
                || !loom_protocol::projection::projection_identifier_valid(&input.receiver_unit_id)
            {
                return Err(failure(400, "projection_confirmation_required"));
            }
            if let Some(unit) = &record.receiver_unit {
                if unit != &input.receiver_unit_id {
                    return Err(failure(409, "projection_invitation_consumed"));
                }
            } else {
                if record.revision != input.expected_revision
                    || record.digest != input.expected_digest
                {
                    return Err(failure(409, "projection_content_changed"));
                }
                record.receiver_unit = Some(input.receiver_unit_id);
                record.status = DeliveryStatus::Accepted;
            }
            Ok(record.response(true))
        }
        "receipt" => {
            let input: ProjectionReceipt = from(body)?;
            if input.status == record.status {
                return Ok(json!({"recorded": true}));
            }
            match input.status {
                DeliveryStatus::Displayed if record.status == DeliveryStatus::Accepted => {
                    record.status = DeliveryStatus::Displayed
                }
                DeliveryStatus::Rejected
                    if record.status == DeliveryStatus::AwaitingConfirmation =>
                {
                    record.status = DeliveryStatus::Rejected;
                    record.unlinked = true;
                }
                _ => return Err(failure(409, "projection_invalid_receipt")),
            }
            Ok(json!({"recorded": true}))
        }
        _ => Err(failure(404, "projection_route_missing")),
    }
}

pub(super) fn update(record: &mut Record, input: ProjectionUpdate) -> PeerResult<()> {
    record.active()?;
    if input.source_session_id != record.envelope.source.session_id {
        return Err(failure(403, "projection_source_mismatch"));
    }
    if input.revision == input.prior_revision.saturating_add(1)
        && record.revision == input.revision
        && record.digest == input.digest
    {
        return Ok(());
    }
    if input.prior_revision != record.revision
        || input.revision != input.prior_revision.saturating_add(1)
        || input.revision > loom_protocol::projection::MAX_PROJECTION_REVISION
    {
        return Err(failure(409, "projection_revision_conflict"));
    }
    let now = unix_time_millis();
    if now.saturating_sub(record.updated_ms) < 500 {
        return Err(failure(429, "projection_update_rate"));
    }
    record.revision = input.revision;
    record.digest = input.digest;
    record.snapshot = input.snapshot;
    record.updated_ms = now;
    Ok(())
}
