use super::*;
impl OfflinePeers {
    pub(super) fn forward_receiver(
        &self,
        actor: &str,
        record: Record,
        operation: &str,
        body: Value,
        registry: &SharedDeviceRegistryStore,
    ) -> PeerResult<Value> {
        let result = self.exchange(
            &record.peer_id,
            record.trust_revision,
            local::remote_call(&record, "receiver", operation, body.clone()),
        );
        if let Err(error) = &result {
            if matches!(error.status, 403 | 404 | 410) {
                let mut store = self
                    .transfers
                    .lock()
                    .map_err(|_| failure(503, "projection_busy"))?;
                let mut current = store.get(&record.envelope.projection_id)?;
                current.unlinked = true;
                store.commit(current)?;
            }
        }
        let response = result?;
        let mut next = record.clone();
        match operation {
            "unlink" => {
                if response["unlinked"] != true {
                    return Err(failure(502, "peer_invalid_response"));
                }
                next.unlinked = true;
            }
            "receipt" => {
                if response["recorded"] != true {
                    return Err(failure(502, "peer_invalid_response"));
                }
                let receipt: ProjectionReceipt = actions::from(body)?;
                next.status = receipt.status;
                next.unlinked = receipt.status == DeliveryStatus::Rejected;
            }
            _ => {
                if response["envelope"] != json!(record.envelope) || response["linked"] != true {
                    return Err(failure(502, "peer_invalid_response"));
                }
                next.revision = response["revision"]
                    .as_u64()
                    .filter(|rev| *rev >= record.revision)
                    .ok_or_else(|| failure(502, "peer_invalid_response"))?;
                next.digest = response["digest"]
                    .as_str()
                    .ok_or_else(|| failure(502, "peer_invalid_response"))?
                    .to_owned();
                if !response["snapshot"].is_null() {
                    next.snapshot = actions::from(response["snapshot"].clone())?;
                }
                next.status = actions::from(response["delivery"]["status"].clone())?;
                next.receiver_unit = actions::from(response["receiverUnitId"].clone())?;
                next.validate()?;
            }
        }
        let state = self
            .state
            .lock()
            .map_err(|_| failure(503, "peer_unavailable"))?;
        let registry = registry
            .try_lock()
            .map_err(|_| failure(503, "projection_busy"))?;
        authorized(&state, &registry, &next)?;
        if next.target_id != actor {
            return Err(failure(403, "projection_access_denied"));
        }
        let mut store = self
            .transfers
            .lock()
            .map_err(|_| failure(503, "projection_busy"))?;
        let current = store.get(&record.envelope.projection_id)?;
        authorized(&state, &registry, &current)?;
        // A source stop can arrive over the independent peer handler while this read is pending.
        if current.unlinked
            && operation != "unlink"
            && !(operation == "receipt" && next.status == current.status)
        {
            return Err(failure(410, "projection_unlinked"));
        }
        let returned = if matches!(operation, "receipt" | "unlink") {
            response
        } else {
            next.response(!response["snapshot"].is_null())
        };
        // Preserve updated image/state for restart recovery, without rewriting every poll.
        if !matches!(operation, "read" | "inspect")
            || next.revision != current.revision
            || next.digest != current.digest
            || next.status != current.status
            || next.receiver_unit != current.receiver_unit
        {
            store.commit(next)?;
        }
        Ok(returned)
    }
}
