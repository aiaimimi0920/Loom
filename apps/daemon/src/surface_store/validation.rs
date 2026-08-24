// Bounded persistence I/O and validation shared by recovered and runtime Surface state.
fn invalid_persisted_store(message: impl Into<String>) -> SurfaceStoreError {
    SurfaceStoreError::Io(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        format!("invalid persisted Surface store: {}", message.into()),
    ))
}

fn read_surface_store_bytes(path: &Path) -> std::io::Result<Vec<u8>> {
    read_surface_store_bytes_with_limit(path, MAX_SURFACE_STORE_BYTES)
}

fn read_surface_store_bytes_with_limit(path: &Path, max_bytes: usize) -> std::io::Result<Vec<u8>> {
    let file = fs::File::open(path)?;
    let read_limit = max_bytes.saturating_add(1);
    let capacity = file
        .metadata()
        .ok()
        .map(|metadata| metadata.len().min(read_limit as u64) as usize)
        .unwrap_or_default();
    let mut bytes = Vec::with_capacity(capacity);
    file.take(read_limit as u64).read_to_end(&mut bytes)?;
    Ok(bytes)
}

/// Rejects excessive structural nesting before `serde_json` allocates a value tree.
///
/// The value-depth validator treats an empty container as depth zero. Allowing one additional
/// container here preserves that public rule while still bounding parser recursion; the exact
/// value-depth check runs after parsing. JSON syntax remains the parser's responsibility.
fn json_nesting_is_within_parse_limit(bytes: &[u8], max_depth: usize) -> bool {
    let max_containers = max_depth.saturating_add(1);
    let mut containers = 0usize;
    let mut in_string = false;
    let mut escaped = false;

    for byte in bytes {
        if in_string {
            if escaped {
                escaped = false;
            } else if *byte == b'\\' {
                escaped = true;
            } else if *byte == b'"' {
                in_string = false;
            }
            continue;
        }

        match *byte {
            b'"' => in_string = true,
            b'{' | b'[' => {
                containers = containers.saturating_add(1);
                if containers > max_containers {
                    return false;
                }
            }
            b'}' | b']' => containers = containers.saturating_sub(1),
            _ => {}
        }
    }
    true
}

struct BoundedStoreWriter {
    bytes: Vec<u8>,
    max_bytes: usize,
}

impl BoundedStoreWriter {
    fn new(max_bytes: usize) -> Self {
        Self {
            bytes: Vec::new(),
            max_bytes,
        }
    }

    fn into_inner(self) -> Vec<u8> {
        self.bytes
    }
}

impl Write for BoundedStoreWriter {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        if self.bytes.len().saturating_add(buffer.len()) > self.max_bytes {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!(
                    "persisted Surface store exceeds the {} byte limit",
                    self.max_bytes
                ),
            ));
        }
        self.bytes.extend_from_slice(buffer);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SurfaceStoreDocumentRef<'a> {
    schema_version: u32,
    instances: BTreeMap<&'a str, &'a SurfaceInstanceRecord>,
}

/// Serializes the persistent projection of `instances` exactly as `persist` writes it.
///
/// Temporary instances are dropped here rather than at the call sites, so a mutation that only
/// touches one of them produces the same bytes as before it ran and `persist` can skip its write.
fn document_bytes(
    instances: &BTreeMap<String, SurfaceInstanceRecord>,
) -> Result<Vec<u8>, SurfaceStoreError> {
    document_bytes_with_limit(instances, MAX_SURFACE_STORE_BYTES)
}

