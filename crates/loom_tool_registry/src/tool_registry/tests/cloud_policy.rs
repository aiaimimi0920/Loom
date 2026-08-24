//! Cloud policy and request template coverage.

use super::*;

#[test]
pub(super) fn a_cloud_art_deadline_can_be_raised_by_the_caller_and_by_the_package() {
    let mut tool = ToolDefinition::new(
        "fixture-cloud-timeout",
        "Fixture Cloud Timeout",
        "Deadline resolution only",
        ToolExecution::CloudApi {
            endpoint: "https://api.example.com/run".to_owned(),
            method: "POST".to_owned(),
            content_type: None,
            headers: None,
            body: None,
        },
    );

    // Nothing declared, nothing requested: the default.
    assert_eq!(cloud_api_timeout(&tool, None), CLOUD_API_TIMEOUT);
    // A caller's deadline is honoured rather than clamped down to the default.
    assert_eq!(
        cloud_api_timeout(&tool, Some(Duration::from_secs(120))),
        Duration::from_secs(120)
    );
    // A package declaration applies when the caller states nothing.
    tool.metadata = Some(serde_json::json!({ "cloudApi": { "timeoutMs": 90_000 } }));
    assert_eq!(cloud_api_timeout(&tool, None), Duration::from_secs(90));
    // An explicit caller deadline still wins over the declaration.
    assert_eq!(
        cloud_api_timeout(&tool, Some(Duration::from_secs(5))),
        Duration::from_secs(5)
    );
    // Both sides are bounded by the host ceiling, and zero never means "no timeout".
    tool.metadata = Some(serde_json::json!({ "cloudApi": { "timeoutMs": 9_000_000 } }));
    assert_eq!(cloud_api_timeout(&tool, None), CLOUD_API_MAX_TIMEOUT);
    assert_eq!(
        cloud_api_timeout(&tool, Some(Duration::from_secs(4_000))),
        CLOUD_API_MAX_TIMEOUT
    );
    assert_eq!(
        cloud_api_timeout(&tool, Some(Duration::ZERO)),
        Duration::from_millis(1)
    );
}

#[test]
pub(super) fn execute_cloud_api_tool_supports_formal_multipart_template_contract() {
    let root = temp_root("cloud-multipart-template");
    let upload_path = root.join("upload.png");
    fs::write(&upload_path, b"loom-upload").expect("write upload fixture");

    let fixture = CloudFixture::start(CloudFixtureMode::MultipartText);
    let tool: ToolDefinition = serde_json::from_value(serde_json::json!({
        "id": "fixture-cloud-multipart",
        "name": "Fixture Cloud Multipart",
        "description": "Call a formal multipart cloud API",
        "enabled": true,
        "execution": {
            "type": "cloud_api",
            "endpoint": fixture.url("/upload/{{inputs.route.value}}?mode={{mode}}"),
            "method": "POST",
            "contentType": "multipart/form-data",
            "headers": "{\"X-Trace\":\"{{inputs.trace.value}}\",\"X-Mode\":\"{{mode}}\"}",
            "body": "{\"file\":\"{{inputs.image.path}}\",\"prompt\":\"{{inputs.prompt.value}}\",\"literal\":\"fixed\",\"skipEmpty\":\"{{inputs.empty.value}}\",\"skipDisabled\":\"{{inputs.disabled.value}}\"}"
        },
        "metadata": { "permissionPolicy": { "network": { "allowLocalhost": true } } }
    }))
    .expect("formal multipart cloud API execution deserializes");

    let result = execute_tool(
        &tool,
        &[],
        serde_json::json!({
            "route": "image",
            "mode": "fast",
            "trace": "trace-42",
            "image": upload_path.display().to_string(),
            "prompt": "hello multipart",
            "empty": "",
            "disabled": "__DISABLED__"
        }),
    )
    .expect("execute formal multipart cloud API tool");

    assert_eq!(result["content"][0]["type"], "text");
    assert_eq!(result["content"][0]["text"], "cloud saw multipart");

    let request = fixture.request();
    let request_lower = request.to_ascii_lowercase();
    assert!(request.starts_with("POST /upload/image?mode=fast HTTP/1.1"));
    assert!(request_lower.contains("x-trace: trace-42"));
    assert!(request_lower.contains("x-mode: fast"));
    assert!(request_lower.contains("content-type: multipart/form-data; boundary="));
    assert!(request.contains("name=\"file\""));
    assert!(request.contains("filename=\"upload.png\""));
    assert!(request.contains("loom-upload"));
    assert!(request.contains("name=\"prompt\""));
    assert!(request.contains("\r\nhello multipart\r\n"));
    assert!(request.contains("name=\"literal\""));
    assert!(request.contains("\r\nfixed\r\n"));
    assert!(!request.contains("skipEmpty"));
    assert!(!request.contains("skipDisabled"));
    assert!(!request.contains("{{"));

    fs::remove_dir_all(root).expect("cleanup multipart template root");
}

