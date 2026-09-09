//! Validation for host-materialized resources passed to an untrusted runtime.

use std::fs;

use loom_security::metadata_has_link_semantics;

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
            || metadata_has_link_semantics(&metadata)
            || !metadata.permissions().readonly()
        {
            return Err(CapabilityHostError::Protocol(
                "staged resource must be a read-only regular file".to_owned(),
            ));
        }
        // The runtime reads the file, not the reference, so the declared length has to describe
        // what is actually on disk. Everything downstream - the plugin's own budget checks and
        // the 512 MiB ceiling `validate_resources` enforces on `byte_length` - is derived from
        // that number, and a staged file larger than its reference would slip past all of them.
        if metadata.len() != reference.byte_length {
            return Err(CapabilityHostError::Protocol(
                "staged resource length does not match its opaque reference".to_owned(),
            ));
        }
    }
    Ok(())
}
