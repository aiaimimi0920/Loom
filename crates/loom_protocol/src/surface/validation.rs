//! Validation for untrusted Surface protocol envelopes.

use std::collections::BTreeSet;

use thiserror::Error;

use super::actions::{
    SurfaceActionCancelRequest, SurfaceActionInvocation, SurfaceConfirmationDecision,
    SurfaceConfirmationRequest,
};
use super::manifest::SurfaceRuntimeKind;
use super::resources::{SurfaceResourceDescriptor, SurfaceResourceLease};
use super::scene::{SurfaceEvent, SurfaceNode, SurfacePatch, SurfaceSnapshot};
use super::SURFACE_PROTOCOL_VERSION;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SurfaceValidationError {
    #[error("unsupported Surface protocol `{0}`")]
    UnsupportedProtocol(String),
    #[error("Surface identifier is not safe: {0}")]
    UnsafeIdentifier(String),
    #[error("Surface node id is duplicated: {0}")]
    DuplicateNodeId(String),
    #[error("Surface node type is empty for node {0}")]
    EmptyNodeType(String),
    #[error("Surface patch revision {revision} does not advance base revision {base_revision}")]
    InvalidPatchRevision { base_revision: u64, revision: u64 },
    #[error("Surface resource id must use sha256 content addressing: {0}")]
    InvalidResourceId(String),
    #[error("Surface runtime entry is invalid: {0}")]
    InvalidRuntimeEntry(String),
    #[error("Surface confirmation is invalid: {0}")]
    InvalidConfirmation(String),
}

pub fn is_safe_surface_identifier(value: &str) -> bool {
    let value = value.trim();
    !value.is_empty()
        && value.len() <= 160
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':' | b'/')
        })
}

pub fn validate_surface_protocol(protocol_version: &str) -> Result<(), SurfaceValidationError> {
    if protocol_version == SURFACE_PROTOCOL_VERSION {
        Ok(())
    } else {
        Err(SurfaceValidationError::UnsupportedProtocol(
            protocol_version.to_owned(),
        ))
    }
}

pub fn validate_surface_node_tree(root: &SurfaceNode) -> Result<(), SurfaceValidationError> {
    let mut seen = BTreeSet::new();
    let mut pending = vec![root];
    while let Some(node) = pending.pop() {
        if !is_safe_surface_identifier(&node.id) {
            return Err(SurfaceValidationError::UnsafeIdentifier(node.id.clone()));
        }
        if node.node_type.trim().is_empty() {
            return Err(SurfaceValidationError::EmptyNodeType(node.id.clone()));
        }
        if !seen.insert(node.id.clone()) {
            return Err(SurfaceValidationError::DuplicateNodeId(node.id.clone()));
        }
        for action in node.events.values() {
            if !is_safe_surface_identifier(action) {
                return Err(SurfaceValidationError::UnsafeIdentifier(action.clone()));
            }
        }
        // Reverse insertion preserves the original left-to-right depth-first error order.
        pending.extend(node.children.iter().rev());
    }
    Ok(())
}

pub fn validate_surface_snapshot(snapshot: &SurfaceSnapshot) -> Result<(), SurfaceValidationError> {
    validate_surface_protocol(&snapshot.protocol_version)?;
    for id in [
        &snapshot.instance_id,
        &snapshot.attachment_id,
        &snapshot.art_id,
    ] {
        if !is_safe_surface_identifier(id) {
            return Err(SurfaceValidationError::UnsafeIdentifier((*id).clone()));
        }
    }
    if snapshot
        .view_id
        .as_deref()
        .is_some_and(|view_id| !is_safe_surface_identifier(view_id))
    {
        return Err(SurfaceValidationError::UnsafeIdentifier(
            snapshot.view_id.clone().unwrap_or_default(),
        ));
    }
    validate_surface_node_tree(&snapshot.scene)?;
    for resource in &snapshot.resources {
        validate_surface_resource(resource)?;
    }
    for lease in &snapshot.resource_leases {
        validate_surface_resource_lease(lease)?;
    }
    if snapshot.runtime == SurfaceRuntimeKind::Javascript {
        let entry_resource_id = snapshot.entry_resource_id.as_deref().ok_or_else(|| {
            SurfaceValidationError::InvalidRuntimeEntry(
                "JavaScript Surface has no entry resource".to_owned(),
            )
        })?;
        if !snapshot
            .resource_leases
            .iter()
            .any(|lease| lease.resource.resource_id == entry_resource_id)
        {
            return Err(SurfaceValidationError::InvalidRuntimeEntry(
                "JavaScript entry resource is not leased by the snapshot".to_owned(),
            ));
        }
    }
    Ok(())
}

