use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, Instant};

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use loom_protocol::{
    validate_surface_protocol, SurfaceActionAck, SurfaceActionCancelRequest,
    SurfaceActionConcurrency, SurfaceActionDefinition, SurfaceActionInvocation,
    SurfaceActionProgress, SurfaceActionResponse, SurfaceActionStatus, SurfaceConfirmationDecision,
    SurfaceConfirmationRequest, SurfaceEvent, SurfaceExecutionError, SurfaceExecutionFailure,
    SurfaceInstanceMode, SurfacePackageManifest, SurfacePatch, SurfacePreviewCommit,
    SurfaceResultCommit, SURFACE_EVENT_ACTION_ACK, SURFACE_EVENT_ACTION_PROGRESS,
    SURFACE_EVENT_CONFIRMATION_REQUEST, SURFACE_EVENT_FAILURE, SURFACE_EVENT_PATCH,
    SURFACE_EVENT_PREVIEW, SURFACE_EVENT_RESULT, SURFACE_PROTOCOL_VERSION,
};
use loom_tool_registry::{framework::FrameworkRegistry, ToolDefinition, ToolRegistry};
use loom_workflow_runtime::execute_tool_with_workflows_timeout_and_cancellation;
use loom_workflow_store::WorkflowStore;
use serde_json::{json, Value};

use super::request_executor::{BoundedRequestExecutor, SubmitError};
use super::surface_resources::{SharedSurfaceResourceStore, SurfaceResourceStoreError};
use super::surface_store::{
    SharedSurfaceInstanceStore, SurfaceConfirmationResolution, SurfaceStoreError,
};
use super::{broadcast_hook_bridge_json, SharedHookBridgeRuntime, SharedMcpServerStore};

// Keep private implementation fragments in one lexical module. This preserves the existing
// visibility graph while keeping each responsibility independently reviewable and testable.
include!("surface_actions/model.rs");
include!("surface_actions/executor.rs");
include!("surface_actions/executor_helpers.rs");
include!("surface_actions/coordination.rs");
include!("surface_actions/job_runtime.rs");
include!("surface_actions/response.rs");
include!("surface_actions/outcomes.rs");

#[cfg(test)]
mod tests {
    include!("surface_actions/tests/fixtures.rs");
    include!("surface_actions/tests/commit_fanout.rs");
    include!("surface_actions/tests/resources_confirmation.rs");
    include!("surface_actions/tests/cancel_replace.rs");
    include!("surface_actions/tests/guard_reaper.rs");
    include!("surface_actions/tests/recovery.rs");
    include!("surface_actions/tests/cache_migration_budget.rs");
}
