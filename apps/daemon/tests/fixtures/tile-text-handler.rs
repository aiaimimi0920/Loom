//! 独立真实Art进程；只记录实际输入并产生声明式patch和正式结果，不调用Otter实现。
use serde_json::{json, Value};
use std::io::{Read, Write};
fn invocation(v: &Value) -> Option<&Value> {
    if v["actionId"] == "refresh_price" && v["payload"].is_object() {
        return Some(v);
    }
    match v {
        Value::Object(o) => o.values().find_map(invocation),
        Value::Array(a) => a.iter().find_map(invocation),
        _ => None,
    }
}
fn main() {
    let mut bytes = Vec::new();
    std::io::stdin()
        .take(1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .unwrap();
    assert!(bytes.len() <= 1024 * 1024);
    let request: Value = serde_json::from_slice(&bytes).unwrap();
    let input = invocation(&request).unwrap();
    let mut log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(std::env::temp_dir().join("text-executions.txt"))
        .unwrap();
    writeln!(
        log,
        "{}",
        json!({"class":input["eventClass"],"payload":input["payload"]})
    )
    .unwrap();
    log.sync_all().unwrap();
    let value = &input["payload"]["value"];
    let (operations, patch, result) = if !value.is_null() {
        let text = value
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| value.as_f64().unwrap().to_string());
        (
            json!([{"op":"set","nodeId":"field","path":"/props/value","value":text}]),
            json!({"value":value}),
            json!({"value":{"kind":"value","value":value}}),
        )
    } else {
        let text = input["payload"]["surfaceValues"]["field"]
            .as_str()
            .expect("actual current form value");
        (
            json!([{"op":"set","nodeId":"label","path":"/props/text","value":"submitted"}]),
            json!({"form":text}),
            json!({"form":{"kind":"value","value":text}}),
        )
    };
    let response = json!({"surfaceAction":{"protocolVersion":"loom.surface.v1","patches":[{"operations":operations,"statePatch":patch}],"result":{"outputs":result,"statePatch":patch}}});
    std::io::stdout()
        .write_all(&serde_json::to_vec(&response).unwrap())
        .unwrap();
}
