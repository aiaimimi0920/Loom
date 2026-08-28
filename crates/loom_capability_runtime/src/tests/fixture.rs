//! Compiled framed-protocol fixture shared by runtime lifecycle tests.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub(super) fn compile_fixture(root: &Path) -> PathBuf {
    let runtime = root.join("runtime");
    fs::create_dir_all(&runtime).expect("runtime directory");
    let source = runtime.join("fixture.rs");
    let executable = runtime.join(if cfg!(windows) {
        "fixture.exe"
    } else {
        "fixture"
    });
    fs::write(&source, FIXTURE_SOURCE).expect("fixture source");
    let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let status = Command::new(rustc)
        .arg(&source)
        .arg("-O")
        .arg("-o")
        .arg(&executable)
        .status()
        .expect("run rustc");
    assert!(status.success(), "compile fixture runtime");
    executable
}

pub(super) fn temp_root(name: &str) -> PathBuf {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "loom-capability-runtime-{name}-{}-{nonce}",
        std::process::id()
    ))
}

pub(super) fn cleanup(root: &Path) {
    let _ = fs::remove_dir_all(root);
    let _ = fs::remove_file(root.with_extension("trust.json"));
}

const FIXTURE_SOURCE: &str = r###"
use std::fs::OpenOptions;
use std::io::{Read, Write};
fn field(input: &str, key: &str) -> String {
    let marker = format!("\"{}\":\"", key);
    let rest = &input[input.find(&marker).unwrap() + marker.len()..];
    rest[..rest.find('\"').unwrap()].to_owned()
}
fn append_pid(path: &str) {
    let mut file = OpenOptions::new().create(true).append(true).open(path).unwrap();
    writeln!(file, "{}", std::process::id()).unwrap();
    file.flush().unwrap();
}
fn main() {
    let mode = std::env::args().nth(1).unwrap_or_default();
    if mode == "tree-child" || mode == "tree-leaf" {
        let pid_file = std::env::args().nth(2).unwrap();
        append_pid(&pid_file);
        if mode == "tree-child" {
            std::process::Command::new(std::env::current_exe().unwrap())
                .arg("tree-leaf")
                .arg(&pid_file)
                .spawn()
                .unwrap();
        }
        loop { std::thread::sleep(std::time::Duration::from_secs(30)); }
    }
    let mut input = std::io::stdin();
    let mut output = std::io::stdout();
    loop {
        let mut length = [0u8; 4];
        if input.read_exact(&mut length).is_err() { break; }
        let mut bytes = vec![0u8; u32::from_be_bytes(length) as usize];
        input.read_exact(&mut bytes).unwrap();
        let request = String::from_utf8(bytes).unwrap();
        let id = field(&request, "requestId");
        let method = field(&request, "method");
        if (mode == "tree" || mode == "crash-tree") && method == "command" {
            let pid_file = std::env::args().nth(2).unwrap();
            std::process::Command::new(std::env::current_exe().unwrap())
                .arg("tree-child")
                .arg(&pid_file)
                .spawn()
                .unwrap();
            for _ in 0..100 {
                let ready = std::fs::read_to_string(&pid_file)
                    .map(|value| value.lines().count() >= 2)
                    .unwrap_or(false);
                if ready { break; }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            if mode == "crash-tree" { std::process::exit(19); }
            std::thread::sleep(std::time::Duration::from_secs(30));
        }
        if mode == "hang" && method == "command" {
            std::thread::sleep(std::time::Duration::from_secs(30));
        }
        let (status, payload, error) = if mode == "fail-secret" && method == "command" {
            ("failed", "null", r#","error":{"code":"runtime_fault","message":"token=fixture-secret","retryable":false}"#)
        } else if mode == "extra" && method == "initialize" {
            ("succeeded", r#"{"contributions":{"commands":[{"id":"publisher.example/fixture.extra","title":"Extra"}]}}"#, "")
        } else if mode == "mutate" && method == "initialize" {
            ("succeeded", r#"{"contributions":{"commands":[{"id":"publisher.example/fixture.run","title":"Mutated"}]}}"#, "")
        } else if mode == "notice" {
            ("succeeded", r#"{"output":{"ok":true},"effects":[{"type":"notice.show","payload":{"message":"ready"}}]}"#, "")
        } else if mode == "clipboard" {
            ("succeeded", r#"{"output":{"ok":true},"effects":[{"type":"clipboard.writeText","payload":{"text":"ready"}}]}"#, "")
        } else if mode == "bad-output" {
            ("succeeded", r#"{"output":{"ok":"wrong"}}"#, "")
        } else { ("succeeded", r#"{"ok":true}"#, "") };
        let response = format!(
            "{{\"type\":\"response\",\"protocol\":\"loom.capability.runtime.v1\",\"apiVersion\":\"1.0\",\"requestId\":\"{}\",\"status\":\"{}\",\"payload\":{}{} }}",
            id, status, payload, error
        );
        output.write_all(&(response.len() as u32).to_be_bytes()).unwrap();
        output.write_all(response.as_bytes()).unwrap();
        output.flush().unwrap();
        if method == "deactivate" { break; }
    }
}
"###;