fn document_bytes_with_limit(
    instances: &BTreeMap<String, SurfaceInstanceRecord>,
    max_bytes: usize,
) -> Result<Vec<u8>, SurfaceStoreError> {
    let document = SurfaceStoreDocumentRef {
        schema_version: SURFACE_STORE_SCHEMA_VERSION,
        instances: instances
            .iter()
            .filter(|(_, record)| {
                record.descriptor.persistence == SurfaceInstancePersistence::Persistent
            })
            .map(|(id, record)| (id.as_str(), record))
            .collect(),
    };
    let mut writer = BoundedStoreWriter::new(max_bytes);
    serde_json::to_writer_pretty(&mut writer, &document)?;
    writer.write_all(b"\n")?;
    Ok(writer.into_inner())
}

fn validate_loaded_instances(
    instances: &BTreeMap<String, SurfaceInstanceRecord>,
) -> Result<(), SurfaceStoreError> {
    for (instance_id, instance) in instances {
        validate_identity(instance_id, "persisted instance id")?;
        if instance.descriptor.instance_id != *instance_id {
            return Err(invalid_persisted_store(format!(
                "instance map key `{instance_id}` does not match its descriptor"
            )));
        }
        if instance.descriptor.persistence != SurfaceInstancePersistence::Persistent {
            return Err(invalid_persisted_store(format!(
                "instance `{instance_id}` is not persistent"
            )));
        }
        validate_identity(&instance.descriptor.art_id, "persisted Art id")?;
        Version::parse(&instance.descriptor.art_version).map_err(|error| {
            invalid_persisted_store(format!(
                "instance `{instance_id}` has an invalid Art version: {error}"
            ))
        })?;
        if normalize_package_digest(&instance.descriptor.package_digest)?
            != instance.descriptor.package_digest
        {
            return Err(invalid_persisted_store(format!(
                "instance `{instance_id}` has a non-canonical package digest"
            )));
        }
        if instance.pending_events.len() > MAX_PENDING_SURFACE_EVENTS {
            return Err(invalid_persisted_store(format!(
                "instance `{instance_id}` exceeds the pending event limit"
            )));
        }
        if instance.pending_confirmations.len() > MAX_PENDING_SURFACE_CONFIRMATIONS {
            return Err(invalid_persisted_store(format!(
                "instance `{instance_id}` exceeds the pending confirmation limit"
            )));
        }
        if instance.migration_history.len() > 8 {
            return Err(invalid_persisted_store(format!(
                "instance `{instance_id}` exceeds the migration history limit"
            )));
        }

        for (attachment_id, attachment) in &instance.attachments {
            validate_identity(attachment_id, "persisted attachment id")?;
            if attachment.descriptor.attachment_id != *attachment_id
                || attachment.descriptor.instance_id != *instance_id
            {
                return Err(invalid_persisted_store(format!(
                    "instance `{instance_id}` has mismatched attachment identity"
                )));
            }
            validate_identity(
                &attachment.descriptor.hook_node_id,
                "persisted Hook node id",
            )?;
            validate_identity(&attachment.descriptor.device_id, "persisted device id")?;
            if let Some(snapshot) = attachment.snapshot.as_ref() {
                validate_surface_snapshot(snapshot)
                    .map_err(|error| invalid_persisted_store(error.to_string()))?;
                validate_snapshot_identity(instance, instance_id, snapshot)?;
                if snapshot.attachment_id != *attachment_id {
                    return Err(invalid_persisted_store(format!(
                        "instance `{instance_id}` has a snapshot for another attachment"
                    )));
                }
            }
        }

        for event in &instance.pending_events {
            validate_surface_event(event)
                .map_err(|error| invalid_persisted_store(error.to_string()))?;
            if event.instance_id != *instance_id {
                return Err(invalid_persisted_store(format!(
                    "instance `{instance_id}` has an event for another instance"
                )));
            }
        }
        for (event_id, ack) in &instance.event_acks {
            validate_surface_protocol(&ack.protocol_version)
                .map_err(|error| invalid_persisted_store(error.to_string()))?;
            if ack.event_id != *event_id || ack.instance_id != *instance_id {
                return Err(invalid_persisted_store(format!(
                    "instance `{instance_id}` has a mismatched action acknowledgement"
                )));
            }
        }
        for (confirmation_id, pending) in &instance.pending_confirmations {
            validate_surface_confirmation_request(&pending.request)
                .map_err(|error| invalid_persisted_store(error.to_string()))?;
            validate_surface_event(&pending.event)
                .map_err(|error| invalid_persisted_store(error.to_string()))?;
            if pending.request.confirmation_id != *confirmation_id
                || pending.request.instance_id != *instance_id
                || pending.event.instance_id != *instance_id
                || pending.request.event_id != pending.event.event_id
            {
                return Err(invalid_persisted_store(format!(
                    "instance `{instance_id}` has a mismatched pending confirmation"
                )));
            }
        }

        if let Some(preview) = instance.latest_preview.as_ref() {
            validate_surface_protocol(&preview.protocol_version)
                .map_err(|error| invalid_persisted_store(error.to_string()))?;
            if preview.instance_id != *instance_id {
                return Err(invalid_persisted_store(format!(
                    "instance `{instance_id}` has a preview for another instance"
                )));
            }
            validate_port_value(&preview.value)?;
        }
        if let Some(result) = instance.latest_result.as_ref() {
            validate_surface_protocol(&result.protocol_version)
                .map_err(|error| invalid_persisted_store(error.to_string()))?;
            if result.instance_id != *instance_id {
                return Err(invalid_persisted_store(format!(
                    "instance `{instance_id}` has a result for another instance"
                )));
            }
            for value in result.outputs.values() {
                validate_port_value(value)?;
            }
        }
        if let Some(failure) = instance.last_failure.as_ref() {
            validate_surface_protocol(&failure.protocol_version)
                .map_err(|error| invalid_persisted_store(error.to_string()))?;
            if failure.instance_id != *instance_id {
                return Err(invalid_persisted_store(format!(
                    "instance `{instance_id}` has a failure for another instance"
                )));
            }
        }
    }
    Ok(())
}

