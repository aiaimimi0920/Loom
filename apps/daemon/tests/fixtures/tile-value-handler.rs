//! 联合专属真实Art：独立解析受限JSON调用，记录实际payload并返回明确的节点/正式结果。
use serde_json::{json, Value};
use std::io::{Read, Write};
fn invocation(v: &Value) -> Option<&Value> {
    if v.get("actionId") == Some(&json!("refresh_price"))
        && v.get("eventClass") == Some(&json!("commit"))
    {
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
    let input = invocation(&request).expect("actual commit invocation");
    let payload = &input["payload"];
    let (node, value, price) = if payload == &json!({"value":true}) {
        ("flag", json!(true), 101)
    } else {
        assert_eq!(payload, &json!({"value":"b"}));
        ("choice", json!("b"), 102)
    };
    let root = std::env::temp_dir();
    let mut calls = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(root.join("executions.txt"))
        .unwrap();
    writeln!(calls, "{}", payload).unwrap();
    calls.sync_all().unwrap();
    let response = json!({"surfaceAction":{"protocolVersion":"loom.surface.v1","patches":[{"operations":[
        {"op":"set","nodeId":node,"path":"/props/value","value":value},
        {"op":"set","nodeId":"price","path":"/props/text","value":price.to_string()}],"statePatch":{"price":price}}],
        "result":{"outputs":{"price":{"kind":"value","value":price}},"statePatch":{"price":price}}}});
    std::io::stdout()
        .write_all(&serde_json::to_vec(&response).unwrap())
        .unwrap();
}
