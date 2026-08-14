use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use loom_mcp::{McpClient, McpServerConfig, McpTransport};
use loom_protocol::{CredentialGrant, FrameworkExecuteRequest};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Debug, Deserialize)]
struct ArtManifest {
    #[serde(default)]
    metadata: ArtMetadata,
}

#[derive(Debug, Default, Deserialize)]
struct ArtMetadata {
    #[serde(default)]
    mcp: Option<McpArtConfig>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct McpArtConfig {
    #[serde(default)]
    server_id: String,
    #[serde(default)]
    command: String,
    #[serde(default)]
    args: Vec<String>,
    tool_name: String,
    #[serde(default)]
    env: BTreeMap<String, String>,
    #[serde(default)]
    credential_env: BTreeMap<String, String>,
    transport: McpTransport,
    #[serde(default)]
    url: String,
    #[serde(default)]
    headers: BTreeMap<String, String>,
    #[serde(default)]
    credential_headers: BTreeMap<String, String>,
    #[serde(default)]
    arguments: Value,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpExecution {
    server_id: String,
    tool_name: String,
    result: Value,
}

pub fn execute(request: &FrameworkExecuteRequest, art_dir: &Path) -> Result<McpExecution, String> {
    let config = load_config(art_dir)?;
    let arguments = build_arguments(request, &config.arguments)?;
    let environment = build_environment(request, &config)?;
    let headers = build_headers(request, &config)?;
    let server_id = non_empty(&config.server_id).unwrap_or(request.art_id.as_str());
    let mut server = match config.transport {
        McpTransport::Stdio => {
            let command = super::resolve_command(art_dir, config.command.trim())?;
            McpServerConfig::new(
                server_id,
                format!("{} MCP server", request.art_id),
                command.to_string_lossy(),
            )
        }
        McpTransport::StreamableHttp => McpServerConfig::remote(
            server_id,
            format!("{} MCP server", request.art_id),
            expand_runtime_paths(&config.url, request, art_dir),
        ),
    };
    server.args = config
        .args
        .iter()
        .map(|argument| expand_runtime_paths(argument, request, art_dir))
        .collect();
    server.env = environment;
    server.headers = headers;

    let (_, result) = execute_tool(&server, &config.tool_name, &arguments)
        .map_err(|error| redact_credentials(error, &request.context.credentials))?;
    Ok(McpExecution {
        server_id: server_id.to_owned(),
        tool_name: config.tool_name,
        result,
    })
}

fn load_config(art_dir: &Path) -> Result<McpArtConfig, String> {
    let manifest_path = art_dir.join("manifest.json");
    let manifest: ArtManifest = serde_json::from_slice(
        &fs::read(&manifest_path)
            .map_err(|error| format!("cannot read {}: {error}", manifest_path.display()))?,
    )
    .map_err(|error| format!("invalid {}: {error}", manifest_path.display()))?;
    let config = manifest
        .metadata
        .mcp
        .ok_or_else(|| "MCP Art metadata.mcp is required".to_owned())?;
    match config.transport {
        McpTransport::Stdio if config.command.trim().is_empty() => {
            return Err("MCP Art metadata.mcp.command is required".to_owned())
        }
        McpTransport::StreamableHttp if config.url.trim().is_empty() => {
            return Err("MCP Art metadata.mcp.url is required".to_owned())
        }
        _ => {}
    }
    if config.tool_name.trim().is_empty() {
        return Err("MCP Art metadata.mcp.toolName is required".to_owned());
    }
    Ok(config)
}

fn build_environment(
    request: &FrameworkExecuteRequest,
    config: &McpArtConfig,
) -> Result<BTreeMap<String, String>, String> {
    let mut environment = config
        .env
        .iter()
        .map(|(name, value)| {
            validate_environment_name(name)?;
            Ok((
                name.clone(),
                expand_runtime_paths(value, request, &request.art_dir),
            ))
        })
        .collect::<Result<BTreeMap<_, _>, String>>()?;

    for (environment_name, credential_name) in &config.credential_env {
        validate_environment_name(environment_name)?;
        let credential_name = credential_name.trim();
        if credential_name.is_empty() {
            return Err(format!(
                "MCP credential mapping for `{environment_name}` is empty"
            ));
        }
        let credential = request
            .context
            .credentials
            .iter()
            .find(|credential| credential.name == credential_name)
            .ok_or_else(|| {
                format!(
                    "MCP Art requires credential `{credential_name}` for `{environment_name}`; available aliases: {}",
                    available_credential_aliases(request)
                )
            })?;
        environment.insert(environment_name.clone(), credential.value.clone());
    }
    Ok(environment)
}

fn build_headers(
    request: &FrameworkExecuteRequest,
    config: &McpArtConfig,
) -> Result<BTreeMap<String, String>, String> {
    let mut headers = config
        .headers
        .iter()
        .map(|(name, value)| {
            validate_header_name(name)?;
            Ok((
                name.clone(),
                expand_runtime_paths(value, request, &request.art_dir),
            ))
        })
        .collect::<Result<BTreeMap<_, _>, String>>()?;
    for (header_name, credential_name) in &config.credential_headers {
        validate_header_name(header_name)?;
        let credential_name = credential_name.trim();
        if credential_name.is_empty() {
            return Err(format!(
                "MCP credential mapping for header `{header_name}` is empty"
            ));
        }
        let credential = request
            .context
            .credentials
            .iter()
            .find(|credential| credential.name == credential_name)
            .ok_or_else(|| {
                format!(
                    "MCP Art requires credential `{credential_name}` for `{header_name}`; available aliases: {}",
                    available_credential_aliases(request)
                )
            })?;
        headers.insert(header_name.clone(), credential.value.clone());
    }
    Ok(headers)
}

fn available_credential_aliases(request: &FrameworkExecuteRequest) -> String {
    let aliases = request
        .context
        .credentials
        .iter()
        .map(|credential| credential.name.as_str())
        .collect::<Vec<_>>();
    if aliases.is_empty() {
        "<none>".to_owned()
    } else {
        aliases.join(", ")
    }
}

fn validate_header_name(name: &str) -> Result<(), String> {
    let valid = !name.trim().is_empty()
        && name.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(
                    byte,
                    b'!' | b'#'
                        | b'$'
                        | b'%'
                        | b'&'
                        | b'\''
                        | b'*'
                        | b'+'
                        | b'-'
                        | b'.'
                        | b'^'
                        | b'_'
                        | b'`'
                        | b'|'
                        | b'~'
                )
        });
    valid
        .then_some(())
        .ok_or_else(|| format!("invalid MCP HTTP header name `{name}`"))
}

