// Surface resource leases and instance creation, reads, and deletion.
fn create_surface_resource(
    body: &str,
    surface_resources: &SharedSurfaceResourceStore,
    shared_images: &SharedImageStoreHandle,
) -> Result<(u16, String)> {
    let request = match serde_json::from_str::<CreateSurfaceResourceRequest>(body) {
        Ok(request) => request,
        Err(error) => return invalid_surface_payload(error),
    };
    let encoded = request
        .data_base64
        .split_once(',')
        .map(|(_, data)| data)
        .unwrap_or(request.data_base64.as_str());
    let bytes = match decode_surface_resource_base64(encoded, MAX_SURFACE_RESOURCE_BYTES) {
        Ok(bytes) => bytes,
        Err(message) => {
            return structured_error(
                400,
                json!({
                    "code": "invalid_surface_resource",
                    "message": message,
                }),
            )
        }
    };
    if request.preferred_transport == Some(SurfaceResourceTransportKind::SharedMemory) {
        if request.kind != SurfaceResourceKind::Image {
            return structured_error(
                400,
                json!({
                    "code": "invalid_surface_resource",
                    "message": "shared-memory Surface resources must be images",
                }),
            );
        }
        let image = match image::load_from_memory(&bytes) {
            Ok(image) => image.to_rgba8(),
            Err(error) => {
                return structured_error(
                    400,
                    json!({
                        "code": "invalid_surface_resource",
                        "message": format!("shared-memory image decode failed: {error}"),
                    }),
                )
            }
        };
        let (width, height) = image.dimensions();
        let rgba = image.into_raw();
        if rgba.len() > MAX_SURFACE_RESOURCE_BYTES {
            return structured_error(
                400,
                json!({
                    "code": "invalid_surface_resource",
                    "message": format!("decoded Surface image exceeds {MAX_SURFACE_RESOURCE_BYTES} bytes"),
                }),
            );
        }
        let shared = match shared_images
            .lock()
            .map_err(|_| anyhow::anyhow!("lock shared image store"))?
            .create_rgba8(width, height, rgba.clone())
        {
            Ok(shared) => shared,
            Err(error) => return shared_image_error_response(error),
        };
        let lease = {
            let mut store = surface_resources
                .lock()
                .map_err(|_| anyhow::anyhow!("Surface resource store is unavailable"))?;
            let lease = match store.register(
                SurfaceResourceKind::Image,
                "application/x-neuro-rgba8",
                &rgba,
                Some(width),
                Some(height),
                request.lease_millis,
            ) {
                Ok(lease) => lease,
                Err(error) => {
                    shared_images
                        .lock()
                        .map_err(|_| anyhow::anyhow!("lock shared image store"))?
                        .release(&shared.handle);
                    return surface_resource_store_error(error);
                }
            };
            match store.replace_lease_transport(
                &lease.lease_id,
                SurfaceResourceTransport {
                    kind: SurfaceResourceTransportKind::SharedMemory,
                    handle: Some(shared.handle.clone()),
                    path: None,
                    stream_id: None,
                },
            ) {
                Ok(lease) => lease,
                Err(error) => {
                    let _ = store.release(&lease.lease_id);
                    shared_images
                        .lock()
                        .map_err(|_| anyhow::anyhow!("lock shared image store"))?
                        .release(&shared.handle);
                    return surface_resource_store_error(error);
                }
            }
        };
        return Ok((201, serde_json::to_string(&lease)?));
    }

    let mut store = surface_resources
        .lock()
        .map_err(|_| anyhow::anyhow!("Surface resource store is unavailable"))?;
    match store.register(
        request.kind,
        &request.mime,
        &bytes,
        request.width,
        request.height,
        request.lease_millis,
    ) {
        Ok(lease) => Ok((201, serde_json::to_string(&lease)?)),
        Err(error) => surface_resource_store_error(error),
    }
}

/// Rejects oversized encoded input before `base64` can allocate its decoded output buffer.
fn decode_surface_resource_base64(
    encoded: &str,
    max_decoded_bytes: usize,
) -> Result<Vec<u8>, String> {
    let max_encoded_bytes = max_decoded_bytes
        .checked_add(2)
        .and_then(|value| value.checked_div(3))
        .and_then(|value| value.checked_mul(4))
        .unwrap_or(usize::MAX);
    if encoded.len() > max_encoded_bytes {
        return Err(format!(
            "encoded Surface resource exceeds the {max_decoded_bytes}-byte decoded limit"
        ));
    }
    let bytes = BASE64
        .decode(encoded)
        .map_err(|error| format!("resource data is not valid base64: {error}"))?;
    if bytes.len() > max_decoded_bytes {
        return Err(format!(
            "decoded Surface resource exceeds {max_decoded_bytes} bytes"
        ));
    }
    Ok(bytes)
}

