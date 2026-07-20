use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UiFieldKind {
    Boolean,
    Select,
    Text,
    Number,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiFieldOption {
    pub value: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiField {
    pub path: String,
    pub label: String,
    pub kind: UiFieldKind,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<UiFieldOption>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiSection {
    pub title: String,
    pub fields: Vec<UiField>,
}