pub fn validate_surface_patch(patch: &SurfacePatch) -> Result<(), SurfaceValidationError> {
    validate_surface_protocol(&patch.protocol_version)?;
    if patch.revision <= patch.base_revision {
        return Err(SurfaceValidationError::InvalidPatchRevision {
            base_revision: patch.base_revision,
            revision: patch.revision,
        });
    }
    for id in [&patch.instance_id, &patch.attachment_id] {
        if !is_safe_surface_identifier(id) {
            return Err(SurfaceValidationError::UnsafeIdentifier((*id).clone()));
        }
    }
    for resource in &patch.resources {
        validate_surface_resource(resource)?;
    }
    for lease in &patch.resource_leases {
        validate_surface_resource_lease(lease)?;
    }
    Ok(())
}

pub fn validate_surface_event(event: &SurfaceEvent) -> Result<(), SurfaceValidationError> {
    validate_surface_protocol(&event.protocol_version)?;
    for id in [
        &event.instance_id,
        &event.attachment_id,
        &event.event_id,
        &event.node_id,
        &event.event,
    ] {
        if !is_safe_surface_identifier(id) {
            return Err(SurfaceValidationError::UnsafeIdentifier((*id).clone()));
        }
    }
    if let Some(action) = &event.action {
        if !is_safe_surface_identifier(action) {
            return Err(SurfaceValidationError::UnsafeIdentifier(action.clone()));
        }
    }
    Ok(())
}

pub fn validate_surface_action_invocation(
    invocation: &SurfaceActionInvocation,
) -> Result<(), SurfaceValidationError> {
    validate_surface_protocol(&invocation.protocol_version)?;
    for id in [
        &invocation.instance_id,
        &invocation.attachment_id,
        &invocation.request_id,
        &invocation.event_id,
        &invocation.action_id,
    ] {
        if !is_safe_surface_identifier(id) {
            return Err(SurfaceValidationError::UnsafeIdentifier((*id).clone()));
        }
    }
    Ok(())
}

pub fn validate_surface_confirmation_request(
    request: &SurfaceConfirmationRequest,
) -> Result<(), SurfaceValidationError> {
    validate_surface_protocol(&request.protocol_version)?;
    for id in [
        &request.confirmation_id,
        &request.instance_id,
        &request.attachment_id,
        &request.device_id,
        &request.hook_node_id,
        &request.event_id,
        &request.request_id,
        &request.action_id,
    ] {
        if !is_safe_surface_identifier(id) {
            return Err(SurfaceValidationError::UnsafeIdentifier((*id).clone()));
        }
    }
    if request.expires_at_ms == 0 {
        return Err(SurfaceValidationError::InvalidConfirmation(
            "confirmation expiry must be positive".to_owned(),
        ));
    }
    Ok(())
}

pub fn validate_surface_confirmation_decision(
    decision: &SurfaceConfirmationDecision,
) -> Result<(), SurfaceValidationError> {
    validate_surface_protocol(&decision.protocol_version)?;
    for id in [
        &decision.confirmation_id,
        &decision.instance_id,
        &decision.attachment_id,
        &decision.device_id,
    ] {
        if !is_safe_surface_identifier(id) {
            return Err(SurfaceValidationError::UnsafeIdentifier((*id).clone()));
        }
    }
    Ok(())
}

pub fn validate_surface_action_cancel_request(
    request: &SurfaceActionCancelRequest,
) -> Result<(), SurfaceValidationError> {
    validate_surface_protocol(&request.protocol_version)?;
    for id in [
        &request.instance_id,
        &request.request_id,
        &request.device_id,
    ] {
        if !is_safe_surface_identifier(id) {
            return Err(SurfaceValidationError::UnsafeIdentifier((*id).clone()));
        }
    }
    Ok(())
}

pub fn validate_surface_resource(
    resource: &SurfaceResourceDescriptor,
) -> Result<(), SurfaceValidationError> {
    let digest = resource.resource_id.strip_prefix("sha256:");
    if !digest.is_some_and(|digest| {
        digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
    }) {
        return Err(SurfaceValidationError::InvalidResourceId(
            resource.resource_id.clone(),
        ));
    }
    Ok(())
}

pub fn validate_surface_resource_lease(
    lease: &SurfaceResourceLease,
) -> Result<(), SurfaceValidationError> {
    if !is_safe_surface_identifier(&lease.lease_id) {
        return Err(SurfaceValidationError::UnsafeIdentifier(
            lease.lease_id.clone(),
        ));
    }
    validate_surface_resource(&lease.resource)?;
    if let Some(stream_id) = lease.transport.stream_id.as_deref() {
        if !is_safe_surface_identifier(stream_id) {
            return Err(SurfaceValidationError::UnsafeIdentifier(
                stream_id.to_owned(),
            ));
        }
    }
    Ok(())
}
