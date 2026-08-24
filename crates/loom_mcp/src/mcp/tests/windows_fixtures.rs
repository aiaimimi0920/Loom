//! Windows command-wrapper fixtures.

use super::*;

#[cfg(windows)]
pub(super) fn windows_cmd_fixture_config() -> McpServerConfig {
    let temp_root = unique_test_temp_dir("fixture");
    std::fs::create_dir_all(&temp_root).expect("create MCP fixture temp dir");

    let command_base = temp_root.join("loom-mcp-fixture");
    let script_path = command_base.with_extension("cmd");
    let current_exe = std::env::current_exe().expect("current test executable");
    let script = format!(
        "@echo off\r\nset LOOM_MCP_FIXTURE_SERVER=1\r\n\"{}\" mcp::tests::fixture_server::mcp_fixture_server --exact --nocapture\r\n",
        current_exe.display()
    );
    std::fs::write(&script_path, script).expect("write MCP fixture cmd wrapper");

    McpServerConfig::new(
        "fixture-cmd",
        "Fixture MCP CMD",
        command_base.display().to_string(),
    )
}

#[cfg(windows)]
pub(super) fn unique_test_temp_dir(label: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "loom-mcp-{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("fixture timestamp")
            .as_nanos()
    ))
}