fn validate_environment_name(name: &str) -> Result<(), String> {
    let mut characters = name.chars();
    let valid = characters
        .next()
        .is_some_and(|character| character == '_' || character.is_ascii_alphabetic())
        && characters.all(|character| character == '_' || character.is_ascii_alphanumeric());
    if valid {
        Ok(())
    } else {
        Err(format!("invalid MCP environment variable name `{name}`"))
    }
}

fn expand_runtime_paths(value: &str, request: &FrameworkExecuteRequest, art_dir: &Path) -> String {
    value
        .replace("{artDir}", &art_dir.to_string_lossy())
        .replace("{cacheDir}", &request.context.cache_dir.to_string_lossy())
        .replace("{tempDir}", &request.context.temp_dir.to_string_lossy())
}

fn build_arguments(request: &FrameworkExecuteRequest, configured: &Value) -> Result<Value, String> {
    let mut arguments = Map::new();
    merge_argument_object(&mut arguments, configured, "metadata.mcp.arguments")?;
    merge_argument_object(&mut arguments, &request.inputs, "inputs")?;
    merge_argument_object(&mut arguments, &request.params, "params")?;
    for name in &request.disabled_params {
        arguments.remove(name);
    }
    Ok(Value::Object(arguments))
}

fn merge_argument_object(
    target: &mut Map<String, Value>,
    source: &Value,
    label: &str,
) -> Result<(), String> {
    if source.is_null() {
        return Ok(());
    }
    let source = source
        .as_object()
        .ok_or_else(|| format!("MCP Art {label} must be a JSON object"))?;
    target.extend(
        source
            .iter()
            .map(|(name, value)| (name.clone(), value.clone())),
    );
    Ok(())
}

