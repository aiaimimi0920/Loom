use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use loom_protocol::{
    is_safe_surface_identifier, validate_surface_action_cancel_request,
    validate_surface_confirmation_decision, validate_surface_confirmation_request,
    validate_surface_event, validate_surface_patch, validate_surface_protocol,
    validate_surface_resource, validate_surface_snapshot, SurfaceActionAck,
    SurfaceActionCancelRequest, SurfaceActionRisk, SurfaceActionStatus,
    SurfaceAttachmentDescriptor, SurfaceConfirmationDecision, SurfaceConfirmationRequest,
    SurfaceEvent, SurfaceEventClass, SurfaceExecutionError, SurfaceExecutionFailure,
    SurfaceHostCapabilities, SurfaceInstanceDescriptor, SurfaceInstanceMode,
    SurfaceInstancePersistence, SurfaceLifecycleEvent, SurfaceLifecycleState, SurfaceNode,
    SurfacePatch, SurfacePatchOperation, SurfacePortValue, SurfacePreviewCommit,
    SurfaceResultCommit, SurfaceSnapshot,
};
use semver::Version;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;
use uuid::Uuid;

use super::{create_sensitive_temporary, replace_sensitive_file, unix_time_millis};

const SURFACE_STORE_SCHEMA_VERSION: u32 = 1;
const MAX_PENDING_SURFACE_EVENTS: usize = 1024;
const MAX_PENDING_SURFACE_CONFIRMATIONS: usize = 64;
const SURFACE_CONFIRMATION_TTL_MILLIS: u64 = 2 * 60 * 1000;

