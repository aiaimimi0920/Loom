//! Image input leases scoped to the owning task; reuses Extension image permissions, Surface store, and Capability broker.
use super::*;
use loom_capability_runtime::CapabilityHostError;
use loom_protocol::local_art::service::images::Image;

pub(super) struct Context {
    pub resources: SharedCapabilityResourceBroker,
    pub surface_resources: SharedSurfaceResourceStore,
    pub image_output: bool,
    pub confirmable: bool,
}
pub(super) struct Prepared {
    pub references: Vec<ExtensionResourceRef>,
    pub staged: crate::capability_resources::CapabilityResourceLease,
    // `staged` drops first, followed by the input store lease; empty inputs still follow the same RAII path.
    _uploads: ExtensionResourceUploadLease,
}
pub(super) fn prepare(
    runtime: &SharedCapabilityRuntime,
    command: &str,
    images: Vec<Image>,
    context: Context,
) -> Result<Prepared, CapabilityHostError> {
    let plugin = runtime
        .command_owner(command)?
        .ok_or_else(|| CapabilityHostError::NotFound(command.into()))?;
    let scope = runtime
        .plugin_scope(&plugin)?
        .ok_or_else(|| CapabilityHostError::NotFound(plugin.clone()))?;
    let snapshot = runtime.contribution_snapshot()?;
    let mut references = Vec::new();
    let uploads = images
        .into_iter()
        .map(|image| ExtensionResourceUpload {
            kind: SurfaceResourceKind::Image,
            mime: "image/png".into(),
            data_base64: image.png,
        })
        .collect();
    let uploads = stage_image_uploads(
        uploads,
        &mut references,
        &snapshot,
        &plugin,
        command,
        &context.surface_resources,
    )
    .map_err(|_| CapabilityHostError::Protocol("local Art image input rejected".into()))?;
    let staged = context
        .resources
        .stage(
            &context.surface_resources,
            &plugin,
            &scope,
            &format!("local-art-resource:{}", Uuid::new_v4()),
            &references,
        )
        .map_err(|_| {
            CapabilityHostError::Unavailable("local Art resource staging failed".into())
        })?;
    for resource in staged.resources() {
        let mut signature = [0; 8];
        std::fs::File::open(&resource.staged_path)?.read_exact(&mut signature)?;
        if signature != *b"\x89PNG\r\n\x1a\n" {
            return Err(CapabilityHostError::Protocol(
                "local Art input is not PNG".into(),
            ));
        }
    }
    if runtime.generation() != snapshot.generation
        || runtime.command_owner(command)?.as_deref() != Some(&plugin)
    {
        return Err(CapabilityHostError::Unavailable(
            "local Art command changed".into(),
        ));
    }
    Ok(Prepared {
        references,
        staged,
        _uploads: uploads,
    })
}
