//! Dedicated live Art fixture; independently logs actual event class and value and publishes declarative, authoritative, and formal results.
use serde_json::{json, Value};
use std::io::{Read, Write};
fn invocation(v: &Value) -> Option<&Value> {
    if v["actionId"] == "refresh_price"
        && matches!(v["eventClass"].as_str(), Some("continuous" | "commit"))
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
    let input = invocation(&request).expect("actual slider invocation");
    let value = input["payload"]["value"].as_f64().unwrap();
    assert!((0.0..=100.0).contains(&value) && value % 10.0 == 0.0);
    assert_eq!(input["payload"].as_object().unwrap().len(), 1);
    let mut calls = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(std::env::temp_dir().join("slider-executions.txt"))
        .unwrap();
    writeln!(
        calls,
        "{}",
        json!({"class":input["eventClass"],"payload":input["payload"]})
    )
    .unwrap();
    calls.sync_all().unwrap();
    let response = json!({"surfaceAction":{"protocolVersion":"loom.surface.v1","patches":[{"operations":[
        {"op":"set","nodeId":"slider","path":"/props/value","value":value},
        {"op":"set","nodeId":"price","path":"/props/text","value":format!("{value:.0}")}],"statePatch":{"price":value}}],
        "result":{"outputs":{"price":{"kind":"value","value":value}},"statePatch":{"price":value}}}});
    std::io::stdout()
        .write_all(&serde_json::to_vec(&response).unwrap())
        .unwrap();
}