#[test]
pub(super) fn only_a_templates_own_placeholders_count_as_unresolved() {
    assert_eq!(
        unresolved_cloud_template_placeholder("{{inputs.prompt.value}}", "{{inputs.prompt.value}}"),
        Some("{{inputs.prompt.value}}")
    );
    assert_eq!(
        unresolved_cloud_template_placeholder("{{prompt}}", "a filled value"),
        None
    );
    // Braces that arrived inside an argument's value are content, not an unfilled placeholder.
    assert_eq!(
        unresolved_cloud_template_placeholder("{{prompt}}", "render {{this}} literally"),
        None
    );
    // An unterminated `{{` cannot be substituted by anything, so it is not reported either.
    assert_eq!(
        unresolved_cloud_template_placeholder("{{prompt", "{{prompt"),
        None
    );
}

#[test]
pub(super) fn a_multipart_field_with_an_unfilled_placeholder_is_reported_not_dropped() {
    let tool = ToolDefinition::new(
        "fixture-cloud-multipart-unresolved",
        "Fixture Cloud Multipart Unresolved",
        "Report a multipart field whose binding never resolved",
        ToolExecution::CloudApi {
            endpoint: "https://example.com/upload".to_owned(),
            method: "POST".to_owned(),
            content_type: Some("multipart/form-data".to_owned()),
            headers: None,
            body: Some("{\"prompt\":\"{{inputs.prompt.value}}\"}".to_owned()),
        },
    );

    let arguments = serde_json::json!({ "unrelated": "value" });
    let error = run_cloud_future(build_cloud_multipart_form(
        &tool,
        Some("{\"prompt\":\"{{inputs.prompt.value}}\"}"),
        &arguments,
    ))
    .expect("run multipart builder")
    .expect_err("an unresolved multipart binding is an error");

    let message = error.to_string();
    assert!(message.contains("prompt"));
    assert!(message.contains("{{inputs.prompt.value}}"));
}

#[test]
pub(super) fn a_body_declared_on_a_method_that_cannot_send_one_is_rejected() {
    let tool: ToolDefinition = serde_json::from_value(serde_json::json!({
        "id": "fixture-cloud-get-body",
        "name": "Fixture Cloud GET Body",
        "description": "Declare a body on a method that never sends it",
        "enabled": true,
        "execution": {
            "type": "cloud_api",
            "endpoint": "https://example.com/search",
            "method": "GET",
            "body": "{\"query\":\"{{inputs.query.value}}\"}"
        }
    }))
    .expect("GET cloud API tool deserializes");

    let error = execute_tool(&tool, &[], serde_json::json!({ "query": "cat" }))
        .expect_err("a body on GET is an authoring mistake, not a silent drop");

    let message = error.to_string();
    assert!(message.contains("GET"));
    assert!(message.contains("does not send one"));
}