fn release_surface_resource_lease(
    lease_id: &str,
    surface_resources: &SharedSurfaceResourceStore,
    shared_images: &SharedImageStoreHandle,
) -> Result<(u16, String)> {
    let mut store = surface_resources
        .lock()
        .map_err(|_| anyhow::anyhow!("Surface resource store is unavailable"))?;
    if let Some(lease) = store.release(lease_id)? {
        drop(store);
        release_surface_shared_memory(&lease, shared_images)?;
        Ok((204, String::new()))
    } else {
        surface_resource_store_error(SurfaceResourceStoreError::NotFound(lease_id.to_owned()))
    }
}

/// Every content address a Surface instance record mentions, wherever it mentions it.
///
/// The scan runs over the record's serialized JSON instead of over named fields on purpose. A
/// resource id can sit in an attachment snapshot, in a lease, in authoritative state a framework
/// wrote for itself, in a queued event, or in a migration checkpoint, and a field added later can
/// start carrying one without this function being updated. Under-reporting is the one direction the
/// collector cannot survive — the garbage collector would delete an object a live Surface is still
/// painting with — so the cost of a serialization per instance buys the safe kind of wrong answer.
fn collect_surface_resource_ids(records: &[SurfaceInstanceRecord]) -> BTreeSet<String> {
    const PREFIX: &str = "sha256:";
    const DIGEST_CHARS: usize = 64;
    let mut ids = BTreeSet::new();
    for record in records {
        let text = match serde_json::to_string(record) {
            Ok(text) => text,
            Err(_) => continue,
        };
        for (offset, _) in text.match_indices(PREFIX) {
            let start = offset.saturating_add(PREFIX.len());
            let digest = match text.get(start..start.saturating_add(DIGEST_CHARS)) {
                Some(digest) => digest,
                None => continue,
            };
            if digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                ids.insert(format!("{PREFIX}{}", digest.to_ascii_lowercase()));
            }
        }
    }
    ids
}

/// Runs one Surface resource collection pass.
///
/// The instance store is read and its lock released *before* the resource store's lock is taken,
/// which is the order `delete_surface_instance` already established. That is also why the resource
/// store cannot look up its own references: it would have to take the two locks the other way
/// round.
///
/// `SurfaceInstanceStore::list` returns temporary instances alongside persisted ones, so the
/// reference set is a superset of what survives a restart. That is deliberate — a resource held
/// only by a temporary instance is still in use right now.
fn collect_surface_resource_garbage(
    surface_instances: &SharedSurfaceInstanceStore,
    surface_resources: &SharedSurfaceResourceStore,
) -> Result<SurfaceResourceGcOutcome> {
    let records = {
        let store = surface_instances
            .lock()
            .map_err(|_| anyhow::anyhow!("Surface instance store is unavailable"))?;
        store.list()
    };
    let referenced = collect_surface_resource_ids(&records);
    let mut store = surface_resources
        .lock()
        .map_err(|_| anyhow::anyhow!("Surface resource store is unavailable"))?;
    Ok(store.collect_garbage(&referenced))
}

/// Collects Surface resource garbage and reports the result to the runtime log. A failed pass is
/// never fatal: the store keeps working, the objects stay on disk, and the next pass tries again.
fn collect_surface_resource_garbage_logged(
    surface_instances: &SharedSurfaceInstanceStore,
    surface_resources: &SharedSurfaceResourceStore,
    reason: &str,
) {
    match collect_surface_resource_garbage(surface_instances, surface_resources) {
        Ok(outcome) => {
            let SurfaceResourceGcOutcome {
                removed_objects,
                removed_bytes,
                removed_orphan_files,
                retained_objects,
                failures,
            } = outcome;
            if removed_objects > 0 || removed_orphan_files > 0 || failures > 0 {
                runtime_log_info(format!(
                    "loom Surface resource GC ({reason}) removed {removed_objects} objects, \
                     {removed_bytes} bytes and {removed_orphan_files} orphan files; retained \
                     {retained_objects} objects with {failures} failures"
                ));
            } else {
                runtime_log_debug(format!(
                    "loom Surface resource GC ({reason}) retained {retained_objects} objects"
                ));
            }
        }
        Err(error) => runtime_log_warn(format!(
            "loom Surface resource GC ({reason}) could not run: {error}"
        )),
    }
}

fn release_surface_resource_leases(
    surface_resources: &SharedSurfaceResourceStore,
    lease_ids: &[String],
    shared_images: &SharedImageStoreHandle,
) -> Result<()> {
    if lease_ids.is_empty() {
        return Ok(());
    }
    let mut store = surface_resources
        .lock()
        .map_err(|_| anyhow::anyhow!("Surface resource store is unavailable"))?;
    let mut released = Vec::new();
    for lease_id in lease_ids {
        if let Some(lease) = store.release(lease_id)? {
            released.push(lease);
        }
    }
    drop(store);
    for lease in &released {
        release_surface_shared_memory(lease, shared_images)?;
    }
    Ok(())
}
