use std::collections::hash_map::DefaultHasher;
use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::{ErrorKind, Read, Write};
use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use base64::engine::general_purpose::{STANDARD as BASE64, URL_SAFE_NO_PAD as BASE64_URL};
use base64::Engine as _;
use ed25519_dalek::{Signature, Verifier as _, VerifyingKey};
use fs2::FileExt;
#[cfg(not(test))]
use loom_configuration::default_configuration_root;
use loom_configuration::{
    built_in_registry, render_app_settings_page, render_settings_index, ConfigRegistry,
    FileDocumentStore, ManagedAppId, ManagedAppSet, ManagedConfigError, ManagedConfigErrorCode,
};
use loom_durable::{
    InMemoryRunEvidenceStore, RunEventDraft, RunEvidenceStore, RunStoreError, RunStoreStatus,
    SqliteRunEvidenceStore,
};
use loom_hook_bridge::{
    capabilities_updated_event, instantiate_workflow, sync_workflow, update_workflow_node,
    HOOK_BRIDGE_PORT,
};
use loom_hooks::{HookSettings, HookSettingsSummary};
use loom_mcp::package::{
    install_server_package, uninstall_server_package, McpPackageError, MAX_MCP_SERVER_PACKAGE_BYTES,
};
use loom_mcp::{McpClient, McpServerConfig, McpTransport};
use loom_plugin_security::{generate_signing_key, sign_message, SigningKeyDocument, TrustPolicy};
use loom_protocol::{
    device_session_signature_message, is_safe_package_id, is_safe_publisher_id, ArtRuntimeManifest,
    DeviceSessionChallengeRequest, DeviceSessionChallengeResponse, DeviceSessionIssueRequest,
    DeviceSessionIssueResponse, HookArtAck, HookArtCancelRequest, HookArtCapability,
    HookArtExecuteRequest, HookArtFailure, HookArtPortValue, HookArtPreviewCommit, HookArtProgress,
    HookArtResourcesReleaseRequest, HookArtResultCommit, HookCapabilities, HookEvent,
    HookHandshakeResponse, HookRequest, HookRequestStatus, HookResponse, HookTransportMode,
    PublisherTrustRecord, SurfaceActionCancelRequest, SurfaceConfirmationDecision, SurfaceEvent,
    SurfaceExecutionFailure, SurfaceHostCapabilities, SurfaceInstanceMode,
    SurfaceInstancePersistence, SurfaceLifecycleEvent, SurfaceNode, SurfacePatch, SurfacePortValue,
    SurfacePreviewCommit, SurfaceResourceDescriptor, SurfaceResourceKind, SurfaceResourceTransport,
    SurfaceResourceTransportKind, SurfaceResultCommit, SurfaceRuntimeKind, SurfaceSnapshot,
    DEVICE_SESSION_PROTOCOL_VERSION, HOOK_EVENT_CACHE_CONTROL, HOOK_EVENT_SETTINGS_UPDATED,
    SURFACE_EVENT_CONFIRMATION_REQUEST, SURFACE_EVENT_DISPOSE, SURFACE_EVENT_GENERATION,
    SURFACE_EVENT_LIFECYCLE, SURFACE_EVENT_PATCH, SURFACE_EVENT_SNAPSHOT,
};
use loom_shared_image::{SharedImageError, SharedImageFormat, SharedImageInfo, SharedImageStore};
use loom_tool_registry::art_settings::{
    apply_settings_metadata, art_is_locally_authored, art_parameter_definitions,
    credential_value_type_matches_parameter, validate_parameter_value, ArtParameterDefinition,
    ArtSettingsStore, ArtUpdateSource, ArtUserSettings,
};
use loom_tool_registry::credentials::{
    CredentialInput, CredentialScope, CredentialStore, CredentialValueType,
};
use loom_tool_registry::network_policy::{
    get_bounded, secure_client, validate_outbound_url, OutboundPolicy,
};
#[cfg(test)]
use loom_tool_registry::WorkflowExecutionBindings;
use loom_tool_registry::{
    framework::{read_dependencies, FrameworkRegistry},
    ToolDefinition, ToolExecution, ToolRegistry, ToolRegistryError,
};
use loom_workflow_runtime::{
    execute_tool_with_workflows, execute_tool_with_workflows_and_preview_timeout_and_cancellation,
    workflow_node_tool_ids, WorkflowRuntimeError,
};
use loom_workflow_store::{WorkflowStore, WorkflowStoreError};
use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest as _, Sha256};
use uuid::Uuid;

mod brain_plan;
mod hook_canvas;
mod http_request;
mod request_executor;
mod surface_actions;
mod surface_resources;
mod surface_store;