fn instance_mut<'a>(
    instances: &'a mut BTreeMap<String, SurfaceInstanceRecord>,
    instance_id: &str,
) -> Result<&'a mut SurfaceInstanceRecord, SurfaceStoreError> {
    instances
        .get_mut(instance_id)
        .ok_or_else(|| SurfaceStoreError::NotFound(instance_id.to_owned()))
}

fn attachment_mut<'a>(
    instance: &'a mut SurfaceInstanceRecord,
    attachment_id: &str,
) -> Result<&'a mut SurfaceAttachmentRecord, SurfaceStoreError> {
    instance
        .attachments
        .get_mut(attachment_id)
        .ok_or_else(|| SurfaceStoreError::NotFound(attachment_id.to_owned()))
}

fn validate_identity(value: &str, label: &str) -> Result<(), SurfaceStoreError> {
    if is_safe_surface_identifier(value) {
        Ok(())
    } else {
        Err(SurfaceStoreError::Invalid(format!(
            "{label} is not a safe Surface identifier"
        )))
    }
}

fn normalize_package_digest(value: &str) -> Result<String, SurfaceStoreError> {
    let digest = value.trim().strip_prefix("sha256:").unwrap_or(value.trim());
    if digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(SurfaceStoreError::Invalid(
            "package digest must be a SHA-256 hex value".to_owned(),
        ));
    }
    Ok(digest.to_ascii_lowercase())
}

fn validate_snapshot_identity(
    instance: &SurfaceInstanceRecord,
    route_instance_id: &str,
    snapshot: &SurfaceSnapshot,
) -> Result<(), SurfaceStoreError> {
    if snapshot.instance_id != route_instance_id {
        return Err(SurfaceStoreError::Invalid(
            "snapshot instance id does not match route".to_owned(),
        ));
    }
    if snapshot.art_id != instance.descriptor.art_id
        || snapshot.art_version != instance.descriptor.art_version
    {
        return Err(SurfaceStoreError::Conflict(
            "snapshot Art identity does not match locked instance package".to_owned(),
        ));
    }
    Ok(())
}

