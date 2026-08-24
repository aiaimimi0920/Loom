// Surface remounting, runtime validation, and authoritative state migration.
#[allow(clippy::too_many_arguments)]
fn remount_surface_attachments(
    instance_id: &str,
    attachment_states: &[(String, loom_protocol::SurfaceLifecycleState)],
    surface_instances: &SharedSurfaceInstanceStore,
    tool_registry: &ToolRegistry,
    framework_registry: &FrameworkRegistry,
    control_plane_root: &Path,
    hook_bridge: &SharedHookBridgeRuntime,
    surface_resources: &SharedSurfaceResourceStore,
    shared_images: &SharedImageStoreHandle,
) -> Result<()> {
    for (attachment_id, previous_lifecycle) in attachment_states {
        if *previous_lifecycle == loom_protocol::SurfaceLifecycleState::Disposed {
            continue;
        }
        let body = json!({ "attachmentId": attachment_id }).to_string();
        let (status, response) = mount_surface_instance(
            instance_id,
            &body,
            surface_instances,
            tool_registry,
            framework_registry,
            control_plane_root,
            hook_bridge,
            surface_resources,
            shared_images,
        )?;
        if status != 200 {
            anyhow::bail!("target Surface remount returned {status}: {response}");
        }
        if matches!(
            previous_lifecycle,
            loom_protocol::SurfaceLifecycleState::Active
                | loom_protocol::SurfaceLifecycleState::Inactive
                | loom_protocol::SurfaceLifecycleState::Suspended
        ) {
            let attachment = surface_instances
                .lock()
                .map_err(|_| anyhow::anyhow!("Surface instance store is unavailable"))?
                .get(instance_id)
                .and_then(|instance| instance.attachments.get(attachment_id).cloned())
                .ok_or_else(|| anyhow::anyhow!("remounted Surface attachment disappeared"))?;
            let event = SurfaceLifecycleEvent {
                protocol_version: loom_protocol::SURFACE_PROTOCOL_VERSION.to_owned(),
                instance_id: instance_id.to_owned(),
                attachment_id: attachment_id.clone(),
                state: previous_lifecycle.clone(),
                revision: attachment.lifecycle_revision.saturating_add(1),
            };
            let body = serde_json::to_string(&event)?;
            let (status, response) = transition_surface_lifecycle(
                instance_id,
                &body,
                surface_instances,
                hook_bridge,
                surface_resources,
                shared_images,
                None,
            )?;
            if status != 200 {
                anyhow::bail!("target Surface lifecycle restore returned {status}: {response}");
            }
        }
    }
    Ok(())
}

fn validate_surface_runtime_entries(
    control_plane_root: &Path,
    art_dir: &Path,
    manifest: &loom_protocol::SurfacePackageManifest,
) -> Result<()> {
    for variant in &manifest.variants {
        let path = resolve_surface_package_entry(control_plane_root, art_dir, &variant.entry)?;
        let metadata = fs::metadata(&path)?;
        match variant.runtime {
            SurfaceRuntimeKind::Declarative => {
                if metadata.len() > MAX_SURFACE_SCENE_BYTES {
                    anyhow::bail!("Surface scene exceeds {MAX_SURFACE_SCENE_BYTES} bytes");
                }
                serde_json::from_slice::<DeclarativeSurfaceFile>(&fs::read(path)?)?;
            }
            SurfaceRuntimeKind::Javascript => {
                drop(metadata);
                load_surface_javascript_source(control_plane_root, art_dir, variant)?;
            }
            SurfaceRuntimeKind::Shader | SurfaceRuntimeKind::LoomRemote => {
                if metadata.len() > MAX_SURFACE_SCENE_BYTES {
                    anyhow::bail!("Surface runtime descriptor is too large");
                }
            }
        }
    }
    if let Some(fallback) = manifest.fallback_scene.as_deref() {
        let path = resolve_surface_package_entry(control_plane_root, art_dir, fallback)?;
        if fs::metadata(&path)?.len() > MAX_SURFACE_SCENE_BYTES {
            anyhow::bail!("Surface fallback exceeds {MAX_SURFACE_SCENE_BYTES} bytes");
        }
        serde_json::from_slice::<DeclarativeSurfaceFile>(&fs::read(path)?)?;
    }
    Ok(())
}

fn migrate_surface_state(
    control_plane_root: &Path,
    art_dir: &Path,
    manifest: &loom_protocol::SurfacePackageManifest,
    source_schema: u32,
    mut state: Value,
) -> Result<Value> {
    if source_schema > manifest.state_schema_version {
        anyhow::bail!(
            "target state schema {} is older than current schema {} and no rollback checkpoint exists",
            manifest.state_schema_version,
            source_schema
        );
    }
    let mut current = source_schema;
    let mut steps = 0_u32;
    while current < manifest.state_schema_version {
        let migration = manifest
            .migrations
            .iter()
            .find(|migration| migration.from == current)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Surface state migration chain stops at schema {current} before {}",
                    manifest.state_schema_version
                )
            })?;
        let path = resolve_surface_package_entry(control_plane_root, art_dir, &migration.entry)?;
        if fs::metadata(&path)?.len() > MAX_SURFACE_SCENE_BYTES {
            anyhow::bail!("Surface state migration is too large");
        }
        let document: SurfaceStateMigrationFile = serde_json::from_slice(&fs::read(path)?)?;
        if document.from != migration.from || document.to != migration.to {
            anyhow::bail!(
                "Surface state migration {} does not match manifest step {} -> {}",
                migration.entry,
                migration.from,
                migration.to
            );
        }
        apply_surface_state_merge_patch(&mut state, &document.state_patch);
        current = migration.to;
        steps = steps.saturating_add(1);
        if steps > 32 {
            anyhow::bail!("Surface state migration chain exceeds 32 steps");
        }
    }
    Ok(state)
}

fn apply_surface_state_merge_patch(target: &mut Value, patch: &Value) {
    match patch {
        Value::Object(patch) => {
            if !target.is_object() {
                *target = json!({});
            }
            let target = target.as_object_mut().expect("state was normalized");
            for (key, value) in patch {
                if value.is_null() {
                    target.remove(key);
                } else {
                    apply_surface_state_merge_patch(
                        target.entry(key.clone()).or_insert(Value::Null),
                        value,
                    );
                }
            }
        }
        value => *target = value.clone(),
    }
}
