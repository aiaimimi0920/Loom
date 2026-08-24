use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowMetadata {
    pub id: String,
    pub name: String,
    pub node_count: usize,
    pub updated_at: String,
}