fn execute_tool(
    server: &McpServerConfig,
    tool_name: &str,
    arguments: &Value,
) -> Result<(Value, Value), String> {
    let mut client = McpClient::connect(server)
        .map_err(|error| format!("failed to connect MCP server: {error}"))?;
    client
        .initialize()
        .map_err(|error| format!("MCP initialize failed: {error}"))?;
    let tools = client
        .list_tools()
        .map_err(|error| format!("MCP tools/list failed: {error}"))?;
    let schema = find_tool_input_schema(&tools, tool_name)
        .ok_or_else(|| format!("MCP server does not expose tool `{tool_name}`"))?;
    let normalized_arguments = normalize_arguments(arguments, schema);
    let result = client
        .call_tool(tool_name, normalized_arguments.clone())
        .map_err(|error| format!("MCP tools/call `{tool_name}` failed: {error}"));
    client.cancel();
    result.map(|result| (normalized_arguments, result))
}

fn find_tool_input_schema<'a>(tools: &'a Value, tool_name: &str) -> Option<&'a Value> {
    tools
        .get("tools")?
        .as_array()?
        .iter()
        .find(|tool| tool.get("name").and_then(Value::as_str) == Some(tool_name))
        .and_then(|tool| tool.get("inputSchema"))
}

fn normalize_arguments(arguments: &Value, schema: &Value) -> Value {
    let Some(arguments) = arguments.as_object() else {
        return arguments.clone();
    };
    let properties = schema.get("properties").and_then(Value::as_object);
    let has_pattern_or_composed_properties =
        ["patternProperties", "allOf", "anyOf", "oneOf", "$ref"]
            .iter()
            .any(|name| schema.get(*name).is_some());
    let rejects_undeclared = schema.get("additionalProperties") == Some(&Value::Bool(false))
        && !has_pattern_or_composed_properties;
    Value::Object(
        arguments
            .iter()
            .filter(|(name, _)| {
                !rejects_undeclared
                    || properties.is_some_and(|properties| properties.contains_key(*name))
            })
            .map(|(name, value)| {
                let schema = properties.and_then(|properties| properties.get(name));
                (name.clone(), normalize_argument(name, value, schema))
            })
            .collect(),
    )
}

fn normalize_argument(name: &str, value: &Value, schema: Option<&Value>) -> Value {
    if name.eq_ignore_ascii_case("search_lang") {
        if let Some(raw) = value
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            let mapped = match raw.to_ascii_lowercase().as_str() {
                "zh" | "zh-cn" => "zh-hans",
                "zh-tw" => "zh-hant",
                _ => raw,
            };
            if let Some(canonical) = canonical_enum_value(schema, mapped) {
                return Value::String(canonical);
            }
            if mapped != raw {
                return Value::String(mapped.to_owned());
            }
        }
    }
    let Some(schema) = schema else {
        return value.clone();
    };
    if schema_type_matches(schema, "integer") {
        if let Some(parsed) = value
            .as_i64()
            .or_else(|| value.as_str().and_then(|value| value.trim().parse().ok()))
        {
            return Value::from(parsed);
        }
    }
    if schema_type_matches(schema, "number") {
        if let Some(parsed) = value
            .as_f64()
            .or_else(|| value.as_str().and_then(|value| value.trim().parse().ok()))
        {
            return Value::from(parsed);
        }
    }
    if schema_type_matches(schema, "boolean") {
        if let Some(parsed) = value.as_bool().or_else(|| {
            value
                .as_str()
                .and_then(|value| match value.trim().to_ascii_lowercase().as_str() {
                    "1" | "true" | "yes" | "on" => Some(true),
                    "0" | "false" | "no" | "off" => Some(false),
                    _ => None,
                })
        }) {
            return Value::from(parsed);
        }
    }
    if let Some(raw) = value.as_str() {
        if let Some(canonical) = canonical_enum_value(Some(schema), raw) {
            return Value::String(canonical);
        }
    }
    value.clone()
}

fn canonical_enum_value(schema: Option<&Value>, expected: &str) -> Option<String> {
    schema?
        .get("enum")?
        .as_array()?
        .iter()
        .filter_map(Value::as_str)
        .find(|candidate| candidate.eq_ignore_ascii_case(expected))
        .map(str::to_owned)
}

fn schema_type_matches(schema: &Value, expected: &str) -> bool {
    match schema.get("type") {
        Some(Value::String(actual)) => actual == expected,
        Some(Value::Array(actual)) => actual.iter().any(|value| value.as_str() == Some(expected)),
        _ => false,
    }
}

