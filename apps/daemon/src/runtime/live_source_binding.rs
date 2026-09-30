// Paired capture sources need no synthetic Art. Viewer/Surface grants keep their attachment checks.
fn validate_live_source_binding(
    surfaces: &SharedSurfaceInstanceStore,
    instance: Option<&str>,
    attachment: Option<&str>,
    device: &str,
    hook: Option<&str>,
    authenticated: Option<&str>,
    source_operation: bool,
) -> std::result::Result<(), LiveRuntimeError> {
    if source_binding_is_standalone(
        instance,
        attachment,
        device,
        authenticated,
        source_operation,
    )? {
        return Ok(());
    }
    validate_live_attachment(
        surfaces,
        instance.expect("validated binding"),
        attachment.expect("validated binding"),
        device,
        hook,
        authenticated,
    )
}

fn source_binding_is_standalone(
    instance: Option<&str>,
    attachment: Option<&str>,
    device: &str,
    authenticated: Option<&str>,
    source_operation: bool,
) -> std::result::Result<bool, LiveRuntimeError> {
    match (instance, attachment) {
        (Some(_), Some(_)) => Ok(false),
        (None, None) if source_operation && authenticated == Some(device) => Ok(true),
        (None, None) => Err(LiveRuntimeError::new(
            403,
            "live_source_binding_required",
            "only an authenticated source operation may omit its Surface binding",
        )),
        _ => Err(LiveRuntimeError::new(
            400,
            "live_source_binding_invalid",
            "Surface instance and attachment must be provided together",
        )),
    }
}

#[cfg(test)]
mod live_source_binding_tests {
    use super::*;
    #[test]
    fn standalone_source_requires_its_paired_identity_and_never_grants_viewer_authority() {
        assert!(source_binding_is_standalone(None, None, "source", Some("source"), true).unwrap());
        for actor in [None, Some("other")] {
            assert!(source_binding_is_standalone(None, None, "source", actor, true).is_err());
        }
        assert!(source_binding_is_standalone(None, None, "source", Some("source"), false).is_err());
        assert!(source_binding_is_standalone(
            Some("instance"),
            None,
            "source",
            Some("source"),
            true
        )
        .is_err());
        assert!(!source_binding_is_standalone(
            Some("instance"),
            Some("attachment"),
            "source",
            None,
            true
        )
        .unwrap());
    }
}