pub(crate) type SharedSurfaceInstanceStore = Arc<Mutex<SurfaceInstanceStore>>;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SurfaceAttachmentRecord {
    pub descriptor: SurfaceAttachmentDescriptor,
    pub lifecycle: SurfaceLifecycleState,
    #[serde(default)]
    pub lifecycle_revision: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host_capabilities: Option<SurfaceHostCapabilities>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<SurfaceSnapshot>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SurfaceInstanceRecord {
    pub descriptor: SurfaceInstanceDescriptor,
    #[serde(default)]
    pub attachments: BTreeMap<String, SurfaceAttachmentRecord>,
    #[serde(default)]
    pub authoritative_state: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_preview: Option<SurfacePreviewCommit>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_result: Option<SurfaceResultCommit>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_failure: Option<SurfaceExecutionFailure>,
    #[serde(default)]
    pub pending_events: Vec<SurfaceEvent>,
    #[serde(default)]
    pub event_acks: BTreeMap<String, SurfaceActionAck>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub pending_confirmations: BTreeMap<String, SurfacePendingConfirmation>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub migration_history: Vec<SurfaceMigrationCheckpoint>,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SurfaceMigrationCheckpoint {
    pub art_version: String,
    pub package_digest: String,
    pub state_schema_version: u32,
    #[serde(default)]
    pub authoritative_state: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_preview: Option<SurfacePreviewCommit>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_result: Option<SurfaceResultCommit>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SurfacePendingConfirmation {
    pub request: SurfaceConfirmationRequest,
    pub event: SurfaceEvent,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum SurfaceConfirmationResolution {
    Approved {
        event: SurfaceEvent,
        ack: SurfaceActionAck,
    },
    Rejected {
        ack: SurfaceActionAck,
    },
    Expired {
        ack: SurfaceActionAck,
    },
}

#[derive(Debug, Error)]
pub(crate) enum SurfaceStoreError {
    #[error("Surface store I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("Surface store JSON is invalid: {0}")]
    Json(#[from] serde_json::Error),
    #[error("unsupported Surface store schema {0}")]
    UnsupportedSchema(u32),
    #[error("invalid Surface request: {0}")]
    Invalid(String),
    #[error("Surface resource was not found: {0}")]
    NotFound(String),
    #[error("Surface state conflict: {0}")]
    Conflict(String),
}

impl SurfaceStoreError {
    pub(crate) fn status_code(&self) -> u16 {
        match self {
            Self::Invalid(_) | Self::Json(_) => 400,
            Self::NotFound(_) => 404,
            Self::Conflict(_) => 409,
            Self::Io(_) | Self::UnsupportedSchema(_) => 500,
        }
    }

    pub(crate) fn code(&self) -> &'static str {
        match self {
            Self::Invalid(_) | Self::Json(_) => "invalid_surface_request",
            Self::NotFound(_) => "surface_not_found",
            Self::Conflict(_) => "surface_conflict",
            Self::Io(_) => "surface_store_io_failed",
            Self::UnsupportedSchema(_) => "surface_store_schema_unsupported",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SurfaceStoreDocument {
    schema_version: u32,
    #[serde(default)]
    instances: BTreeMap<String, SurfaceInstanceRecord>,
}

pub(crate) struct SurfaceInstanceStore {
    path: PathBuf,
    instances: BTreeMap<String, SurfaceInstanceRecord>,
    latest_continuous_events: BTreeMap<String, SurfaceEvent>,
}

impl SurfaceInstanceStore {
    pub(crate) fn new(path: impl AsRef<Path>) -> Result<Self, SurfaceStoreError> {
        let path = path.as_ref().to_path_buf();
        let instances = match fs::read(&path) {
            Ok(bytes) => {
                let document = serde_json::from_slice::<SurfaceStoreDocument>(&bytes)?;
                if document.schema_version != SURFACE_STORE_SCHEMA_VERSION {
                    return Err(SurfaceStoreError::UnsupportedSchema(
                        document.schema_version,
                    ));
                }
                document
                    .instances
                    .into_iter()
                    .filter(|(_, record)| {
                        record.descriptor.persistence == SurfaceInstancePersistence::Persistent
                    })
                    .collect()
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => BTreeMap::new(),
            Err(error) => return Err(error.into()),
        };
        Ok(Self {
            path,
            instances,
            latest_continuous_events: BTreeMap::new(),
        })
    }

    pub(crate) fn list(&self) -> Vec<SurfaceInstanceRecord> {
        self.instances.values().cloned().collect()
    }

    pub(crate) fn get(&self, instance_id: &str) -> Option<SurfaceInstanceRecord> {
        self.instances.get(instance_id).cloned()
    }

    pub(crate) fn event_ack(&self, instance_id: &str, event_id: &str) -> Option<SurfaceActionAck> {
        self.instances
            .get(instance_id)
            .and_then(|instance| instance.event_acks.get(event_id))
            .cloned()
    }

    pub(crate) fn pending_events(&self) -> Vec<SurfaceEvent> {
        self.instances
            .values()
            .flat_map(|instance| instance.pending_events.iter().cloned())
            .collect()
    }

    pub(crate) fn pending_confirmations(&self) -> Vec<SurfaceConfirmationRequest> {
        self.instances
            .values()
            .flat_map(|instance| {
                instance
                    .pending_confirmations
                    .values()
                    .map(|pending| pending.request.clone())
            })
            .collect()
    }

    pub(crate) fn create(
        &mut self,
        art_id: &str,
        art_version: &str,
        package_digest: &str,
        state_schema_version: u32,
        persistence: SurfaceInstancePersistence,
        instance_mode: SurfaceInstanceMode,
    ) -> Result<SurfaceInstanceRecord, SurfaceStoreError> {
        validate_identity(art_id, "Art id")?;
        Version::parse(art_version).map_err(|error| {
            SurfaceStoreError::Invalid(format!("Art version is not valid semver: {error}"))
        })?;
        let package_digest = normalize_package_digest(package_digest)?;
        let now = unix_time_millis();
        let instance_id = format!("instance:{}", Uuid::new_v4());
        let record = SurfaceInstanceRecord {
            descriptor: SurfaceInstanceDescriptor {
                instance_id: instance_id.clone(),
                art_id: art_id.to_owned(),
                art_version: art_version.to_owned(),
                package_digest,
                instance_mode,
                state_schema_version,
                persistence,
                generation: 0,
                surface_revision: 0,
                preview_revision: 0,
                result_revision: 0,
            },
            attachments: BTreeMap::new(),
            authoritative_state: Value::Object(Default::default()),
            latest_preview: None,
            latest_result: None,
            last_failure: None,
            pending_events: Vec::new(),
            event_acks: BTreeMap::new(),
            pending_confirmations: BTreeMap::new(),
            migration_history: Vec::new(),
            created_at_ms: now,
            updated_at_ms: now,
        };
        self.transaction(|instances| {
            instances.insert(instance_id, record.clone());
            Ok(record)
        })
    }

    pub(crate) fn find_shared(
        &self,
        art_id: &str,
        art_version: &str,
        package_digest: &str,
        persistence: &SurfaceInstancePersistence,
    ) -> Option<SurfaceInstanceRecord> {
        self.instances
            .values()
            .find(|instance| {
                instance.descriptor.instance_mode == SurfaceInstanceMode::Shared
                    && instance.descriptor.art_id == art_id
                    && instance.descriptor.art_version == art_version
                    && instance.descriptor.package_digest == package_digest
                    && &instance.descriptor.persistence == persistence
            })
            .cloned()
    }

    pub(crate) fn delete(&mut self, instance_id: &str) -> Result<(), SurfaceStoreError> {
        self.transaction(|instances| {
            instances
                .remove(instance_id)
                .ok_or_else(|| SurfaceStoreError::NotFound(instance_id.to_owned()))?;
            Ok(())
        })
    }

    pub(crate) fn migrate_instance(
        &mut self,
        instance_id: &str,
        expected_generation: Option<u64>,
        target_version: &str,
        target_digest: &str,
        target_state_schema_version: u32,
        migrated_state: Value,
    ) -> Result<SurfaceInstanceRecord, SurfaceStoreError> {
        Version::parse(target_version).map_err(|error| {
            SurfaceStoreError::Invalid(format!("target Art version is not valid semver: {error}"))
        })?;
        let target_digest = normalize_package_digest(target_digest)?;
        if target_state_schema_version == 0 {
            return Err(SurfaceStoreError::Invalid(
                "target state schema version must be at least 1".to_owned(),
            ));
        }
        self.transaction(|instances| {
            let instance = instance_mut(instances, instance_id)?;
            if expected_generation
                .is_some_and(|expected| expected != instance.descriptor.generation)
            {
                return Err(SurfaceStoreError::Conflict(format!(
                    "expected generation {} but current generation is {}",
                    expected_generation.unwrap_or_default(),
                    instance.descriptor.generation
                )));
            }
            if !instance.pending_events.is_empty() {
                return Err(SurfaceStoreError::Conflict(
                    "Surface instance has pending actions and cannot migrate".to_owned(),
                ));
            }
            if instance.descriptor.art_version == target_version
                && instance.descriptor.package_digest == target_digest
                && instance.descriptor.state_schema_version == target_state_schema_version
            {
                return Ok(instance.clone());
            }
            let rollback = instance
                .migration_history
                .iter()
                .rposition(|checkpoint| {
                    checkpoint.art_version == target_version
                        && checkpoint.package_digest == target_digest
                        && checkpoint.state_schema_version == target_state_schema_version
                })
                .map(|index| instance.migration_history.remove(index));
            let current_checkpoint = SurfaceMigrationCheckpoint {
                art_version: instance.descriptor.art_version.clone(),
                package_digest: instance.descriptor.package_digest.clone(),
                state_schema_version: instance.descriptor.state_schema_version,
                authoritative_state: instance.authoritative_state.clone(),
                latest_preview: instance.latest_preview.clone(),
                latest_result: instance.latest_result.clone(),
            };
            instance.migration_history.push(current_checkpoint);
            if instance.migration_history.len() > 8 {
                instance.migration_history.remove(0);
            }
            instance.descriptor.art_version = target_version.to_owned();
            instance.descriptor.package_digest = target_digest;
            instance.descriptor.state_schema_version = target_state_schema_version;
            instance.descriptor.generation = instance.descriptor.generation.saturating_add(1);
            instance.authoritative_state = rollback
                .as_ref()
                .map(|checkpoint| checkpoint.authoritative_state.clone())
                .unwrap_or(migrated_state);
            instance.latest_preview = rollback
                .as_ref()
                .and_then(|checkpoint| checkpoint.latest_preview.clone());
            instance.latest_result = rollback
                .as_ref()
                .and_then(|checkpoint| checkpoint.latest_result.clone());
            if let Some(preview) = instance.latest_preview.as_mut() {
                preview.generation = instance.descriptor.generation;
            }
            if let Some(result) = instance.latest_result.as_mut() {
                result.generation = instance.descriptor.generation;
            }
            instance.last_failure = None;
            instance.pending_events.clear();
            instance.event_acks.clear();
            instance.pending_confirmations.clear();
            for attachment in instance.attachments.values_mut() {
                attachment.snapshot = None;
                if attachment.lifecycle != SurfaceLifecycleState::Disposed {
                    attachment.lifecycle = SurfaceLifecycleState::Created;
                    attachment.lifecycle_revision = attachment.lifecycle_revision.saturating_add(1);
                }
            }
            instance.updated_at_ms = unix_time_millis();
            Ok(instance.clone())
        })
    }

    pub(crate) fn attach(
        &mut self,
        instance_id: &str,
        hook_node_id: &str,
        device_id: &str,
        host_capabilities: Option<SurfaceHostCapabilities>,
    ) -> Result<SurfaceAttachmentRecord, SurfaceStoreError> {
        validate_identity(hook_node_id, "Hook node id")?;
        validate_identity(device_id, "device id")?;
        self.transaction(|instances| {
            let instance = instance_mut(instances, instance_id)?;
            if let Some(existing) = instance.attachments.values().find(|attachment| {
                attachment.descriptor.hook_node_id == hook_node_id
                    && attachment.descriptor.device_id == device_id
            }) {
                return Ok(existing.clone());
            }
            let attachment_id = format!("attachment:{}", Uuid::new_v4());
            let record = SurfaceAttachmentRecord {
                descriptor: SurfaceAttachmentDescriptor {
                    attachment_id: attachment_id.clone(),
                    instance_id: instance_id.to_owned(),
                    hook_node_id: hook_node_id.to_owned(),
                    device_id: device_id.to_owned(),
                },
                lifecycle: SurfaceLifecycleState::Created,
                lifecycle_revision: 0,
                host_capabilities,
                snapshot: None,
            };
            instance.attachments.insert(attachment_id, record.clone());
            instance.updated_at_ms = unix_time_millis();
            Ok(record)
        })
    }

    pub(crate) fn put_snapshot(
        &mut self,
        instance_id: &str,
        mut snapshot: SurfaceSnapshot,
    ) -> Result<SurfaceInstanceRecord, SurfaceStoreError> {
        validate_surface_snapshot(&snapshot)
            .map_err(|error| SurfaceStoreError::Invalid(error.to_string()))?;
        self.transaction(|instances| {
            let instance = instance_mut(instances, instance_id)?;
            validate_snapshot_identity(instance, instance_id, &snapshot)?;
            let has_authoritative_state = !instance.authoritative_state.is_null()
                && !instance
                    .authoritative_state
                    .as_object()
                    .is_some_and(serde_json::Map::is_empty);
            if !has_authoritative_state {
                instance.authoritative_state = snapshot.authoritative_state.clone();
            } else {
                snapshot.authoritative_state = instance.authoritative_state.clone();
            }
            let attachment = attachment_mut(instance, &snapshot.attachment_id)?;
            if let Some(previous) = attachment.snapshot.as_ref() {
                if snapshot.revision < previous.revision {
                    return Err(SurfaceStoreError::Conflict(format!(
                        "snapshot revision {} is older than {}",
                        snapshot.revision, previous.revision
                    )));
                }
                if snapshot.revision == previous.revision {
                    if &snapshot == previous {
                        return Ok(instance.clone());
                    }
                    return Err(SurfaceStoreError::Conflict(format!(
                        "snapshot revision {} already contains different state",
                        snapshot.revision
                    )));
                }
            }
            attachment.snapshot = Some(snapshot.clone());
            if attachment.lifecycle == SurfaceLifecycleState::Disposed {
                return Err(SurfaceStoreError::Conflict(
                    "a disposed Surface attachment cannot be remounted".to_owned(),
                ));
            }
            if attachment.lifecycle == SurfaceLifecycleState::Created {
                attachment.lifecycle = SurfaceLifecycleState::Mounted;
                attachment.lifecycle_revision = attachment.lifecycle_revision.saturating_add(1);
            }
            instance.descriptor.surface_revision =
                instance.descriptor.surface_revision.max(snapshot.revision);
            instance.updated_at_ms = unix_time_millis();
            Ok(instance.clone())
        })
    }

    pub(crate) fn apply_patch(
        &mut self,
        instance_id: &str,
        patch: SurfacePatch,
    ) -> Result<SurfaceInstanceRecord, SurfaceStoreError> {
        validate_surface_patch(&patch)
            .map_err(|error| SurfaceStoreError::Invalid(error.to_string()))?;
        self.transaction(|instances| {
            let instance = instance_mut(instances, instance_id)?;
            if patch.instance_id != instance_id {
                return Err(SurfaceStoreError::Invalid(
                    "patch instance id does not match route".to_owned(),
                ));
            }
            let attachment = attachment_mut(instance, &patch.attachment_id)?;
            let snapshot = attachment.snapshot.as_mut().ok_or_else(|| {
                SurfaceStoreError::Conflict("an initial snapshot is required before patches".into())
            })?;
            if patch.base_revision != snapshot.revision {
                return Err(SurfaceStoreError::Conflict(format!(
                    "patch base revision {} does not match current revision {}",
                    patch.base_revision, snapshot.revision
                )));
            }

            let mut next = snapshot.clone();
            for operation in &patch.operations {
                apply_operation(&mut next.scene, operation)?;
            }
            merge_json(&mut next.authoritative_state, &patch.state_patch);
            merge_resources(&mut next.resources, &patch.resources);
            merge_resource_leases(&mut next.resource_leases, &patch.resource_leases);
            next.revision = patch.revision;
            loom_protocol::validate_surface_node_tree(&next.scene)
                .map_err(|error| SurfaceStoreError::Invalid(error.to_string()))?;
            *snapshot = next;
            merge_json(&mut instance.authoritative_state, &patch.state_patch);
            instance.descriptor.surface_revision =
                instance.descriptor.surface_revision.max(patch.revision);
            instance.updated_at_ms = unix_time_millis();
            Ok(instance.clone())
        })
    }

    pub(crate) fn begin_generation(
        &mut self,
        instance_id: &str,
        expected_generation: Option<u64>,
    ) -> Result<SurfaceInstanceDescriptor, SurfaceStoreError> {
        self.transaction(|instances| {
            let instance = instance_mut(instances, instance_id)?;
            if let Some(expected) = expected_generation {
                if expected != instance.descriptor.generation {
                    return Err(SurfaceStoreError::Conflict(format!(
                        "expected generation {expected}, current generation is {}",
                        instance.descriptor.generation
                    )));
                }
            }
            instance.descriptor.generation = instance.descriptor.generation.saturating_add(1);
            instance.last_failure = None;
            instance.updated_at_ms = unix_time_millis();
            Ok(instance.descriptor.clone())
        })
    }

    pub(crate) fn transition_lifecycle(
        &mut self,
        instance_id: &str,
        event: SurfaceLifecycleEvent,
    ) -> Result<SurfaceAttachmentRecord, SurfaceStoreError> {
        validate_surface_protocol(&event.protocol_version)
            .map_err(|error| SurfaceStoreError::Invalid(error.to_string()))?;
        validate_identity(&event.instance_id, "instance id")?;
        validate_identity(&event.attachment_id, "attachment id")?;
        if event.instance_id != instance_id {
            return Err(SurfaceStoreError::Invalid(
                "lifecycle instance id does not match route".to_owned(),
            ));
        }
        self.transaction(|instances| {
            let instance = instance_mut(instances, instance_id)?;
            let attachment = attachment_mut(instance, &event.attachment_id)?;
            if event.revision < attachment.lifecycle_revision {
                return Err(SurfaceStoreError::Conflict(format!(
                    "lifecycle revision {} is older than {}",
                    event.revision, attachment.lifecycle_revision
                )));
            }
            if event.revision == attachment.lifecycle_revision {
                if event.state == attachment.lifecycle {
                    return Ok(attachment.clone());
                }
                return Err(SurfaceStoreError::Conflict(
                    "lifecycle revision already contains a different state".to_owned(),
                ));
            }
            if event.revision != attachment.lifecycle_revision.saturating_add(1) {
                return Err(SurfaceStoreError::Conflict(format!(
                    "lifecycle revision {} must advance {} by exactly one",
                    event.revision, attachment.lifecycle_revision
                )));
            }
            if !lifecycle_transition_allowed(&attachment.lifecycle, &event.state) {
                return Err(SurfaceStoreError::Conflict(format!(
                    "invalid Surface lifecycle transition {:?} -> {:?}",
                    attachment.lifecycle, event.state
                )));
            }
            attachment.lifecycle = event.state;
            attachment.lifecycle_revision = event.revision;
            if attachment.lifecycle == SurfaceLifecycleState::Disposed {
                attachment.snapshot = None;
                attachment.host_capabilities = None;
            }
            let result = attachment.clone();
            if result.lifecycle == SurfaceLifecycleState::Disposed {
                let confirmation_ids = instance
                    .pending_confirmations
                    .iter()
                    .filter(|(_, pending)| {
                        pending.request.attachment_id == result.descriptor.attachment_id
                    })
                    .map(|(id, _)| id.clone())
                    .collect::<Vec<_>>();
                for confirmation_id in confirmation_ids {
                    let Some(pending) = instance.pending_confirmations.remove(&confirmation_id)
                    else {
                        continue;
                    };
                    if let Some(ack) = instance.event_acks.get_mut(&pending.event.event_id) {
                        ack.status = SurfaceActionStatus::Cancelled;
                        ack.error = Some(SurfaceExecutionError {
                            code: "surface_attachment_disposed".to_owned(),
                            message: "Surface attachment was disposed before confirmation"
                                .to_owned(),
                            detail: None,
                        });
                    }
                }
            }
            instance.updated_at_ms = unix_time_millis();
            Ok(result)
        })
    }

    pub(crate) fn commit_preview(
        &mut self,
        instance_id: &str,
        commit: SurfacePreviewCommit,
    ) -> Result<SurfaceInstanceRecord, SurfaceStoreError> {
        validate_surface_protocol(&commit.protocol_version)
            .map_err(|error| SurfaceStoreError::Invalid(error.to_string()))?;
        validate_identity(&commit.request_id, "request id")?;
        validate_identity(&commit.port_id, "preview port id")?;
        validate_port_value(&commit.value)?;
        self.transaction(|instances| {
            let instance = instance_mut(instances, instance_id)?;
            validate_commit_identity(
                instance,
                instance_id,
                &commit.instance_id,
                commit.generation,
            )?;
            if commit.preview_revision <= instance.descriptor.preview_revision {
                return Err(SurfaceStoreError::Conflict(format!(
                    "preview revision {} does not advance {}",
                    commit.preview_revision, instance.descriptor.preview_revision
                )));
            }
            instance.descriptor.preview_revision = commit.preview_revision;
            instance.latest_preview = Some(commit);
            instance.updated_at_ms = unix_time_millis();
            Ok(instance.clone())
        })
    }

    pub(crate) fn commit_result(
        &mut self,
        instance_id: &str,
        commit: SurfaceResultCommit,
    ) -> Result<SurfaceInstanceRecord, SurfaceStoreError> {
        validate_surface_protocol(&commit.protocol_version)
            .map_err(|error| SurfaceStoreError::Invalid(error.to_string()))?;
        validate_identity(&commit.request_id, "request id")?;
        if commit.outputs.is_empty() {
            return Err(SurfaceStoreError::Invalid(
                "formal result must contain at least one output".to_owned(),
            ));
        }
        for (port_id, value) in &commit.outputs {
            validate_identity(port_id, "formal output port id")?;
            validate_port_value(value)?;
        }
        self.transaction(|instances| {
            let instance = instance_mut(instances, instance_id)?;
            validate_commit_identity(
                instance,
                instance_id,
                &commit.instance_id,
                commit.generation,
            )?;
            if commit.result_revision <= instance.descriptor.result_revision {
                return Err(SurfaceStoreError::Conflict(format!(
                    "result revision {} does not advance {}",
                    commit.result_revision, instance.descriptor.result_revision
                )));
            }
            let mut next_state = instance.authoritative_state.clone();
            merge_json(&mut next_state, &commit.state_patch);
            instance.authoritative_state = next_state;
            instance.descriptor.result_revision = commit.result_revision;
            instance.latest_result = Some(commit);
            instance.last_failure = None;
            instance.updated_at_ms = unix_time_millis();
            Ok(instance.clone())
        })
    }

    pub(crate) fn record_failure(
        &mut self,
        instance_id: &str,
        mut failure: SurfaceExecutionFailure,
    ) -> Result<SurfaceInstanceRecord, SurfaceStoreError> {
        validate_surface_protocol(&failure.protocol_version)
            .map_err(|error| SurfaceStoreError::Invalid(error.to_string()))?;
        validate_identity(&failure.request_id, "request id")?;
        self.transaction(|instances| {
            let instance = instance_mut(instances, instance_id)?;
            validate_commit_identity(
                instance,
                instance_id,
                &failure.instance_id,
                failure.generation,
            )?;
            failure.last_successful_result_revision = instance
                .latest_result
                .as_ref()
                .map(|result| result.result_revision);
            instance.last_failure = Some(failure);
            instance.updated_at_ms = unix_time_millis();
            Ok(instance.clone())
        })
    }

    pub(crate) fn accept_event(
        &mut self,
        instance_id: &str,
        event: SurfaceEvent,
    ) -> Result<SurfaceActionAck, SurfaceStoreError> {
        validate_surface_event(&event)
            .map_err(|error| SurfaceStoreError::Invalid(error.to_string()))?;
        if event.class == SurfaceEventClass::Local {
            return Err(SurfaceStoreError::Invalid(
                "local Surface events must not cross the Loom boundary".to_owned(),
            ));
        }
        let action = event.action.as_deref().ok_or_else(|| {
            SurfaceStoreError::Invalid("remote Surface event has no declared action".to_owned())
        })?;
        if let Some(existing) = self
            .instances
            .get(instance_id)
            .and_then(|instance| instance.event_acks.get(&event.event_id))
        {
            return Ok(existing.clone());
        }
        let ack = {
            let instance = self
                .instances
                .get(instance_id)
                .ok_or_else(|| SurfaceStoreError::NotFound(instance_id.to_owned()))?;
            validate_surface_event_context(instance, instance_id, &event, action)?;
            SurfaceActionAck {
                protocol_version: loom_protocol::SURFACE_PROTOCOL_VERSION.to_owned(),
                instance_id: instance_id.to_owned(),
                event_id: event.event_id.clone(),
                request_id: surface_request_id(&event.event_id),
                accepted: true,
                status: SurfaceActionStatus::Queued,
                error: None,
            }
        };

        if event.class == SurfaceEventClass::Continuous {
            let key = format!("{}:{}:{}", event.instance_id, event.node_id, action);
            self.latest_continuous_events.insert(key, event);
            return Ok(ack);
        }

        self.transaction(|instances| {
            let instance = instance_mut(instances, instance_id)?;
            if instance.pending_events.len() >= MAX_PENDING_SURFACE_EVENTS {
                return Err(SurfaceStoreError::Conflict(
                    "Surface action queue is full".to_owned(),
                ));
            }
            instance.pending_events.push(event.clone());
            instance
                .event_acks
                .insert(event.event_id.clone(), ack.clone());
            instance.updated_at_ms = unix_time_millis();
            Ok(ack)
        })
    }

    pub(crate) fn await_confirmation(
        &mut self,
        instance_id: &str,
        event: SurfaceEvent,
        risk: SurfaceActionRisk,
    ) -> Result<(SurfaceActionAck, SurfaceConfirmationRequest), SurfaceStoreError> {
        validate_surface_event(&event)
            .map_err(|error| SurfaceStoreError::Invalid(error.to_string()))?;
        if event.class == SurfaceEventClass::Local {
            return Err(SurfaceStoreError::Invalid(
                "local Surface events must not cross the Loom boundary".to_owned(),
            ));
        }
        if event.class == SurfaceEventClass::Continuous {
            return Err(SurfaceStoreError::Invalid(
                "continuous Surface events cannot require confirmation".to_owned(),
            ));
        }
        let action = event.action.as_deref().ok_or_else(|| {
            SurfaceStoreError::Invalid("remote Surface event has no declared action".to_owned())
        })?;
        if let Some(instance) = self.instances.get(instance_id) {
            if let Some(existing) = instance.event_acks.get(&event.event_id) {
                let pending = instance
                    .pending_confirmations
                    .values()
                    .find(|pending| pending.event.event_id == event.event_id)
                    .ok_or_else(|| {
                        SurfaceStoreError::Conflict(
                            "Surface event already has a non-confirmation action state".to_owned(),
                        )
                    })?;
                return Ok((existing.clone(), pending.request.clone()));
            }
        }
        let (attachment, request_id) = {
            let instance = self
                .instances
                .get(instance_id)
                .ok_or_else(|| SurfaceStoreError::NotFound(instance_id.to_owned()))?;
            validate_surface_event_context(instance, instance_id, &event, action)?;
            let attachment = instance
                .attachments
                .get(&event.attachment_id)
                .ok_or_else(|| SurfaceStoreError::NotFound(event.attachment_id.clone()))?
                .descriptor
                .clone();
            (attachment, surface_request_id(&event.event_id))
        };
        let request = SurfaceConfirmationRequest {
            protocol_version: loom_protocol::SURFACE_PROTOCOL_VERSION.to_owned(),
            confirmation_id: format!("confirmation:{}", Uuid::new_v4()),
            instance_id: instance_id.to_owned(),
            attachment_id: event.attachment_id.clone(),
            device_id: attachment.device_id,
            hook_node_id: attachment.hook_node_id,
            event_id: event.event_id.clone(),
            request_id: request_id.clone(),
            action_id: action.to_owned(),
            risk,
            expires_at_ms: unix_time_millis().saturating_add(SURFACE_CONFIRMATION_TTL_MILLIS),
            payload: event.payload.clone(),
        };
        validate_surface_confirmation_request(&request)
            .map_err(|error| SurfaceStoreError::Invalid(error.to_string()))?;
        let ack = SurfaceActionAck {
            protocol_version: loom_protocol::SURFACE_PROTOCOL_VERSION.to_owned(),
            instance_id: instance_id.to_owned(),
            event_id: event.event_id.clone(),
            request_id,
            accepted: true,
            status: SurfaceActionStatus::AwaitingConfirmation,
            error: None,
        };
        self.transaction(|instances| {
            let instance = instance_mut(instances, instance_id)?;
            if instance.pending_confirmations.len() >= MAX_PENDING_SURFACE_CONFIRMATIONS {
                return Err(SurfaceStoreError::Conflict(
                    "Surface confirmation queue is full".to_owned(),
                ));
            }
            instance
                .event_acks
                .insert(event.event_id.clone(), ack.clone());
            instance.pending_confirmations.insert(
                request.confirmation_id.clone(),
                SurfacePendingConfirmation {
                    request: request.clone(),
                    event,
                },
            );
            instance.updated_at_ms = unix_time_millis();
            Ok((ack, request))
        })
    }

    pub(crate) fn resolve_confirmation(
        &mut self,
        decision: SurfaceConfirmationDecision,
    ) -> Result<SurfaceConfirmationResolution, SurfaceStoreError> {
        validate_surface_confirmation_decision(&decision)
            .map_err(|error| SurfaceStoreError::Invalid(error.to_string()))?;
        self.transaction(|instances| {
            let instance = instance_mut(instances, &decision.instance_id)?;
            let pending = instance
                .pending_confirmations
                .get(&decision.confirmation_id)
                .cloned()
                .ok_or_else(|| SurfaceStoreError::NotFound(decision.confirmation_id.clone()))?;
            if pending.request.instance_id != decision.instance_id
                || pending.request.attachment_id != decision.attachment_id
                || pending.request.device_id != decision.device_id
            {
                return Err(SurfaceStoreError::Invalid(
                    "Surface confirmation decision identity does not match the request".to_owned(),
                ));
            }
            let mut ack = instance
                .event_acks
                .get(&pending.event.event_id)
                .cloned()
                .ok_or_else(|| {
                    SurfaceStoreError::Conflict(
                        "Surface confirmation action acknowledgement is missing".to_owned(),
                    )
                })?;
            instance
                .pending_confirmations
                .remove(&decision.confirmation_id);
            let resolution = if pending.request.expires_at_ms <= unix_time_millis() {
                ack.status = SurfaceActionStatus::Failed;
                ack.error = Some(SurfaceExecutionError {
                    code: "surface_confirmation_expired".to_owned(),
                    message: "Surface action confirmation expired".to_owned(),
                    detail: None,
                });
                SurfaceConfirmationResolution::Expired { ack: ack.clone() }
            } else if !decision.approved {
                ack.status = SurfaceActionStatus::Cancelled;
                ack.error = Some(SurfaceExecutionError {
                    code: "surface_confirmation_rejected".to_owned(),
                    message: "Surface action was rejected by the user".to_owned(),
                    detail: None,
                });
                SurfaceConfirmationResolution::Rejected { ack: ack.clone() }
            } else {
                if instance.pending_events.len() >= MAX_PENDING_SURFACE_EVENTS {
                    return Err(SurfaceStoreError::Conflict(
                        "Surface action queue is full".to_owned(),
                    ));
                }
                ack.status = SurfaceActionStatus::Queued;
                ack.error = None;
                instance.pending_events.push(pending.event.clone());
                SurfaceConfirmationResolution::Approved {
                    event: pending.event,
                    ack: ack.clone(),
                }
            };
            instance.event_acks.insert(ack.event_id.clone(), ack);
            instance.updated_at_ms = unix_time_millis();
            Ok(resolution)
        })
    }

    pub(crate) fn expire_confirmations(
        &mut self,
    ) -> Result<Vec<SurfaceActionAck>, SurfaceStoreError> {
        let now = unix_time_millis();
        self.transaction(|instances| {
            let mut expired = Vec::new();
            for instance in instances.values_mut() {
                let ids = instance
                    .pending_confirmations
                    .iter()
                    .filter(|(_, pending)| pending.request.expires_at_ms <= now)
                    .map(|(id, _)| id.clone())
                    .collect::<Vec<_>>();
                for id in ids {
                    let Some(pending) = instance.pending_confirmations.remove(&id) else {
                        continue;
                    };
                    let Some(mut ack) = instance.event_acks.get(&pending.event.event_id).cloned()
                    else {
                        continue;
                    };
                    ack.status = SurfaceActionStatus::Failed;
                    ack.error = Some(SurfaceExecutionError {
                        code: "surface_confirmation_expired".to_owned(),
                        message: "Surface action confirmation expired".to_owned(),
                        detail: None,
                    });
                    instance
                        .event_acks
                        .insert(ack.event_id.clone(), ack.clone());
                    instance.updated_at_ms = now;
                    expired.push(ack);
                }
            }
            Ok(expired)
        })
    }

    pub(crate) fn request_cancel(
        &mut self,
        request: SurfaceActionCancelRequest,
    ) -> Result<(SurfaceEvent, SurfaceActionAck), SurfaceStoreError> {
        validate_surface_action_cancel_request(&request)
            .map_err(|error| SurfaceStoreError::Invalid(error.to_string()))?;
        self.transaction(|instances| {
            let instance = instance_mut(instances, &request.instance_id)?;
            let ack = instance
                .event_acks
                .values()
                .find(|ack| ack.request_id == request.request_id)
                .cloned()
                .ok_or_else(|| SurfaceStoreError::NotFound(request.request_id.clone()))?;
            if !matches!(
                ack.status,
                SurfaceActionStatus::Queued
                    | SurfaceActionStatus::Running
                    | SurfaceActionStatus::CancelRequested
            ) {
                return Err(SurfaceStoreError::Conflict(format!(
                    "Surface action cannot be cancelled while {:?}",
                    ack.status
                )));
            }
            let event = instance
                .pending_events
                .iter()
                .find(|event| event.event_id == ack.event_id)
                .cloned()
                .ok_or_else(|| {
                    SurfaceStoreError::Conflict(
                        "Surface action is no longer pending or running".to_owned(),
                    )
                })?;
            let attachment = instance
                .attachments
                .get(&event.attachment_id)
                .ok_or_else(|| SurfaceStoreError::NotFound(event.attachment_id.clone()))?;
            if attachment.descriptor.device_id != request.device_id {
                return Err(SurfaceStoreError::Invalid(
                    "Surface cancel device does not own the action attachment".to_owned(),
                ));
            }
            let mut cancel_ack = ack;
            cancel_ack.status = SurfaceActionStatus::CancelRequested;
            cancel_ack.error = None;
            instance
                .event_acks
                .insert(cancel_ack.event_id.clone(), cancel_ack.clone());
            instance.updated_at_ms = unix_time_millis();
            Ok((event, cancel_ack))
        })
    }

    pub(crate) fn update_event_ack(
        &mut self,
        mut ack: SurfaceActionAck,
        remove_pending: bool,
    ) -> Result<SurfaceActionAck, SurfaceStoreError> {
        validate_surface_protocol(&ack.protocol_version)
            .map_err(|error| SurfaceStoreError::Invalid(error.to_string()))?;
        validate_identity(&ack.instance_id, "instance id")?;
        validate_identity(&ack.event_id, "event id")?;
        validate_identity(&ack.request_id, "request id")?;
        self.transaction(|instances| {
            let instance = instance_mut(instances, &ack.instance_id)?;
            if let Some(previous) = instance.event_acks.get(&ack.event_id) {
                if previous.request_id != ack.request_id {
                    return Err(SurfaceStoreError::Conflict(
                        "Surface event request identity changed".to_owned(),
                    ));
                }
                ack.accepted = previous.accepted;
            }
            instance
                .event_acks
                .insert(ack.event_id.clone(), ack.clone());
            if remove_pending {
                instance
                    .pending_events
                    .retain(|event| event.event_id != ack.event_id);
            }
            instance.updated_at_ms = unix_time_millis();
            Ok(ack)
        })
    }

    fn transaction<T>(
        &mut self,
        change: impl FnOnce(
            &mut BTreeMap<String, SurfaceInstanceRecord>,
        ) -> Result<T, SurfaceStoreError>,
    ) -> Result<T, SurfaceStoreError> {
        let previous = self.instances.clone();
        let output = match change(&mut self.instances) {
            Ok(output) => output,
            Err(error) => {
                self.instances = previous;
                return Err(error);
            }
        };
        if let Err(error) = self.persist() {
            self.instances = previous;
            return Err(error);
        }
        Ok(output)
    }

    fn persist(&self) -> Result<(), SurfaceStoreError> {
        let parent = self.path.parent().ok_or_else(|| {
            SurfaceStoreError::Invalid("Surface store path has no parent".to_owned())
        })?;
        fs::create_dir_all(parent)?;
        let document = SurfaceStoreDocument {
            schema_version: SURFACE_STORE_SCHEMA_VERSION,
            instances: self
                .instances
                .iter()
                .filter(|(_, record)| {
                    record.descriptor.persistence == SurfaceInstancePersistence::Persistent
                })
                .map(|(id, record)| (id.clone(), record.clone()))
                .collect(),
        };
        let mut bytes = serde_json::to_vec_pretty(&document)?;
        bytes.push(b'\n');
        let (temporary, mut file) = create_sensitive_temporary(&self.path)?;
        let result = (|| -> Result<(), SurfaceStoreError> {
            file.write_all(&bytes)?;
            file.sync_all()?;
            drop(file);
            replace_sensitive_file(&temporary, &self.path)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }
}

fn instance_mut<'a>(
    instances: &'a mut BTreeMap<String, SurfaceInstanceRecord>,
    instance_id: &str,
) -> Result<&'a mut SurfaceInstanceRecord, SurfaceStoreError> {
    instances
        .get_mut(instance_id)
        .ok_or_else(|| SurfaceStoreError::NotFound(instance_id.to_owned()))
}

fn attachment_mut<'a>(
    instance: &'a mut SurfaceInstanceRecord,
    attachment_id: &str,
) -> Result<&'a mut SurfaceAttachmentRecord, SurfaceStoreError> {
    instance
        .attachments
        .get_mut(attachment_id)
        .ok_or_else(|| SurfaceStoreError::NotFound(attachment_id.to_owned()))
}

fn validate_identity(value: &str, label: &str) -> Result<(), SurfaceStoreError> {
    if is_safe_surface_identifier(value) {
        Ok(())
    } else {
        Err(SurfaceStoreError::Invalid(format!(
            "{label} is not a safe Surface identifier"
        )))
    }
}

fn normalize_package_digest(value: &str) -> Result<String, SurfaceStoreError> {
    let digest = value.trim().strip_prefix("sha256:").unwrap_or(value.trim());
    if digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(SurfaceStoreError::Invalid(
            "package digest must be a SHA-256 hex value".to_owned(),
        ));
    }
    Ok(digest.to_ascii_lowercase())
}

fn validate_snapshot_identity(
    instance: &SurfaceInstanceRecord,
    route_instance_id: &str,
    snapshot: &SurfaceSnapshot,
) -> Result<(), SurfaceStoreError> {
    if snapshot.instance_id != route_instance_id {
        return Err(SurfaceStoreError::Invalid(
            "snapshot instance id does not match route".to_owned(),
        ));
    }
    if snapshot.art_id != instance.descriptor.art_id
        || snapshot.art_version != instance.descriptor.art_version
    {
        return Err(SurfaceStoreError::Conflict(
            "snapshot Art identity does not match locked instance package".to_owned(),
        ));
    }
    Ok(())
}

fn validate_commit_identity(
    instance: &SurfaceInstanceRecord,
    route_instance_id: &str,
    commit_instance_id: &str,
    generation: u64,
) -> Result<(), SurfaceStoreError> {
    if commit_instance_id != route_instance_id {
        return Err(SurfaceStoreError::Invalid(
            "commit instance id does not match route".to_owned(),
        ));
    }
    if generation != instance.descriptor.generation {
        return Err(SurfaceStoreError::Conflict(format!(
            "commit generation {generation} is stale; current generation is {}",
            instance.descriptor.generation
        )));
    }
    Ok(())
}

fn validate_port_value(value: &SurfacePortValue) -> Result<(), SurfaceStoreError> {
    match value {
        SurfacePortValue::Value { .. } => Ok(()),
        SurfacePortValue::Resource { resource } => validate_surface_resource(resource)
            .map_err(|error| SurfaceStoreError::Invalid(error.to_string())),
        SurfacePortValue::Stream { stream } => {
            validate_identity(&stream.stream_id, "stream id")?;
            if stream.item_type.trim().is_empty() {
                return Err(SurfaceStoreError::Invalid(
                    "stream item type cannot be empty".to_owned(),
                ));
            }
            Ok(())
        }
    }
}

fn validate_surface_event_context(
    instance: &SurfaceInstanceRecord,
    route_instance_id: &str,
    event: &SurfaceEvent,
    action: &str,
) -> Result<(), SurfaceStoreError> {
    if event.instance_id != route_instance_id {
        return Err(SurfaceStoreError::Invalid(
            "Surface event instance id does not match route".to_owned(),
        ));
    }
    if event.generation != instance.descriptor.generation {
        return Err(SurfaceStoreError::Conflict(format!(
            "Surface event generation {} is stale; current generation is {}",
            event.generation, instance.descriptor.generation
        )));
    }
    let attachment = instance
        .attachments
        .get(&event.attachment_id)
        .ok_or_else(|| SurfaceStoreError::NotFound(event.attachment_id.clone()))?;
    if !matches!(
        attachment.lifecycle,
        SurfaceLifecycleState::Mounted | SurfaceLifecycleState::Active
    ) {
        return Err(SurfaceStoreError::Conflict(format!(
            "Surface attachment is not interactive while {:?}",
            attachment.lifecycle
        )));
    }
    let snapshot = attachment.snapshot.as_ref().ok_or_else(|| {
        SurfaceStoreError::Conflict("Surface event attachment has no mounted snapshot".to_owned())
    })?;
    if event.base_revision != snapshot.revision {
        return Err(SurfaceStoreError::Conflict(format!(
            "Surface event base revision {} does not match current revision {}",
            event.base_revision, snapshot.revision
        )));
    }
    let node = find_node(&snapshot.scene, &event.node_id)
        .ok_or_else(|| SurfaceStoreError::NotFound(event.node_id.clone()))?;
    if node.events.get(&event.event).map(String::as_str) != Some(action) {
        return Err(SurfaceStoreError::Invalid(format!(
            "Surface node {} does not declare action {action} for event {}",
            event.node_id, event.event
        )));
    }
    Ok(())
}

fn lifecycle_transition_allowed(
    current: &SurfaceLifecycleState,
    next: &SurfaceLifecycleState,
) -> bool {
    use SurfaceLifecycleState::{Active, Created, Disposed, Inactive, Mounted, Suspended};
    matches!(
        (current, next),
        (Created, Mounted | Disposed)
            | (Mounted, Active | Inactive | Suspended | Disposed)
            | (Active, Inactive | Suspended | Disposed)
            | (Inactive, Active | Suspended | Disposed)
            | (Suspended, Active | Inactive | Disposed)
    )
}

fn surface_request_id(event_id: &str) -> String {
    format!(
        "request:{}",
        event_id.strip_prefix("event:").unwrap_or(event_id)
    )
}

fn merge_resources(
    target: &mut Vec<loom_protocol::SurfaceResourceDescriptor>,
    additions: &[loom_protocol::SurfaceResourceDescriptor],
) {
    for resource in additions {
        if let Some(existing) = target
            .iter_mut()
            .find(|existing| existing.resource_id == resource.resource_id)
        {
            *existing = resource.clone();
        } else {
            target.push(resource.clone());
        }
    }
}

fn merge_resource_leases(
    target: &mut Vec<loom_protocol::SurfaceResourceLease>,
    additions: &[loom_protocol::SurfaceResourceLease],
) {
    for lease in additions {
        if let Some(existing) = target
            .iter_mut()
            .find(|existing| existing.lease_id == lease.lease_id)
        {
            *existing = lease.clone();
        } else {
            target.push(lease.clone());
        }
    }
}

fn merge_json(target: &mut Value, patch: &Value) {
    match patch {
        Value::Object(patch) => {
            if !target.is_object() {
                *target = Value::Object(Default::default());
            }
            let target = target.as_object_mut().expect("object initialized");
            for (key, value) in patch {
                if value.is_null() {
                    target.remove(key);
                } else {
                    merge_json(target.entry(key.clone()).or_insert(Value::Null), value);
                }
            }
        }
        replacement => *target = replacement.clone(),
    }
}

fn apply_operation(
    root: &mut SurfaceNode,
    operation: &SurfacePatchOperation,
) -> Result<(), SurfaceStoreError> {
    match operation {
        SurfacePatchOperation::Set {
            node_id,
            path,
            value,
        } => mutate_node_json(root, node_id, path, Some(value.clone())),
        SurfacePatchOperation::Remove { node_id, path } => {
            mutate_node_json(root, node_id, path, None)
        }
        SurfacePatchOperation::InsertNode {
            parent_id,
            index,
            node,
        } => {
            let parent = find_node_mut(root, parent_id)
                .ok_or_else(|| SurfaceStoreError::NotFound(parent_id.clone()))?;
            if *index > parent.children.len() {
                return Err(SurfaceStoreError::Invalid(format!(
                    "insert index {index} is out of range"
                )));
            }
            parent.children.insert(*index, node.clone());
            Ok(())
        }
        SurfacePatchOperation::RemoveNode { node_id } => {
            if root.id == *node_id {
                return Err(SurfaceStoreError::Invalid(
                    "the Surface root node cannot be removed".to_owned(),
                ));
            }
            remove_node(root, node_id)
                .map(|_| ())
                .ok_or_else(|| SurfaceStoreError::NotFound(node_id.clone()))
        }
        SurfacePatchOperation::MoveNode {
            node_id,
            parent_id,
            index,
        } => {
            if root.id == *node_id {
                return Err(SurfaceStoreError::Invalid(
                    "the Surface root node cannot be moved".to_owned(),
                ));
            }
            let node = remove_node(root, node_id)
                .ok_or_else(|| SurfaceStoreError::NotFound(node_id.clone()))?;
            let parent = find_node_mut(root, parent_id).ok_or_else(|| {
                SurfaceStoreError::Invalid("a node cannot move into itself".into())
            })?;
            if *index > parent.children.len() {
                return Err(SurfaceStoreError::Invalid(format!(
                    "move index {index} is out of range"
                )));
            }
            parent.children.insert(*index, node);
            Ok(())
        }
        SurfacePatchOperation::ReplaceNode { node_id, node } => {
            if node.id != *node_id {
                return Err(SurfaceStoreError::Invalid(
                    "replacement node must preserve its stable id".to_owned(),
                ));
            }
            let target = find_node_mut(root, node_id)
                .ok_or_else(|| SurfaceStoreError::NotFound(node_id.clone()))?;
            *target = node.clone();
            Ok(())
        }
        SurfacePatchOperation::SetVisibility { node_id, visible } => {
            let node = find_node_mut(root, node_id)
                .ok_or_else(|| SurfaceStoreError::NotFound(node_id.clone()))?;
            if !node.props.is_object() {
                node.props = Value::Object(Default::default());
            }
            node.props
                .as_object_mut()
                .expect("object initialized")
                .insert("visible".to_owned(), Value::Bool(*visible));
            Ok(())
        }
        SurfacePatchOperation::SetBinding {
            node_id,
            path,
            binding,
        } => {
            let node = find_node_mut(root, node_id)
                .ok_or_else(|| SurfaceStoreError::NotFound(node_id.clone()))?;
            if !node.props.is_object() {
                node.props = Value::Object(Default::default());
            }
            let props = node.props.as_object_mut().expect("object initialized");
            let bindings = props
                .entry("bindings")
                .or_insert_with(|| Value::Object(Default::default()));
            if !bindings.is_object() {
                *bindings = Value::Object(Default::default());
            }
            bindings
                .as_object_mut()
                .expect("object initialized")
                .insert(path.clone(), Value::String(binding.clone()));
            Ok(())
        }
    }
}

fn find_node_mut<'a>(root: &'a mut SurfaceNode, id: &str) -> Option<&'a mut SurfaceNode> {
    if root.id == id {
        return Some(root);
    }
    root.children
        .iter_mut()
        .find_map(|child| find_node_mut(child, id))
}

fn find_node<'a>(root: &'a SurfaceNode, id: &str) -> Option<&'a SurfaceNode> {
    if root.id == id {
        return Some(root);
    }
    root.children.iter().find_map(|child| find_node(child, id))
}

fn remove_node(root: &mut SurfaceNode, id: &str) -> Option<SurfaceNode> {
    if let Some(index) = root.children.iter().position(|child| child.id == id) {
        return Some(root.children.remove(index));
    }
    root.children
        .iter_mut()
        .find_map(|child| remove_node(child, id))
}

fn mutate_node_json(
    root: &mut SurfaceNode,
    node_id: &str,
    path: &str,
    value: Option<Value>,
) -> Result<(), SurfaceStoreError> {
    let allowed = ["/props", "/layout", "/style", "/accessibility", "/events"];
    if !allowed
        .iter()
        .any(|prefix| path == *prefix || path.starts_with(&format!("{prefix}/")))
    {
        return Err(SurfaceStoreError::Invalid(
            "node patch path must target props, layout, style, accessibility, or events".to_owned(),
        ));
    }
    let node = find_node_mut(root, node_id)
        .ok_or_else(|| SurfaceStoreError::NotFound(node_id.to_owned()))?;
    let stable_id = node.id.clone();
    let mut encoded = serde_json::to_value(&*node)?;
    match value {
        Some(value) => set_json_pointer(&mut encoded, path, value)?,
        None => remove_json_pointer(&mut encoded, path)?,
    }
    let replacement = serde_json::from_value::<SurfaceNode>(encoded)?;
    if replacement.id != stable_id {
        return Err(SurfaceStoreError::Invalid(
            "node patch changed a stable id".to_owned(),
        ));
    }
    *node = replacement;
    Ok(())
}

fn pointer_tokens(path: &str) -> Result<Vec<String>, SurfaceStoreError> {
    if !path.starts_with('/') {
        return Err(SurfaceStoreError::Invalid(
            "node patch path must be a JSON pointer".to_owned(),
        ));
    }
    Ok(path[1..]
        .split('/')
        .map(|token| token.replace("~1", "/").replace("~0", "~"))
        .collect())
}

fn set_json_pointer(target: &mut Value, path: &str, value: Value) -> Result<(), SurfaceStoreError> {
    let tokens = pointer_tokens(path)?;
    let (last, parents) = tokens
        .split_last()
        .ok_or_else(|| SurfaceStoreError::Invalid("cannot replace the entire node".to_owned()))?;
    let mut cursor = target;
    for token in parents {
        if !cursor.is_object() {
            *cursor = Value::Object(Default::default());
        }
        cursor = cursor
            .as_object_mut()
            .expect("object initialized")
            .entry(token.clone())
            .or_insert_with(|| Value::Object(Default::default()));
    }
    if !cursor.is_object() {
        *cursor = Value::Object(Default::default());
    }
    cursor
        .as_object_mut()
        .expect("object initialized")
        .insert(last.clone(), value);
    Ok(())
}

fn remove_json_pointer(target: &mut Value, path: &str) -> Result<(), SurfaceStoreError> {
    let tokens = pointer_tokens(path)?;
    let (last, parents) = tokens
        .split_last()
        .ok_or_else(|| SurfaceStoreError::Invalid("cannot remove the entire node".to_owned()))?;
    let mut cursor = target;
    for token in parents {
        let Some(next) = cursor
            .as_object_mut()
            .and_then(|object| object.get_mut(token))
        else {
            return Ok(());
        };
        cursor = next;
    }
    if let Some(object) = cursor.as_object_mut() {
        object.remove(last);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use loom_protocol::{SurfaceResourceDescriptor, SurfaceResourceKind, SURFACE_PROTOCOL_VERSION};

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir()
            .join(format!("loom-surface-store-{name}-{}", Uuid::new_v4()))
            .join("instances.json")
    }

    fn create(store: &mut SurfaceInstanceStore) -> SurfaceInstanceRecord {
        store
            .create(
                "neuro.official/stock-price",
                "1.2.3",
                &"a".repeat(64),
                1,
                SurfaceInstancePersistence::Persistent,
                SurfaceInstanceMode::Independent,
            )
            .expect("create Surface instance")
    }

    fn snapshot(record: &SurfaceInstanceRecord, attachment_id: &str) -> SurfaceSnapshot {
        SurfaceSnapshot {
            protocol_version: SURFACE_PROTOCOL_VERSION.to_owned(),
            instance_id: record.descriptor.instance_id.clone(),
            attachment_id: attachment_id.to_owned(),
            art_id: record.descriptor.art_id.clone(),
            art_version: record.descriptor.art_version.clone(),
            revision: 1,
            runtime: loom_protocol::SurfaceRuntimeKind::Declarative,
            entry_resource_id: None,
            view_id: None,
            scene: SurfaceNode {
                id: "root".to_owned(),
                node_type: "column".to_owned(),
                children: vec![SurfaceNode {
                    id: "price".to_owned(),
                    node_type: "text".to_owned(),
                    props: serde_json::json!({"text": "100"}),
                    events: BTreeMap::from([("click".to_owned(), "refresh".to_owned())]),
                    ..SurfaceNode::default()
                }],
                ..SurfaceNode::default()
            },
            authoritative_state: serde_json::json!({"price": 100}),
            resources: Vec::new(),
            resource_leases: Vec::new(),
        }
    }

    #[test]
    fn persistent_instance_round_trips_and_temporary_instance_does_not() {
        let path = temp_path("round-trip");
        let persistent_id;
        {
            let mut store = SurfaceInstanceStore::new(&path).expect("open store");
            persistent_id = create(&mut store).descriptor.instance_id;
            store
                .create(
                    "neuro.official/temporary",
                    "0.1.0",
                    &"b".repeat(64),
                    1,
                    SurfaceInstancePersistence::Temporary,
                    SurfaceInstanceMode::Independent,
                )
                .expect("create temporary instance");
            assert_eq!(store.list().len(), 2);
        }
        let store = SurfaceInstanceStore::new(&path).expect("reload store");
        assert!(store.get(&persistent_id).is_some());
        assert_eq!(store.list().len(), 1);
        let _ = fs::remove_dir_all(path.parent().expect("parent"));
    }

    #[test]
    fn confirmation_is_persisted_identity_bound_and_queues_only_after_approval() {
        let path = temp_path("confirmation");
        let mut store = SurfaceInstanceStore::new(&path).expect("open store");
        let record = create(&mut store);
        let attachment = store
            .attach(
                &record.descriptor.instance_id,
                "hook-node:stock",
                "device-000-local",
                None,
            )
            .expect("attach Surface");
        store
            .put_snapshot(
                &record.descriptor.instance_id,
                snapshot(&record, &attachment.descriptor.attachment_id),
            )
            .expect("mount Surface");
        let event = SurfaceEvent {
            protocol_version: SURFACE_PROTOCOL_VERSION.to_owned(),
            instance_id: record.descriptor.instance_id.clone(),
            attachment_id: attachment.descriptor.attachment_id.clone(),
            event_id: "event:confirm-one".to_owned(),
            node_id: "price".to_owned(),
            event: "click".to_owned(),
            action: Some("refresh".to_owned()),
            class: SurfaceEventClass::Discrete,
            generation: 0,
            base_revision: 1,
            payload: serde_json::json!({"symbol": "MSFT"}),
        };
        let (ack, request) = store
            .await_confirmation(
                &record.descriptor.instance_id,
                event.clone(),
                SurfaceActionRisk::High,
            )
            .expect("await confirmation");
        assert_eq!(ack.status, SurfaceActionStatus::AwaitingConfirmation);
        assert!(store.pending_events().is_empty());
        assert_eq!(request.device_id, "device-000-local");
        assert_eq!(request.hook_node_id, "hook-node:stock");
        drop(store);

        let mut store = SurfaceInstanceStore::new(&path).expect("reload confirmation store");
        assert_eq!(store.pending_confirmations(), vec![request.clone()]);
        let mismatched = SurfaceConfirmationDecision {
            protocol_version: SURFACE_PROTOCOL_VERSION.to_owned(),
            confirmation_id: request.confirmation_id.clone(),
            instance_id: request.instance_id.clone(),
            attachment_id: request.attachment_id.clone(),
            device_id: "device-000-other".to_owned(),
            approved: true,
        };
        assert!(matches!(
            store.resolve_confirmation(mismatched),
            Err(SurfaceStoreError::Invalid(_))
        ));
        assert_eq!(store.pending_confirmations(), vec![request.clone()]);

        let approved = store
            .resolve_confirmation(SurfaceConfirmationDecision {
                protocol_version: SURFACE_PROTOCOL_VERSION.to_owned(),
                confirmation_id: request.confirmation_id,
                instance_id: request.instance_id,
                attachment_id: request.attachment_id,
                device_id: request.device_id,
                approved: true,
            })
            .expect("approve confirmation");
        let SurfaceConfirmationResolution::Approved {
            event: approved_event,
            ack: approved_ack,
        } = approved
        else {
            panic!("expected approved confirmation")
        };
        assert_eq!(approved_event, event);
        assert_eq!(approved_ack.status, SurfaceActionStatus::Queued);
        assert_eq!(store.pending_events(), vec![approved_event]);
        assert!(store.pending_confirmations().is_empty());
        let _ = fs::remove_dir_all(path.parent().expect("parent"));
    }

    #[test]
    fn invalid_store_json_is_not_silently_overwritten() {
        let path = temp_path("invalid-json");
        fs::create_dir_all(path.parent().expect("parent")).expect("create parent");
        fs::write(&path, b"{broken").expect("write broken JSON");
        assert!(matches!(
            SurfaceInstanceStore::new(&path),
            Err(SurfaceStoreError::Json(_))
        ));
        assert_eq!(fs::read(&path).expect("read original"), b"{broken");
        let _ = fs::remove_dir_all(path.parent().expect("parent"));
    }

    #[test]
    fn patch_requires_current_revision_and_updates_recoverable_snapshot() {
        let path = temp_path("patch");
        let mut store = SurfaceInstanceStore::new(&path).expect("open store");
        let record = create(&mut store);
        let attachment = store
            .attach(
                &record.descriptor.instance_id,
                "hook-node:1",
                "device:1",
                None,
            )
            .expect("attach");
        store
            .put_snapshot(
                &record.descriptor.instance_id,
                snapshot(&record, &attachment.descriptor.attachment_id),
            )
            .expect("put snapshot");
        let patch = SurfacePatch {
            protocol_version: SURFACE_PROTOCOL_VERSION.to_owned(),
            instance_id: record.descriptor.instance_id.clone(),
            attachment_id: attachment.descriptor.attachment_id,
            base_revision: 1,
            revision: 2,
            operations: vec![SurfacePatchOperation::Set {
                node_id: "price".to_owned(),
                path: "/props/text".to_owned(),
                value: serde_json::json!("101"),
            }],
            state_patch: serde_json::json!({"price": 101}),
            resources: Vec::new(),
            resource_leases: Vec::new(),
        };
        store
            .apply_patch(&record.descriptor.instance_id, patch.clone())
            .expect("apply patch");
        assert!(matches!(
            store.apply_patch(&record.descriptor.instance_id, patch),
            Err(SurfaceStoreError::Conflict(_))
        ));
        let reloaded = SurfaceInstanceStore::new(&path).expect("reload store");
        let record = reloaded
            .get(&record.descriptor.instance_id)
            .expect("stored record");
        let snapshot = record
            .attachments
            .values()
            .next()
            .and_then(|attachment| attachment.snapshot.as_ref())
            .expect("stored snapshot");
        assert_eq!(snapshot.revision, 2);
        assert_eq!(snapshot.scene.children[0].props["text"], "101");
        assert_eq!(record.authoritative_state["price"], 101);
        let _ = fs::remove_dir_all(path.parent().expect("parent"));
    }

    #[test]
    fn stale_generation_cannot_replace_preview_or_formal_result() {
        let path = temp_path("generation");
        let mut store = SurfaceInstanceStore::new(&path).expect("open store");
        let record = create(&mut store);
        let instance_id = record.descriptor.instance_id.clone();
        let generation = store
            .begin_generation(&instance_id, Some(0))
            .expect("begin generation")
            .generation;
        let resource = SurfaceResourceDescriptor {
            resource_id: format!("sha256:{}", "c".repeat(64)),
            kind: SurfaceResourceKind::Image,
            mime: "image/webp".to_owned(),
            size: 10,
            width: Some(2),
            height: Some(2),
        };
        store
            .commit_preview(
                &instance_id,
                SurfacePreviewCommit {
                    protocol_version: SURFACE_PROTOCOL_VERSION.to_owned(),
                    instance_id: instance_id.clone(),
                    request_id: "request:one".to_owned(),
                    generation,
                    preview_revision: 1,
                    port_id: "preview".to_owned(),
                    value: SurfacePortValue::Resource {
                        resource: resource.clone(),
                    },
                },
            )
            .expect("commit preview");
        store
            .begin_generation(&instance_id, Some(generation))
            .expect("begin newer generation");
        let result = SurfaceResultCommit {
            protocol_version: SURFACE_PROTOCOL_VERSION.to_owned(),
            instance_id: instance_id.clone(),
            request_id: "request:one".to_owned(),
            generation,
            result_revision: 1,
            outputs: BTreeMap::from([(
                "output".to_owned(),
                SurfacePortValue::Resource { resource },
            )]),
            state_patch: Value::Null,
        };
        assert!(matches!(
            store.commit_result(&instance_id, result),
            Err(SurfaceStoreError::Conflict(_))
        ));
        assert!(store
            .get(&instance_id)
            .expect("record")
            .latest_result
            .is_none());
        let _ = fs::remove_dir_all(path.parent().expect("parent"));
    }

    #[test]
    fn failure_preserves_last_successful_atomic_result() {
        let path = temp_path("failure");
        let mut store = SurfaceInstanceStore::new(&path).expect("open store");
        let record = create(&mut store);
        let instance_id = record.descriptor.instance_id;
        let generation = store
            .begin_generation(&instance_id, None)
            .expect("begin generation")
            .generation;
        store
            .commit_result(
                &instance_id,
                SurfaceResultCommit {
                    protocol_version: SURFACE_PROTOCOL_VERSION.to_owned(),
                    instance_id: instance_id.clone(),
                    request_id: "request:success".to_owned(),
                    generation,
                    result_revision: 1,
                    outputs: BTreeMap::from([(
                        "price".to_owned(),
                        SurfacePortValue::Value {
                            value: serde_json::json!(100),
                        },
                    )]),
                    state_patch: serde_json::json!({"price": 100}),
                },
            )
            .expect("commit result");
        let generation = store
            .begin_generation(&instance_id, Some(generation))
            .expect("begin failing generation")
            .generation;
        let failed = store
            .record_failure(
                &instance_id,
                SurfaceExecutionFailure {
                    protocol_version: SURFACE_PROTOCOL_VERSION.to_owned(),
                    instance_id: instance_id.clone(),
                    request_id: "request:failure".to_owned(),
                    generation,
                    error: loom_protocol::SurfaceExecutionError {
                        code: "offline".to_owned(),
                        message: "provider unavailable".to_owned(),
                        detail: None,
                    },
                    last_successful_result_revision: None,
                },
            )
            .expect("record failure");
        assert_eq!(
            failed.latest_result.expect("last result").result_revision,
            1
        );
        assert_eq!(
            failed
                .last_failure
                .expect("failure")
                .last_successful_result_revision,
            Some(1)
        );
        let _ = fs::remove_dir_all(path.parent().expect("parent"));
    }

    #[test]
    fn discrete_events_are_validated_deduplicated_and_persisted() {
        let path = temp_path("events");
        let mut store = SurfaceInstanceStore::new(&path).expect("open store");
        let record = create(&mut store);
        let instance_id = record.descriptor.instance_id.clone();
        let attachment = store
            .attach(&instance_id, "hook-node:1", "device:1", None)
            .expect("attach");
        store
            .put_snapshot(
                &instance_id,
                snapshot(&record, &attachment.descriptor.attachment_id),
            )
            .expect("put snapshot");
        let event = SurfaceEvent {
            protocol_version: SURFACE_PROTOCOL_VERSION.to_owned(),
            instance_id: instance_id.clone(),
            attachment_id: attachment.descriptor.attachment_id,
            event_id: "event:refresh-1".to_owned(),
            node_id: "price".to_owned(),
            event: "click".to_owned(),
            action: Some("refresh".to_owned()),
            class: SurfaceEventClass::Discrete,
            generation: 0,
            base_revision: 1,
            payload: Value::Null,
        };
        let first = store
            .accept_event(&instance_id, event.clone())
            .expect("accept event");
        let duplicate = store
            .accept_event(&instance_id, event)
            .expect("deduplicate event");
        assert_eq!(duplicate, first);
        let running = SurfaceActionAck {
            status: SurfaceActionStatus::Running,
            ..first
        };
        store
            .update_event_ack(running, false)
            .expect("mark action running");
        let succeeded = SurfaceActionAck {
            status: SurfaceActionStatus::Succeeded,
            ..store
                .event_ack(&instance_id, "event:refresh-1")
                .expect("running ack")
        };
        store
            .update_event_ack(succeeded, true)
            .expect("complete action");
        let reloaded = SurfaceInstanceStore::new(&path).expect("reload store");
        let record = reloaded.get(&instance_id).expect("record");
        assert!(record.pending_events.is_empty());
        assert_eq!(record.event_acks.len(), 1);
        assert_eq!(
            record.event_acks["event:refresh-1"].status,
            SurfaceActionStatus::Succeeded
        );
        let _ = fs::remove_dir_all(path.parent().expect("parent"));
    }

    #[test]
    fn lifecycle_is_ordered_idempotent_and_dispose_releases_attachment_state() {
        let path = temp_path("lifecycle");
        let mut store = SurfaceInstanceStore::new(&path).expect("open store");
        let record = create(&mut store);
        let instance_id = record.descriptor.instance_id.clone();
        let attachment = store
            .attach(&instance_id, "hook-node:1", "device:1", None)
            .expect("attach");
        let attachment_id = attachment.descriptor.attachment_id.clone();
        store
            .put_snapshot(&instance_id, snapshot(&record, &attachment_id))
            .expect("mount");
        let active = SurfaceLifecycleEvent {
            protocol_version: SURFACE_PROTOCOL_VERSION.to_owned(),
            instance_id: instance_id.clone(),
            attachment_id: attachment_id.clone(),
            state: SurfaceLifecycleState::Active,
            revision: 2,
        };
        assert_eq!(
            store
                .transition_lifecycle(&instance_id, active.clone())
                .expect("activate")
                .lifecycle,
            SurfaceLifecycleState::Active
        );
        store
            .transition_lifecycle(&instance_id, active)
            .expect("idempotent replay");
        assert!(store
            .transition_lifecycle(
                &instance_id,
                SurfaceLifecycleEvent {
                    protocol_version: SURFACE_PROTOCOL_VERSION.to_owned(),
                    instance_id: instance_id.clone(),
                    attachment_id: attachment_id.clone(),
                    state: SurfaceLifecycleState::Suspended,
                    revision: 4,
                },
            )
            .is_err());
        let disposed = store
            .transition_lifecycle(
                &instance_id,
                SurfaceLifecycleEvent {
                    protocol_version: SURFACE_PROTOCOL_VERSION.to_owned(),
                    instance_id: instance_id.clone(),
                    attachment_id,
                    state: SurfaceLifecycleState::Disposed,
                    revision: 3,
                },
            )
            .expect("dispose");
        assert_eq!(disposed.lifecycle, SurfaceLifecycleState::Disposed);
        assert!(disposed.snapshot.is_none());
        assert!(disposed.host_capabilities.is_none());
        let _ = fs::remove_dir_all(path.parent().expect("parent"));
    }
}
