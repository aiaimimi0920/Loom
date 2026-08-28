//! Validation for host-materialized resources passed to an untrusted runtime.

use std::fs;

use super::*;

pub(super) fn validate_staged_resources(invocation: &CapabilityInvocation) -> HostResult<()> {
    if invocation.resource_refs.len() != invocation.staged_resources.len() {
        return Err(CapabilityHostError::Protocol(
            "every resource reference requires one host-staged resource".to_owned(),
        ));
    }
    for (reference, staged) in invocation
        .resource_refs
        .iter()
        .zip(&invocation.staged_resources)
    {
        if reference != &staged.resource_ref
            || !staged.staged_path.is_absolute()
            || staged.staged_path.as_os_str().is_empty()
        {
            return Err(CapabilityHostError::Protocol(
                "staged resource identity does not match its opaque reference".to_owned(),
            ));
        }
        let metadata = fs::symlink_metadata(&staged.staged_path).map_err(|_| {
            CapabilityHostError::Protocol("staged resource is unavailable".to_owned())
        })?;
        if !metadata.is_file()
            || metadata.file_type().is_symlink()
            || !metadata.permissions().readonly()
        {
            return Err(CapabilityHostError::Protocol(
                "staged resource must be a read-only regular file".to_owned(),
            ));
        }
    }
    Ok(())
}
