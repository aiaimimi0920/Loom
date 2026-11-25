//! 只投影 resolver 验证过的不可变 manifest；当前目录 capability 不能替代包证明。
use super::*;
use loom_protocol::local_art::package::ports::{Port, Ports, MAX_PORTS};

fn project(values: &[Value]) -> LocalResult<Vec<Port>> {
    if values.len() > MAX_PORTS {
        return Err(fail(409, "local_art_ports_invalid"));
    }
    values
        .iter()
        .map(|value| {
            let name = value
                .get("name")
                .and_then(Value::as_str)
                .ok_or_else(|| fail(409, "local_art_ports_invalid"))?;
            let kinds: Vec<_> = ["type", "data_type", "execution_type"]
                .iter()
                .filter_map(|key| value.get(key).and_then(Value::as_str))
                .collect();
            let image = kinds
                .iter()
                .any(|kind| kind.to_ascii_lowercase().contains("image"))
                || value.get("widget").and_then(Value::as_str) == Some("image_link");
            let kind = if image {
                "image"
            } else {
                kinds.first().copied().unwrap_or("any")
            };
            Ok(Port {
                name: name.into(),
                kind: kind.into(),
            })
        })
        .collect()
}
pub(super) fn resolve(tool: &loom_tool_registry::ToolDefinition) -> LocalResult<Ports> {
    let ports = Ports {
        inputs: project(&tool.inputs)?,
        outputs: project(&tool.outputs)?,
    };
    ports
        .validate()
        .map_err(|_| fail(409, "local_art_ports_invalid"))?;
    Ok(ports)
}
pub(super) fn visibility(
    tool: &loom_tool_registry::ToolDefinition,
) -> LocalResult<loom_protocol::local_art::package::visibility::Visibility> {
    // 当前包只读取单端口 bool；旧能力级映射不参与签名默认值。
    let project = |values: &[Value]| {
        values
            .iter()
            .map(|value| {
                value
                    .get("defaultVisible")
                    .and_then(Value::as_bool)
                    .unwrap_or(true)
            })
            .collect()
    };
    let visibility = loom_protocol::local_art::package::visibility::Visibility {
        inputs: project(&tool.inputs),
        outputs: project(&tool.outputs),
    };
    visibility
        .validate(&resolve(tool)?)
        .map_err(|_| fail(409, "local_art_ports_invalid"))?;
    Ok(visibility)
}
