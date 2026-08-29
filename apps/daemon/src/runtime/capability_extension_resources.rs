// Invocation-scoped upload conversion for authenticated Hook extension commands.
const MAX_EXTENSION_RESOURCE_UPLOADS: usize = 4;
const EXTENSION_IMAGE_READ_PERMISSION: &str = "hook.unit.image.read";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ExtensionResourceUploadError {
    Invalid,
    PermissionDenied,
    Busy,
    Store,
}

struct ExtensionResourceUploadLease {
    surface_resources: SharedSurfaceResourceStore,
    lease_ids: Vec<String>,
}

impl ExtensionResourceUploadLease {
    fn new(surface_resources: &SharedSurfaceResourceStore) -> Self {
        Self {
            surface_resources: Arc::clone(surface_resources),
            lease_ids: Vec::new(),
        }
    }
}

impl Drop for ExtensionResourceUploadLease {
    fn drop(&mut self) {
        let Ok(mut store) = self.surface_resources.lock() else {
            runtime_log_warn("could not release extension upload leases: store is busy");
            return;
        };
        for lease_id in &self.lease_ids {
            if let Err(error) = store.release(lease_id) {
                runtime_log_warn(format!(
                    "could not release extension upload lease {lease_id}: {error}"
                ));
            }
        }
    }
}

fn stage_extension_resource_uploads(
    uploads: Vec<ExtensionResourceUpload>,
    invocation: &mut ExtensionInvocation,
    snapshot: &ContributionSnapshot,
    surface_resources: &SharedSurfaceResourceStore,
) -> Result<ExtensionResourceUploadLease, ExtensionResourceUploadError> {
    let mut lease = ExtensionResourceUploadLease::new(surface_resources);
    if uploads.is_empty() {
        return Ok(lease);
    }
    if uploads.len() > MAX_EXTENSION_RESOURCE_UPLOADS
        || invocation.resource_refs.len().saturating_add(uploads.len()) > 128
    {
        return Err(ExtensionResourceUploadError::Invalid);
    }
    if !command_allows_image_upload(snapshot, invocation) {
        return Err(ExtensionResourceUploadError::PermissionDenied);
    }

    let mut total_bytes = 0usize;
    let mut uploaded_refs = Vec::with_capacity(uploads.len());
    for upload in uploads {
        let mime = validate_image_upload(&upload)?;
        let encoded = upload_base64_payload(&upload, mime)?;
        let remaining = MAX_SURFACE_RESOURCE_BYTES.saturating_sub(total_bytes);
        let bytes = decode_surface_resource_base64(encoded, remaining)
            .map_err(|_| ExtensionResourceUploadError::Invalid)?;
        total_bytes = total_bytes
            .checked_add(bytes.len())
            .ok_or(ExtensionResourceUploadError::Invalid)?;
        let resource_lease = surface_resources
            .lock()
            .map_err(|_| ExtensionResourceUploadError::Busy)?
            .register(
                SurfaceResourceKind::Image,
                mime,
                &bytes,
                None,
                None,
                Some(60_000),
            )
            .map_err(map_extension_upload_store_error)?;
        let digest = resource_lease
            .resource
            .resource_id
            .strip_prefix("sha256:")
            .ok_or(ExtensionResourceUploadError::Store)?
            .to_owned();
        uploaded_refs.push(ExtensionResourceRef {
            resource_id: resource_lease.resource.resource_id,
            kind: ExtensionResourceKind::SharedImage,
            digest,
            byte_length: resource_lease.resource.size,
            lease_id: resource_lease.lease_id.clone(),
        });
        lease.lease_ids.push(resource_lease.lease_id);
    }
    invocation.resource_refs.extend(uploaded_refs);
    Ok(lease)
}

fn command_allows_image_upload(
    snapshot: &ContributionSnapshot,
    invocation: &ExtensionInvocation,
) -> bool {
    snapshot.contributions.commands.iter().any(|command| {
        command.id == invocation.command_id
            && command.plugin_id == invocation.plugin_id
            && command
                .payload
                .get("permissions")
                .and_then(Value::as_array)
                .is_some_and(|permissions| {
                    permissions.iter().any(|permission| {
                        permission.as_str() == Some(EXTENSION_IMAGE_READ_PERMISSION)
                    })
                })
    })
}

fn validate_image_upload(
    upload: &ExtensionResourceUpload,
) -> Result<&str, ExtensionResourceUploadError> {
    let mime = upload.mime.trim();
    let supported = matches!(
        mime,
        "image/png" | "image/jpeg" | "image/webp" | "image/bmp" | "image/gif"
    );
    if upload.kind != SurfaceResourceKind::Image || !supported || upload.data_base64.is_empty() {
        return Err(ExtensionResourceUploadError::Invalid);
    }
    Ok(mime)
}

fn upload_base64_payload<'a>(
    upload: &'a ExtensionResourceUpload,
    mime: &str,
) -> Result<&'a str, ExtensionResourceUploadError> {
    if !upload.data_base64.starts_with("data:") {
        return Ok(&upload.data_base64);
    }
    let (header, encoded) = upload
        .data_base64
        .split_once(',')
        .ok_or(ExtensionResourceUploadError::Invalid)?;
    if header != format!("data:{mime};base64") || encoded.is_empty() {
        return Err(ExtensionResourceUploadError::Invalid);
    }
    Ok(encoded)
}

fn map_extension_upload_store_error(
    error: SurfaceResourceStoreError,
) -> ExtensionResourceUploadError {
    match error {
        SurfaceResourceStoreError::Invalid(_) | SurfaceResourceStoreError::Json(_) => {
            ExtensionResourceUploadError::Invalid
        }
        SurfaceResourceStoreError::LeaseRejected(_) => ExtensionResourceUploadError::Busy,
        SurfaceResourceStoreError::NotFound(_) | SurfaceResourceStoreError::Io(_) => {
            ExtensionResourceUploadError::Store
        }
    }
}