fn redact_credentials(mut message: String, credentials: &[CredentialGrant]) -> String {
    let mut values = credentials
        .iter()
        .map(|credential| credential.value.as_str())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    values.sort_unstable();
    values.dedup();
    values.sort_unstable_by_key(|value| std::cmp::Reverse(value.len()));
    for value in values {
        message = message.replace(value, "[REDACTED]");
    }
    message
}

fn non_empty(value: &str) -> Option<&str> {
    let value = value.trim();
    (!value.is_empty()).then_some(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use loom_protocol::FrameworkExecutionContext;
    use serde_json::json;
    use std::path::PathBuf;
    #[cfg(windows)]
    use std::{
        io::{BufRead, BufReader, Write},
        net::TcpListener,
        thread,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn request() -> FrameworkExecuteRequest {
        FrameworkExecuteRequest {
            protocol_version: "loom.framework.v1".to_owned(),
            supported_protocol_versions: vec!["loom.framework.v1".to_owned()],
            framework_id: "mcp".to_owned(),
            art_id: "fixture".to_owned(),
            art_dir: PathBuf::from("art"),
            inputs: json!({ "input": "from-input", "disabled": true }),
            params: json!({ "query": "loom", "count": "2" }),
            disabled_params: vec!["disabled".to_owned()],
            context: FrameworkExecutionContext {
                credentials: vec![CredentialGrant {
                    name: "api_key".to_owned(),
                    value: "secret-value".to_owned(),
                    expires_at: None,
                }],
                ..FrameworkExecutionContext::default()
            },
        }
    }

    #[test]
    fn arguments_merge_defaults_inputs_and_params() {
        let arguments = build_arguments(
            &request(),
            &json!({ "query": "default", "safesearch": "strict" }),
        )
        .unwrap();
        assert_eq!(
            arguments,
            json!({
                "input": "from-input",
                "query": "loom",
                "count": "2",
                "safesearch": "strict"
            })
        );
    }

    #[test]
    fn credential_alias_maps_to_server_environment() {
        let config = McpArtConfig {
            server_id: "fixture".to_owned(),
            command: "fixture".to_owned(),
            args: Vec::new(),
            tool_name: "search".to_owned(),
            env: BTreeMap::new(),
            credential_env: BTreeMap::from([("BRAVE_API_KEY".to_owned(), "api_key".to_owned())]),
            transport: McpTransport::Stdio,
            url: String::new(),
            headers: BTreeMap::new(),
            credential_headers: BTreeMap::new(),
            arguments: Value::Null,
        };
        let environment = build_environment(&request(), &config).unwrap();
        assert_eq!(
            environment.get("BRAVE_API_KEY"),
            Some(&"secret-value".to_owned())
        );
    }

    #[test]
    fn schema_normalizes_mcp_argument_types() {
        let normalized = normalize_arguments(
            &json!({ "count": "2", "spellcheck": "true", "search_lang": "zh-cn" }),
            &json!({
                "properties": {
                    "count": { "type": "integer" },
                    "spellcheck": { "type": "boolean" },
                    "search_lang": { "type": "string", "enum": ["en", "zh-hans"] }
                }
            }),
        );
        assert_eq!(
            normalized,
            json!({ "count": 2, "spellcheck": true, "search_lang": "zh-hans" })
        );
    }

    #[test]
    fn schema_excludes_arguments_rejected_by_the_mcp_tool() {
        let normalized = normalize_arguments(
            &json!({
                "query": "red panda",
                "count": "2",
                "result_index": 1,
                "__exec_manualTrigger": 123,
                "force_update": 456
            }),
            &json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string" },
                    "count": { "type": "integer" }
                },
                "additionalProperties": false
            }),
        );
        assert_eq!(normalized, json!({ "query": "red panda", "count": 2 }));
    }

    #[test]
    fn schema_preserves_arguments_when_pattern_properties_define_dynamic_keys() {
        let normalized = normalize_arguments(
            &json!({ "header_x": "visible" }),
            &json!({
                "type": "object",
                "properties": {},
                "patternProperties": { "^header_": { "type": "string" } },
                "additionalProperties": false
            }),
        );
        assert_eq!(normalized, json!({ "header_x": "visible" }));
    }

    #[test]
    fn credential_values_are_redacted_from_mcp_errors() {
        assert_eq!(
            redact_credentials(
                "server printed secret-value".to_owned(),
                &request().context.credentials
            ),
            "server printed [REDACTED]"
        );
    }

    #[test]
    fn short_credential_values_are_redacted_from_mcp_errors() {
        assert_eq!(
            redact_credentials(
                "server printed key=abc".to_owned(),
                &[CredentialGrant {
                    name: "short_key".to_owned(),
                    value: "abc".to_owned(),
                    expires_at: None,
                }]
            ),
            "server printed key=[REDACTED]"
        );
    }

    #[cfg(windows)]
    #[test]
    fn package_local_image_search_server_executes_through_mcp_framework() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind fixture image API");
        let address = listener.local_addr().expect("fixture image API address");
        let fixture = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept image API request");
            let mut reader = BufReader::new(stream.try_clone().expect("clone fixture stream"));
            let mut request = String::new();
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).expect("read request line");
                if line == "\r\n" || line.is_empty() {
                    break;
                }
                request.push_str(&line);
            }
            assert!(request.starts_with(
                "GET /res/v1/images/search?q=loom%20framework&count=2&safesearch=strict HTTP/1.1\r\n"
            ));
            assert!(request
                .to_ascii_lowercase()
                .contains("x-subscription-token: fixture-api-key\r\n"));

            let body = json!({
                "results": [
                    {
                        "title": "Loom first",
                        "url": "https://example.test/source/1",
                        "thumbnail": { "src": "https://cdn.example.test/thumb-1.jpg" },
                        "properties": {
                            "url": "https://cdn.example.test/image-1.png",
                            "width": 640,
                            "height": 480
                        }
                    },
                    {
                        "title": "Loom second",
                        "url": "https://example.test/source/2",
                        "thumbnail": { "src": "https://cdn.example.test/thumb-2.jpg" },
                        "properties": {}
                    }
                ]
            })
            .to_string();
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .expect("write fixture response");
            stream.flush().expect("flush fixture response");
        });

        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let art_dir = std::env::temp_dir().join(format!(
            "loom-image-search-mcp-framework-{}-{suffix}",
            std::process::id()
        ));
        let runtime_dir = art_dir.join("runtime");
        fs::create_dir_all(&runtime_dir).expect("create staged Art runtime");
        let source_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../art-packages/samples/image-search");
        fs::copy(
            source_dir.join("runtime/image-search-mcp.ps1"),
            runtime_dir.join("image-search-mcp.ps1"),
        )
        .expect("stage package-local image-search MCP server");
        let mut manifest: Value = serde_json::from_slice(
            &fs::read(source_dir.join("manifest.json")).expect("read image-search manifest"),
        )
        .expect("parse image-search manifest");
        manifest["metadata"]["mcp"]["args"] = json!([
            "-Endpoint",
            format!("http://{address}/res/v1/images/search")
        ]);
        fs::write(
            art_dir.join("manifest.json"),
            serde_json::to_vec(&manifest).expect("serialize staged manifest"),
        )
        .expect("write staged manifest");

        let mut request = request();
        request.art_id = "custom-image-search".to_owned();
        request.art_dir = art_dir.clone();
        request.inputs = json!({});
        request.params = json!({
            "query": "loom framework",
            "count": "2",
            "result_index": 1,
            "__exec_manualTrigger": 123,
            "force_update": 456
        });
        request.context.credentials = vec![CredentialGrant {
            name: "brave_api_key".to_owned(),
            value: "fixture-api-key".to_owned(),
            expires_at: None,
        }];

        let execution = execute(&request, &art_dir).expect("execute package-local MCP server");
        assert_eq!(execution.server_id, "neuro-image-search");
        assert_eq!(execution.tool_name, "brave_image_search");
        assert_eq!(execution.result["structuredContent"]["count"], 2);
        assert!(
            serde_json::to_value(&execution)
                .expect("serialize MCP execution")
                .get("arguments")
                .is_none(),
            "MCP arguments must not be echoed into the Art runtime payload"
        );
        assert_eq!(
            execution.result["structuredContent"]["candidates"][0]["imageUrl"],
            "https://cdn.example.test/image-1.png"
        );
        assert_eq!(
            execution.result["structuredContent"]["candidates"][1]["imageUrl"],
            "https://cdn.example.test/thumb-2.jpg"
        );

        fixture.join().expect("image API fixture thread");
        fs::remove_dir_all(&art_dir).expect("remove staged image-search Art");
    }
}