#[test]
pub(super) fn a_multipart_field_named_file_no_longer_uploads_a_caller_named_path() {
    let root = temp_root("cloud-multipart-field-name");
    let secret_path = root.join("private-key");
    fs::write(&secret_path, b"BEGIN PRIVATE KEY").expect("write secret fixture");

    let fixture = CloudFixture::start(CloudFixtureMode::MultipartText);
    // The author bound an ordinary value, not a path. Before this fix the field *name* alone
    // made the host read the caller's path off disk and upload the bytes.
    let tool: ToolDefinition = serde_json::from_value(serde_json::json!({
        "id": "fixture-cloud-multipart-field-name",
        "name": "Fixture Cloud Multipart Field Name",
        "description": "Call a multipart cloud API with a value-bound field named file",
        "enabled": true,
        "execution": {
            "type": "cloud_api",
            "endpoint": fixture.url("/upload/plain"),
            "method": "POST",
            "contentType": "multipart/form-data",
            "body": "{\"file\":\"{{inputs.file.value}}\",\"image_file\":\"{{inputs.file.value}}\"}"
        },
        "metadata": { "permissionPolicy": { "network": { "allowLocalhost": true } } }
    }))
    .expect("multipart cloud API execution deserializes");

    execute_tool(
        &tool,
        &[],
        serde_json::json!({ "file": secret_path.display().to_string() }),
    )
    .expect("execute multipart cloud API tool");

    let request = fixture.request();
    assert!(request.contains("name=\"file\""));
    assert!(
        !request.contains("filename="),
        "a value-bound field must travel as text: {request}"
    );
    assert!(
        !request.contains("BEGIN PRIVATE KEY"),
        "the named file's contents must never be uploaded: {request}"
    );

    fs::remove_dir_all(root).expect("cleanup multipart field name root");
}

