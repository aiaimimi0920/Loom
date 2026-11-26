//! Live Art process for the isolated joint gate; reads bounded stdin, records a single call in the package directory, and returns a fixed business result.
use std::io::{Read, Write};
fn main() {
    let mut request = String::new();
    std::io::stdin()
        .take(1024 * 1024 + 1)
        .read_to_string(&mut request)
        .unwrap();
    assert!(request.len() <= 1024 * 1024);
    assert!(request.contains("\"refresh_price\""));
    let mut calls = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("executions.txt")
        .unwrap();
    calls.write_all(b"1").unwrap();
    calls.sync_all().unwrap();
    std::io::stdout()
        .write_all(
            br#"{
        "surfaceAction": {
            "protocolVersion": "loom.surface.v1",
            "patches": [{
                "operations": [{"op":"set","nodeId":"price","path":"/props/text","value":"101"}],
                "statePatch": {"price":101}
            }],
            "result": {
                "outputs": {"price":{"kind":"value","value":101}},
                "statePatch": {"price":101}
            }
        }
    }"#,
        )
        .unwrap();
}
