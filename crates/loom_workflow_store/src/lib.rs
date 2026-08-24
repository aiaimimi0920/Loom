//! Workflow persistence and graph codec contracts for Loom.

mod decode;
mod encode;
mod error;
mod helpers;
mod model;
mod storage;
mod store;
mod validation;

pub use decode::{collect_workflow_uses, workflow_yaml_to_graph_json};
pub use encode::graph_json_to_workflow_yaml;
pub use error::{WorkflowStoreError, WorkflowStoreResult};
pub use helpers::workflow_file_name;
pub use model::WorkflowMetadata;
pub use store::WorkflowStore;
pub use validation::{validate_art_id, validate_workflow_uses};

pub(crate) const WORKFLOW_INDEX_FILE: &str = "workflow_index.json";
pub(crate) const LIVE_WORKFLOW_FILE: &str = "latest.yaml";
pub(crate) const STICKER_USES: &str = "__sticker__";
pub(crate) const VISUAL_META_KEYS: [&str; 7] = [
    "src",
    "previewSrc",
    "minified",
    "savedRect",
    "cropOffset",
    "opacityNormal",
    "opacityMini",
];

#[cfg(test)]
mod tests;
