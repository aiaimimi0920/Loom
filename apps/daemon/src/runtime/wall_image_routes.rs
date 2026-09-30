// Wall membership is an image grant; it does not mint a transferable Surface lease.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WallImageRead {
    endpoint_id: String,
    lease_id: String,
    revision: u64,
    resource_id: String,
}

fn read_wall_image(
    body: &str,
    walls: &SharedWallStore,
    resources: &SharedSurfaceResourceStore,
    actor: Option<&str>,
) -> std::result::Result<(u16, String), WallStoreError> {
    let input: WallImageRead = parse_wall_body(body)?;
    walls.authorize_image(
        &input.endpoint_id,
        wall_presenter_actor(actor)?,
        &input.lease_id,
        input.revision,
        &input.resource_id,
    )?;
    wall_image_payload(resources, &input.resource_id)
}

fn wall_image_payload(
    resources: &SharedSurfaceResourceStore,
    resource_id: &str,
) -> std::result::Result<(u16, String), WallStoreError> {
    let mut store = resources.lock().map_err(|_| wall_route_unavailable())?;
    let digest = resource_id.strip_prefix("sha256:").ok_or_else(|| {
        WallStoreError::new(400, "wall_invalid", "image id must be content addressed")
    })?;
    let payload = store.get(digest).map_err(|error| {
        WallStoreError::new(
            error.status_code(),
            "wall_image_unavailable",
            "wall image resource is unavailable",
        )
    })?;
    if payload.descriptor.kind != SurfaceResourceKind::Image
        || !matches!(
            payload.descriptor.mime.as_str(),
            "image/png"
                | "image/jpeg"
                | "image/webp"
                | "image/bmp"
                | "image/gif"
                | "application/x-neuro-rgba8"
        )
    {
        return Err(WallStoreError::new(
            415,
            "wall_image_format_unsupported",
            "wall source is not a supported raster image",
        ));
    }
    wall_json(
        &json!({ "protocolVersion": loom_protocol::wall::WALL_PROTOCOL_VERSION,
        "resource": payload.descriptor, "dataBase64": BASE64.encode(&payload.bytes) }),
    )
}
