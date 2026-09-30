// A wall mirror shares its source view, even when the Art creates independent instances.
fn surface_patch_targets(
    instance: &crate::surface_store::SurfaceInstanceRecord,
    event_attachment: &str,
    explicit_attachment: Option<&str>,
) -> Vec<String> {
    if explicit_attachment.is_none()
        && instance.descriptor.instance_mode == SurfaceInstanceMode::Shared
    {
        return instance
            .attachments
            .values()
            .filter(|attachment| attachment.snapshot.is_some())
            .map(|attachment| attachment.descriptor.attachment_id.clone())
            .collect();
    }
    let target = explicit_attachment.unwrap_or(event_attachment);
    let Some(attachment) = instance.attachments.get(target) else {
        return vec![target.to_owned()];
    };
    let source = attachment.mirror_of.as_deref().unwrap_or(target);
    instance
        .attachments
        .values()
        .filter(|candidate| {
            candidate.snapshot.is_some()
                && (candidate.descriptor.attachment_id == source
                    || candidate.mirror_of.as_deref() == Some(source))
        })
        .map(|candidate| candidate.descriptor.attachment_id.clone())
        .collect()
}