fn validate_commit_identity(
    instance: &SurfaceInstanceRecord,
    route_instance_id: &str,
    commit_instance_id: &str,
    generation: u64,
) -> Result<(), SurfaceStoreError> {
    if commit_instance_id != route_instance_id {
        return Err(SurfaceStoreError::Invalid(
            "commit instance id does not match route".to_owned(),
        ));
    }
    if generation != instance.descriptor.generation {
        return Err(SurfaceStoreError::Conflict(format!(
            "commit generation {generation} is stale; current generation is {}",
            instance.descriptor.generation
        )));
    }
    Ok(())
}

fn validate_port_value(value: &SurfacePortValue) -> Result<(), SurfaceStoreError> {
    match value {
        SurfacePortValue::Value { .. } => Ok(()),
        SurfacePortValue::Resource { resource } => validate_surface_resource(resource)
            .map_err(|error| SurfaceStoreError::Invalid(error.to_string())),
        SurfacePortValue::Stream { stream } => {
            validate_identity(&stream.stream_id, "stream id")?;
            if stream.item_type.trim().is_empty() {
                return Err(SurfaceStoreError::Invalid(
                    "stream item type cannot be empty".to_owned(),
                ));
            }
            Ok(())
        }
    }
}

fn validate_surface_event_context(
    instance: &SurfaceInstanceRecord,
    route_instance_id: &str,
    event: &SurfaceEvent,
    action: &str,
) -> Result<(), SurfaceStoreError> {
    if event.instance_id != route_instance_id {
        return Err(SurfaceStoreError::Invalid(
            "Surface event instance id does not match route".to_owned(),
        ));
    }
    if event.generation != instance.descriptor.generation {
        return Err(SurfaceStoreError::Conflict(format!(
            "Surface event generation {} is stale; current generation is {}",
            event.generation, instance.descriptor.generation
        )));
    }
    let attachment = instance
        .attachments
        .get(&event.attachment_id)
        .ok_or_else(|| SurfaceStoreError::NotFound(event.attachment_id.clone()))?;
    if !matches!(
        attachment.lifecycle,
        SurfaceLifecycleState::Mounted | SurfaceLifecycleState::Active
    ) {
        return Err(SurfaceStoreError::Conflict(format!(
            "Surface attachment is not interactive while {:?}",
            attachment.lifecycle
        )));
    }
    let snapshot = attachment.snapshot.as_ref().ok_or_else(|| {
        SurfaceStoreError::Conflict("Surface event attachment has no mounted snapshot".to_owned())
    })?;
    if event.base_revision != snapshot.revision {
        return Err(SurfaceStoreError::Conflict(format!(
            "Surface event base revision {} does not match current revision {}",
            event.base_revision, snapshot.revision
        )));
    }
    let node = find_node(&snapshot.scene, &event.node_id)
        .ok_or_else(|| SurfaceStoreError::NotFound(event.node_id.clone()))?;
    if node.events.get(&event.event).map(String::as_str) != Some(action) {
        return Err(SurfaceStoreError::Invalid(format!(
            "Surface node {} does not declare action {action} for event {}",
            event.node_id, event.event
        )));
    }
    Ok(())
}

fn lifecycle_transition_allowed(
    current: &SurfaceLifecycleState,
    next: &SurfaceLifecycleState,
) -> bool {
    use SurfaceLifecycleState::{Active, Created, Disposed, Inactive, Mounted, Suspended};
    matches!(
        (current, next),
        (Created, Mounted | Disposed)
            | (Mounted, Active | Inactive | Suspended | Disposed)
            | (Active, Inactive | Suspended | Disposed)
            | (Inactive, Active | Suspended | Disposed)
            | (Suspended, Active | Inactive | Disposed)
    )
}
