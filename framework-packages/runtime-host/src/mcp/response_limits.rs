// Allocation-free encoded-size checks and depth limits for untrusted MCP response values.
const MAX_MCP_RESPONSE_VALUE_BYTES: usize = 8 * 1024 * 1024;
const MAX_MCP_RESPONSE_VALUE_DEPTH: usize = 64;

struct JsonSizeWriter {
    written: usize,
    limit: usize,
    exceeded: bool,
}

impl std::io::Write for JsonSizeWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > self.limit.saturating_sub(self.written) {
            self.exceeded = true;
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "JSON value exceeds configured byte limit",
            ));
        }
        self.written += bytes.len();
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn validate_mcp_response_value(value: &Value, label: &str) -> Result<(), String> {
    validate_json_value_limits(
        value,
        label,
        MAX_MCP_RESPONSE_VALUE_BYTES,
        MAX_MCP_RESPONSE_VALUE_DEPTH,
    )
}

fn validate_json_value_limits(
    value: &Value,
    label: &str,
    max_bytes: usize,
    max_depth: usize,
) -> Result<(), String> {
    if !value_is_within_depth(value, 0, max_depth) {
        return Err(format!(
            "{label} exceeds the {max_depth}-level nesting limit"
        ));
    }
    let mut writer = JsonSizeWriter {
        written: 0,
        limit: max_bytes,
        exceeded: false,
    };
    if let Err(error) = serde_json::to_writer(&mut writer, value) {
        if writer.exceeded {
            return Err(format!("{label} exceeds the {max_bytes} byte limit"));
        }
        return Err(format!("cannot encode {label}: {error}"));
    }
    Ok(())
}
