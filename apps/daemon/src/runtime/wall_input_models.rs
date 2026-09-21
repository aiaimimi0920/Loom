// Endpoint control is volatile and independent of device-wide Live viewer sequences.
use crate::wall_store::{WallInputBinding, WallInputTarget};
use loom_protocol::wall::{TileInputCapability, TilePixelPoint};

const WALL_CONTROL_TTL_MS: u64 = 4_000;

#[derive(Clone)]
struct WallController {
    binding: WallInputBinding,
    control_id: String,
    device_id: String,
    token_hash: String,
    placement_id: String,
    session_id: String,
    pointer_id: u32,
    sequence: u64,
    deadline: Instant,
    buttons: u8,
    keys: BTreeSet<u16>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WallControlAcquire {
    binding: WallInputBinding,
    pixel: TilePixelPoint,
    pointer_id: u32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WallControlReference {
    binding: WallInputBinding,
    control_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WallInputRequest {
    control: WallControlReference,
    sequence: u64,
    event: WallInputEvent,
}

#[derive(Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
enum WallInputEvent {
    Move {
        pixel: TilePixelPoint,
        pointer_id: u32,
    },
    Button {
        pixel: TilePixelPoint,
        pointer_id: u32,
        button: loom_protocol::LiveMouseButton,
        state: loom_protocol::LiveButtonState,
        click_count: u8,
    },
    Wheel {
        pixel: TilePixelPoint,
        delta_x: i32,
        delta_y: i32,
    },
    Key {
        virtual_key: u16,
        state: loom_protocol::LiveButtonState,
    },
}

impl WallInputEvent {
    fn pixel(&self) -> Option<TilePixelPoint> {
        match self {
            Self::Move { pixel, .. } | Self::Button { pixel, .. } | Self::Wheel { pixel, .. } => {
                Some(*pixel)
            }
            Self::Key { .. } => None,
        }
    }

    fn capability(&self) -> TileInputCapability {
        match self {
            Self::Move { .. } | Self::Button { .. } => TileInputCapability::Pointer,
            Self::Wheel { .. } => TileInputCapability::Wheel,
            Self::Key { .. } => TileInputCapability::Keyboard,
        }
    }

    fn translate(
        &self,
        target: &WallInputTarget,
        owner: &mut WallController,
    ) -> std::result::Result<loom_protocol::LiveInputKind, WallStoreError> {
        use loom_protocol::*;
        let point = target.point.unwrap_or(wall::WallPoint { x: 0.0, y: 0.0 });
        let invalid = || {
            WallStoreError::new(
                400,
                "wall_input_invalid",
                "invalid input edge or pointer identity",
            )
        };
        Ok(match self {
            Self::Move { pointer_id, .. } => {
                if *pointer_id != owner.pointer_id {
                    return Err(invalid());
                }
                LiveInputKind::MouseMove(LivePointerPosition {
                    x: point.x,
                    y: point.y,
                })
            }
            Self::Button {
                pointer_id,
                button,
                state,
                click_count,
                ..
            } => {
                if *pointer_id != owner.pointer_id || !(1..=2).contains(click_count) {
                    return Err(invalid());
                }
                let mask = match button {
                    LiveMouseButton::Left => 1,
                    LiveMouseButton::Right => 2,
                    LiveMouseButton::Middle => 4,
                    _ => return Err(invalid()),
                };
                if *state == LiveButtonState::Pressed {
                    owner.buttons |= mask;
                } else {
                    if owner.buttons & mask == 0 {
                        return Err(invalid());
                    }
                    owner.buttons &= !mask;
                }
                LiveInputKind::MouseButton(LiveMouseButtonInput {
                    button: *button,
                    state: *state,
                    x: point.x,
                    y: point.y,
                    click_count: *click_count,
                })
            }
            Self::Wheel {
                delta_x, delta_y, ..
            } => {
                if (*delta_x == 0) == (*delta_y == 0)
                    || delta_x.unsigned_abs() > 1200
                    || delta_y.unsigned_abs() > 1200
                {
                    return Err(invalid());
                }
                LiveInputKind::Wheel(LiveWheelInput {
                    delta_x: *delta_x,
                    delta_y: *delta_y,
                    x: point.x,
                    y: point.y,
                })
            }
            Self::Key { virtual_key, state } => {
                if !(1..=254).contains(virtual_key) {
                    return Err(invalid());
                }
                if *state == LiveButtonState::Pressed {
                    if owner.keys.len() >= 32 && !owner.keys.contains(virtual_key) {
                        return Err(invalid());
                    }
                    owner.keys.insert(*virtual_key);
                } else if !owner.keys.remove(virtual_key) {
                    return Err(invalid());
                }
                LiveInputKind::Key(LiveKeyInput {
                    code: format!("vk:{virtual_key}"),
                    state: *state,
                    modifiers: 0,
                })
            }
        })
    }
}

fn wall_live_error(error: LiveRuntimeError) -> WallStoreError {
    WallStoreError::new(
        error.status,
        error.code,
        "Live source rejected wall control",
    )
}

fn wall_control_invalid() -> WallStoreError {
    WallStoreError::new(
        409,
        "wall_control_invalid",
        "wall control is expired, replaced or belongs to another endpoint",
    )
}