#[test]
pub(super) fn a_declared_multipart_upload_path_has_to_sit_inside_a_loom_owned_root() {
    let root = temp_root("cloud-multipart-containment");
    let inside = root.join("staged-input.png");
    fs::write(&inside, b"staged").expect("write staged input");

    // A directory the host does not own, next to Loom's own staging directories rather than
    // inside one, standing in for any local file a caller might name.
    let outside_root = std::env::temp_dir().join(format!(
        "cloud-upload-probe-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    fs::create_dir_all(&outside_root).expect("create probe root");
    let outside = outside_root.join("private-key");
    fs::write(&outside, b"BEGIN PRIVATE KEY").expect("write probe secret");

    let tool = ToolDefinition::new(
        "fixture-cloud-containment",
        "Fixture Cloud Containment",
        "Upload path resolution only",
        ToolExecution::CloudApi {
            endpoint: "https://api.example.com/upload".to_owned(),
            method: "POST".to_owned(),
            content_type: Some("multipart/form-data".to_owned()),
            headers: None,
            body: None,
        },
    );

    assert_eq!(
        cloud_multipart_upload_path(&tool, "file", &inside.display().to_string())
            .expect("a staged input under a Loom temp root is accepted"),
        fs::canonicalize(&inside).expect("canonical staged input")
    );

    let error = cloud_multipart_upload_path(&tool, "file", &outside.display().to_string())
        .expect_err("a file outside every Loom-owned root is refused");
    assert!(
        error.to_string().contains("resolves outside"),
        "unexpected refusal reason: {error}"
    );

    assert!(
        cloud_multipart_upload_path(&tool, "file", &root.display().to_string())
            .expect_err("a directory is not an upload")
            .to_string()
            .contains("is not a file")
    );
    assert!(cloud_multipart_upload_path(
        &tool,
        "file",
        &root.join("absent.png").display().to_string()
    )
    .expect_err("a missing path is refused")
    .to_string()
    .contains("cannot resolve upload path"));

    // A package that ships its own resource declares where it lives, and that directory vouches
    // for its subtree even when it sits nowhere near the host temp directory.
    let mut packaged = tool.clone();
    packaged.metadata = Some(serde_json::json!({
        "artPackage": { "dir": outside_root.display().to_string() }
    }));
    assert!(
        cloud_multipart_upload_path(&packaged, "file", &outside.display().to_string()).is_ok(),
        "a file inside the declared Art package directory is uploadable"
    );

    fs::remove_dir_all(outside_root).expect("cleanup probe root");
    fs::remove_dir_all(root).expect("cleanup multipart containment root");
}

#[test]
pub(super) fn only_a_declared_path_binding_makes_a_multipart_field_a_file() {
    assert!(is_cloud_multipart_file_field("{{inputs.input.path}}"));
    assert!(is_cloud_multipart_file_field("{{inputs.image}}"));
    // Field names no longer decide this, and neither does a value binding.
    assert!(!is_cloud_multipart_file_field("{{inputs.file.value}}"));
    assert!(!is_cloud_multipart_file_field("{{prompt}}"));
    assert!(!is_cloud_multipart_file_field("fixed"));
}

#[test]
pub(super) fn an_endpoint_argument_cannot_rewrite_the_request_authority() {
    let arguments = serde_json::json!({ "suffix": "@127.0.0.1:8787/steal", "route": "image-v2" });
    let rendered = substitute_cloud_template_with(
        "https://api.example.com{{inputs.suffix}}",
        &arguments,
        percent_encode_cloud_template_value,
    );
    assert_eq!(
        rendered,
        "https://api.example.com%40127.0.0.1%3A8787%2Fsteal"
    );
    assert!(!rendered.contains('@'));

    // Unreserved characters still travel through untouched, so ordinary route and parameter
    // bindings render the way their authors wrote them.
    assert_eq!(
        substitute_cloud_template_with(
            "https://api.example.com/v1/{{inputs.route.value}}?mode={{route}}",
            &arguments,
            percent_encode_cloud_template_value,
        ),
        "https://api.example.com/v1/image-v2?mode=image-v2"
    );

    // The authority guard states the invariant outright, independently of the encoding.
    assert!(validate_rendered_cloud_authority(
        "https://api.example.com/v1/{{inputs.route.value}}",
        "https://api.example.com/v1/image-v2",
    )
    .is_ok());
    assert!(validate_rendered_cloud_authority(
        "https://api.example.com/v1",
        "https://api.example.com@127.0.0.1:8787/v1",
    )
    .expect_err("a moved authority is refused")
    .contains("does not match the declared authority"));
    // An author who templates the host itself is trusted to have meant it; the declared domain
    // list is what constrains that case.
    assert!(validate_rendered_cloud_authority(
        "https://{{inputs.region}}.api.example.com/v1",
        "https://eu.api.example.com/v1",
    )
    .is_ok());
}

#[test]
pub(super) fn a_json_body_argument_cannot_add_sibling_fields() {
    let fixture = CloudFixture::start(CloudFixtureMode::Text);
    let tool: ToolDefinition = serde_json::from_value(serde_json::json!({
        "id": "fixture-cloud-json-injection",
        "name": "Fixture Cloud JSON Injection",
        "description": "Call a JSON cloud API with a quote-carrying argument",
        "enabled": true,
        "execution": {
            "type": "cloud_api",
            "endpoint": fixture.url("/text"),
            "method": "POST",
            "contentType": "application/json",
            "body": "{\"prompt\":\"{{inputs.text}}\"}"
        },
        "metadata": { "permissionPolicy": { "network": { "allowLocalhost": true } } }
    }))
    .expect("JSON cloud API execution deserializes");

    let injection = "x\",\"stream\":true,\"model\":\"attacker";
    execute_tool(&tool, &[], serde_json::json!({ "text": injection }))
        .expect("execute JSON cloud API tool");

    let request = fixture.request();
    let body = request
        .split_once("\r\n\r\n")
        .map(|(_, body)| body.to_owned())
        .expect("captured request body");
    let sent = serde_json::from_str::<serde_json::Value>(&body).expect("request body is JSON");
    let sent = sent.as_object().expect("request body is an object");
    // The argument stays one string value: it cannot become extra request members.
    assert_eq!(sent.len(), 1);
    assert_eq!(sent["prompt"], serde_json::json!(injection));
    assert!(sent.get("stream").is_none());
    assert!(sent.get("model").is_none());
}

#[test]
pub(super) fn a_header_argument_stays_one_header_value() {
    let fixture = CloudFixture::start(CloudFixtureMode::Text);
    let tool: ToolDefinition = serde_json::from_value(serde_json::json!({
        "id": "fixture-cloud-header-injection",
        "name": "Fixture Cloud Header Injection",
        "description": "Call a JSON cloud API with a quote-carrying header argument",
        "enabled": true,
        "execution": {
            "type": "cloud_api",
            "endpoint": fixture.url("/text"),
            "method": "POST",
            "contentType": "application/json",
            "headers": "{\"X-Trace\":\"{{inputs.trace.value}}\"}",
            "body": "{\"prompt\":\"{{inputs.prompt.value}}\"}"
        },
        "metadata": { "permissionPolicy": { "network": { "allowLocalhost": true } } }
    }))
    .expect("header template cloud API execution deserializes");

    execute_tool(
        &tool,
        &[],
        serde_json::json!({
            "trace": "trace-42\",\"X-Injected\":\"yes",
            "prompt": "hello"
        }),
    )
    .expect("execute header template cloud API tool");

    let request = fixture.request();
    let request_lower = request.to_ascii_lowercase();
    assert!(!request_lower.contains("x-injected:"));
    assert_eq!(request_lower.matches("x-trace:").count(), 1);
    assert!(request.contains("trace-42\",\"X-Injected\":\"yes"));
}

#[test]
pub(super) fn a_header_argument_carrying_a_line_break_is_refused() {
    let fixture = CloudFixture::start(CloudFixtureMode::Text);
    let tool: ToolDefinition = serde_json::from_value(serde_json::json!({
        "id": "fixture-cloud-header-control",
        "name": "Fixture Cloud Header Control",
        "description": "Call a JSON cloud API with a line break in a header argument",
        "enabled": true,
        "execution": {
            "type": "cloud_api",
            "endpoint": fixture.url("/text"),
            "method": "POST",
            "contentType": "application/json",
            "headers": "{\"X-Trace\":\"{{inputs.trace.value}}\"}",
            "body": "{\"prompt\":\"hello\"}"
        },
        "metadata": { "permissionPolicy": { "network": { "allowLocalhost": true } } }
    }))
    .expect("header control cloud API execution deserializes");

    let error = execute_tool(
        &tool,
        &[],
        serde_json::json!({ "trace": "trace-42\r\nX-Injected: yes" }),
    )
    .expect_err("a header value carrying a line break is refused");
    assert!(error.to_string().contains("control character"));
}

#[test]
pub(super) fn a_json_body_template_that_is_not_json_yet_still_renders() {
    // A placeholder standing in for an unquoted number cannot be parsed before substitution, so
    // that template keeps the original splice-then-parse path.
    let tool: ToolDefinition = serde_json::from_value(serde_json::json!({
        "id": "fixture-cloud-typed-template",
        "name": "Fixture Cloud Typed Template",
        "description": "Render a body template whose placeholder is an unquoted value",
        "enabled": true,
        "execution": {
            "type": "cloud_api",
            "endpoint": "https://example.invalid/v1",
            "method": "POST"
        }
    }))
    .expect("typed template cloud API execution deserializes");

    let rendered = render_cloud_json_template(
        &tool,
        "body",
        "{\"steps\": {{inputs.steps.value}}, \"prompt\": \"{{inputs.prompt.value}}\"}",
        &serde_json::json!({ "steps": 12, "prompt": "hello" }),
    )
    .expect("render an unquoted placeholder body");
    assert_eq!(rendered["steps"], serde_json::json!(12));
    assert_eq!(rendered["prompt"], serde_json::json!("hello"));

    // A templated object key still renders on the structural path.
    let keyed = render_cloud_json_template(
        &tool,
        "body",
        "{\"{{inputs.field.value}}\":\"{{inputs.prompt.value}}\"}",
        &serde_json::json!({ "field": "prompt", "prompt": "hello" }),
    )
    .expect("render a templated body key");
    assert_eq!(keyed["prompt"], serde_json::json!("hello"));
}
