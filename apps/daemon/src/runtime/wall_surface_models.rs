// Exact wall view envelopes keep presenter authority outside the Surface action protocol.
type WallSurfaceResult<T> = std::result::Result<T, WallStoreError>;
type WallSurfaceLinks = BTreeMap<(String, String), crate::wall_store::WallSurfaceLink>;

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WallSurfaceView {
    binding: crate::wall_store::WallInputBinding,
    instance_id: String,
    attachment_id: String,
}

impl WallSurfaceView {
    fn key(&self) -> (String, String) {
        (self.binding.endpoint_id.clone(), self.instance_id.clone())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WallSurfaceOpen {
    binding: crate::wall_store::WallInputBinding,
    instance_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WallSurfaceRead {
    view: WallSurfaceView,
    snapshot_revision: Option<u64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WallSurfaceClose {
    view: WallSurfaceView,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WallSurfaceImage {
    view: WallSurfaceView,
    resource_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WallSurfaceEvent {
    view: WallSurfaceView,
    placement_id: String,
    pixel: loom_protocol::wall::TilePixelPoint,
    sequence: u64,
    event: SurfaceEvent,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WallSurfaceDecision {
    view: WallSurfaceView,
    placement_id: String,
    pixel: loom_protocol::wall::TilePixelPoint,
    confirmation_id: String,
    approved: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WallSurfaceCancel {
    view: WallSurfaceView,
    request_id: String,
}

struct WallSurfaceServices<'a> {
    walls: &'a SharedWallStore,
    instances: &'a SharedSurfaceInstanceStore,
    actions: &'a SharedSurfaceActionExecutor,
    resources: &'a SharedSurfaceResourceStore,
    shared_images: &'a SharedImageStoreHandle,
    bridge: &'a SharedHookBridgeRuntime,
    tools: &'a ToolRegistry,
    frameworks: &'a FrameworkRegistry,
    root: &'a Path,
}

fn wall_surface_store_error(error: SurfaceStoreError) -> WallStoreError {
    WallStoreError::new(
        error.status_code(),
        error.code(),
        "Surface view operation was rejected",
    )
}

fn wall_surface_link<'a>(
    links: &'a WallSurfaceLinks,
    view: &WallSurfaceView,
    actor: &str,
) -> WallSurfaceResult<&'a crate::wall_store::WallSurfaceLink> {
    let link = links.get(&view.key()).ok_or_else(|| {
        WallStoreError::new(
            409,
            "wall_surface_detached",
            "Surface view is no longer attached",
        )
    })?;
    if link.closing
        || link.device_id != actor
        || link.binding != view.binding
        || link.attachment_id != view.attachment_id
    {
        return Err(WallStoreError::new(
            403,
            "wall_surface_forbidden",
            "Surface view binding does not match its presenter",
        ));
    }
    Ok(link)
}
