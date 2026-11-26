//! Live Art process for Tile joint tests: fast result is 101; if a slow call is not cancelled, late result 102 must be fenced out.
use std::io::{Read, Write};
fn main() {
    let mut request = String::new();
    std::io::stdin()
        .take(1024 * 1024 + 1)
        .read_to_string(&mut request)
        .unwrap();
    assert!(request.len() <= 1024 * 1024 && request.contains("\"refresh_price\""));
    let slow = request.contains("\"slow\"");
    // TEMP is set to an isolated root by the exclusive joint fixture and inherited via the managed allowlist, never polluting locked Art package directories.
    let root = std::env::temp_dir();
    let mut calls = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(root.join("executions.txt"))
        .unwrap();
    calls.write_all(if slow { b"s" } else { b"f" }).unwrap();
    calls.sync_all().unwrap();
    if slow {
        std::thread::sleep(std::time::Duration::from_secs(2));
        std::fs::write(root.join("late.txt"), b"finished slow handler").unwrap();
    }
    let price = if slow { 102 } else { 101 };
    let response = r#"{"surfaceAction":{"protocolVersion":"loom.surface.v1","patches":[{"operations":[{"op":"set","nodeId":"price","path":"/props/text","value":"$price"}],"statePatch":{"price":$price}}],"result":{"outputs":{"price":{"kind":"value","value":$price}},"statePatch":{"price":$price}}}}"#
        .replace("$price", &price.to_string());
    std::io::stdout().write_all(response.as_bytes()).unwrap();
}