use brain_plan::{
    build_brain_planner, BrainPlanRequest, BrainPlannerConfig, BrainPlannerStatus,
    SharedBrainPlanner,
};
use http_request::*;
use request_executor::{
    BoundedRequestExecutor, RequestExecutorConfig, RequestExecutorStatus, SubmitError,
};
use surface_actions::{SharedSurfaceActionExecutor, SurfaceActionExecutor};
use surface_resources::{
    SharedSurfaceResourceStore, SurfaceResourceGcOutcome, SurfaceResourceStore,
    SurfaceResourceStoreError, DEFAULT_RESOURCE_GC_MIN_AGE_MILLIS, MAX_SURFACE_RESOURCE_BYTES,
    MIN_RESOURCE_GC_AGE_MILLIS,
};
use surface_store::{
    SharedSurfaceInstanceStore, SurfaceInstanceRecord, SurfaceInstanceStore, SurfaceStoreError,
};

// Responsibility-focused daemon implementation slices share the crate root to preserve the public API.
include!("runtime/daemon_config.rs");
include!("runtime/daemon_lifecycle.rs");
include!("runtime/connection_dispatch.rs");
include!("runtime/http_routing.rs");
include!("runtime/secure_persistence.rs");
include!("runtime/mcp_persistence_models.rs");
include!("runtime/settings_ocr_runtime.rs");
include!("runtime/hook_bridge_state.rs");
include!("runtime/device_registry_store.rs");
include!("runtime/device_auth.rs");
include!("runtime/route_dispatch.rs");
include!("runtime/route_dispatch/surfaces_devices.rs");
include!("runtime/route_dispatch/mcp_art.rs");
include!("runtime/route_dispatch/framework_tools.rs");
include!("runtime/route_dispatch/settings_runs.rs");
include!("runtime/device_handlers.rs");
include!("runtime/surface_request_models.rs");
include!("runtime/surface_resource_leases.rs");
include!("runtime/surface_instance_routes.rs");
include!("runtime/surface_lifecycle.rs");
include!("runtime/surface_commit_events.rs");
include!("runtime/surface_javascript_source.rs");
include!("runtime/surface_remount_state.rs");
include!("runtime/surface_instance_mounting.rs");
include!("runtime/surface_host_routes.rs");
include!("runtime/settings_mcp_servers.rs");
include!("runtime/mcp_server_lifecycle.rs");
include!("runtime/mcp_tool_execution.rs");
include!("runtime/art_lifecycle.rs");
include!("runtime/art_management.rs");
include!("runtime/art_updates_publisher.rs");
include!("runtime/publisher_store.rs");
include!("runtime/art_store_install_publish.rs");
include!("runtime/framework_diagnostics_trust.rs");
include!("runtime/publisher_framework_tools.rs");
include!("runtime/tool_capability_helpers.rs");
include!("runtime/python_art_io.rs");
include!("runtime/python_art_discovery.rs");
include!("runtime/tool_execution.rs");
include!("runtime/shared_image_routes.rs");
include!("runtime/settings_workflow_routes.rs");
include!("runtime/canvas_workflow_persistence.rs");
include!("runtime/canvas_bridge_routes.rs");
include!("runtime/hook_canvas_preview_session.rs");
include!("runtime/hook_art_request_lifecycle.rs");
include!("runtime/hook_canvas_live_persistence.rs");
include!("runtime/hook_bridge_websocket.rs");
include!("runtime/hook_protocol_dispatch.rs");
include!("runtime/hook_art_execution.rs");
include!("runtime/hook_art_results_broadcast.rs");
include!("runtime/surface_recovery_errors.rs");
include!("runtime/capability_invocation.rs");
include!("runtime/run_http_responses.rs");

#[cfg(test)]
mod tests {
    include!("tests/suite/part_01.rs");
    include!("tests/suite/part_02.rs");
    include!("tests/suite/part_03.rs");
    include!("tests/suite/part_04.rs");
    include!("tests/suite/part_05.rs");
    include!("tests/suite/part_06.rs");
    include!("tests/suite/part_07.rs");
    include!("tests/suite/part_08.rs");
    include!("tests/suite/part_09.rs");
    include!("tests/suite/part_10.rs");
    include!("tests/suite/part_11.rs");
    include!("tests/suite/part_12.rs");
    include!("tests/suite/part_13.rs");
    include!("tests/suite/part_14.rs");
    include!("tests/suite/part_15.rs");
    include!("tests/suite/part_16.rs");
    include!("tests/suite/part_17.rs");
    include!("tests/suite/part_18.rs");
    include!("tests/suite/part_19.rs");
    include!("tests/suite/part_20.rs");
    include!("tests/suite/part_21.rs");
    include!("tests/suite/part_22.rs");
    include!("tests/suite/part_23.rs");
    include!("tests/suite/part_24.rs");
    include!("tests/suite/part_25.rs");
    include!("tests/suite/part_26.rs");
    include!("tests/suite/part_27.rs");
    include!("tests/suite/part_28.rs");
    include!("tests/suite/part_29.rs");
    include!("tests/suite/part_30.rs");
    include!("tests/suite/part_31.rs");
}
