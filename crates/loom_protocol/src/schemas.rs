//! Embedded JSON schemas shipped as part of the public protocol contract.

pub const FRAMEWORK_MANIFEST_V1: &str =
    include_str!("../../../protocol/schemas/framework-manifest.v1.schema.json");
pub const FRAMEWORK_EXECUTE_REQUEST_V1: &str =
    include_str!("../../../protocol/schemas/framework-execute-request.v1.schema.json");
pub const FRAMEWORK_EXECUTE_RESPONSE_V1: &str =
    include_str!("../../../protocol/schemas/framework-execute-response.v1.schema.json");
pub const FRAMEWORK_AUTHORING_V1: &str =
    include_str!("../../../protocol/schemas/framework-authoring.v1.schema.json");
pub const ART_RUNTIME_V1: &str =
    include_str!("../../../protocol/schemas/art-runtime.v1.schema.json");
pub const SURFACE_MANIFEST_V1: &str =
    include_str!("../../../protocol/schemas/surface-manifest.v1.schema.json");
pub const SURFACE_MESSAGE_V1: &str =
    include_str!("../../../protocol/schemas/surface-message.v1.schema.json");
pub const SURFACE_SCENE_V1: &str =
    include_str!("../../../protocol/schemas/surface-scene.v1.schema.json");
pub const SURFACE_STREAM_V1: &str =
    include_str!("../../../protocol/schemas/surface-stream.v1.schema.json");
pub const DEVICE_SESSION_V1: &str =
    include_str!("../../../protocol/schemas/device-session.v1.schema.json");
pub const HOOK_MESSAGE_V1: &str =
    include_str!("../../../protocol/schemas/hook-message.v1.schema.json");
