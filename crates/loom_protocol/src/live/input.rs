use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LiveMouseButton {
    Left,
    Middle,
    Right,
    X1,
    X2,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LiveButtonState {
    Pressed,
    Released,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum LiveInputKind {
    MouseMove(LivePointerPosition),
    MouseButton(LiveMouseButtonInput),
    Wheel(LiveWheelInput),
    Key(LiveKeyInput),
    Text(LiveTextInput),
    Focus(LiveFocusInput),
    Cancel,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LivePointerPosition {
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LiveMouseButtonInput {
    pub button: LiveMouseButton,
    pub state: LiveButtonState,
    pub x: f64,
    pub y: f64,
    pub click_count: u8,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LiveWheelInput {
    pub delta_x: i32,
    pub delta_y: i32,
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LiveKeyInput {
    pub code: String,
    pub state: LiveButtonState,
    pub modifiers: u8,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LiveTextInput {
    pub text: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LiveFocusInput {
    pub focused: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LiveInputEvent {
    pub input_sequence: u64,
    pub issued_at_ms: u64,
    pub source_device_id: String,
    pub kind: LiveInputKind,
}

impl LiveInputEvent {
    pub fn may_coalesce(&self) -> bool {
        matches!(self.kind, LiveInputKind::MouseMove(_))
    }
}
