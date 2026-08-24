//! Workflow runtime contracts grouped by execution concern.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::time::{SystemTime, UNIX_EPOCH};

use loom_tool_registry::{
    ToolDefinition, ToolExecution, ToolRegistry, WorkflowExecutionBindings, WorkflowInputBinding,
    WorkflowOutputBinding,
};
use loom_workflow_store::WorkflowStore;
use serde_json::json;

use super::*;

mod bindings;
mod child_resolution;
mod execution;
mod fixtures;
mod hardening;
mod image_path;
mod preview;

use fixtures::*;
