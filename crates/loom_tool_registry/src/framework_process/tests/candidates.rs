use super::super::*;
use std::fs;
use std::path::PathBuf;

fn temp_root(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("loom-framework-process-{name}-{}", request_id()));
    fs::create_dir_all(&root).expect("create process test root");
    root
}

#[test]
fn framework_image_candidates_use_canonical_loom_metadata() {
    let mut tool = ToolDefinition::new(
        "fixture-image-art",
        "Fixture Image Art",
        "Projects framework candidates for Hook.",
        crate::ToolExecution::FrameworkArt {
            framework: "mcp".to_owned(),
        },
    );
    tool.outputs = vec![json!({
        "name": "output",
        "type": "image",
        "execution_type": "image_buffer",
    })];
    let response: FrameworkExecuteResponse = serde_json::from_value(json!({
        "status": "success",
        "output": {
            "content": [{ "type": "image", "data": "fixture", "mimeType": "image/png" }],
            "selectedCandidate": "candidate-2",
        },
        "candidates": [
            { "id": "candidate-1", "index": 0, "data": "first" },
            { "id": "candidate-2", "index": 1, "data": "second" },
        ],
    }))
    .expect("framework response");

    let result = response_to_tool_value(&tool, response);
    assert!(result.get("candidates").is_none());
    assert_eq!(
        result["loomMetadata"]["candidates"]["kind"],
        "image.candidates"
    );
    assert_eq!(result["loomMetadata"]["candidates"]["selectedIndex"], 1);
    assert_eq!(
        result["loomMetadata"]["candidates"]["items"][1]["id"],
        "candidate-2"
    );
}

#[test]
fn framework_image_candidates_are_normalized_to_the_consumer_wire_shape() {
    let mut tool = ToolDefinition::new(
        "fixture-image-art",
        "Fixture Image Art",
        "Projects framework candidates for Hook.",
        crate::ToolExecution::FrameworkArt {
            framework: "mcp".to_owned(),
        },
    );
    tool.outputs = vec![json!({
        "name": "output",
        "type": "image",
        "execution_type": "image_buffer",
    })];
    let response: FrameworkExecuteResponse = serde_json::from_value(json!({
        "status": "success",
        "output": {
            "content": [{ "type": "image", "data": "fixture", "mimeType": "image/png" }],
        },
        "candidates": [
            // The shape the shipped image-search Art emits: no `imageUrl`, and the source page
            // under `sourceUrl`.
            {
                "id": "candidate-1",
                "title": "first",
                "thumbnail": "data:image/png;base64,AAA",
                "data": "data:image/png;base64,AAA",
                "sourceUrl": "https://example.test/one",
                "width": 10,
                "height": 20,
            },
            // An Art that already speaks the wire shape must pass through untouched.
            {
                "id": "candidate-2",
                "imageUrl": "https://example.test/two.png",
                "sourcePageUrl": "https://example.test/two",
                "index": 7,
            },
            // Nothing usable as an image reference: no key is invented.
            { "id": "candidate-3", "title": "third" },
        ],
    }))
    .expect("framework response");

    let result = response_to_tool_value(&tool, response);
    let items = &result["loomMetadata"]["candidates"]["items"];
    assert_eq!(items[0]["imageUrl"], "data:image/png;base64,AAA");
    assert_eq!(items[0]["thumbnailUrl"], "data:image/png;base64,AAA");
    assert_eq!(items[0]["sourcePageUrl"], "https://example.test/one");
    assert_eq!(items[0]["index"], 0);
    assert_eq!(items[0]["thumbnail"], "data:image/png;base64,AAA");
    assert_eq!(items[0]["id"], "candidate-1");
    assert_eq!(items[1]["imageUrl"], "https://example.test/two.png");
    assert_eq!(items[1]["sourcePageUrl"], "https://example.test/two");
    assert_eq!(items[1]["index"], 7);
    assert!(items[1].get("thumbnailUrl").is_none());
    assert!(items[2].get("imageUrl").is_none());
    assert_eq!(items[2]["index"], 2);
}

fn image_candidate_tool() -> ToolDefinition {
    let mut tool = ToolDefinition::new(
        "fixture-image-art",
        "Fixture Image Art",
        "Projects framework candidates for Hook.",
        crate::ToolExecution::FrameworkArt {
            framework: "mcp".to_owned(),
        },
    );
    tool.outputs = vec![json!({
        "name": "output",
        "type": "image",
        "execution_type": "image_buffer",
    })];
    tool
}

fn image_candidate_response(candidates: Vec<Value>) -> FrameworkExecuteResponse {
    serde_json::from_value(json!({
        "status": "success",
        "output": {
            "content": [{ "type": "image", "data": "fixture", "mimeType": "image/png" }],
        },
        "candidates": candidates,
    }))
    .expect("framework response")
}

#[test]
fn framework_image_candidates_are_capped_by_item_count() {
    let candidates = (0..(MAX_FRAMEWORK_CANDIDATES * 3))
        .map(|index| json!({ "id": format!("candidate-{index}"), "imageUrl": "https://a.test" }))
        .collect::<Vec<_>>();
    let result = response_to_tool_value(
        &image_candidate_tool(),
        image_candidate_response(candidates),
    );

    let metadata = &result["loomMetadata"]["candidates"];
    assert_eq!(
        metadata["items"].as_array().expect("items").len(),
        MAX_FRAMEWORK_CANDIDATES
    );
    assert_eq!(metadata["droppedItems"], MAX_FRAMEWORK_CANDIDATES * 2);
    assert_eq!(metadata["items"][0]["id"], "candidate-0");
}

#[test]
fn framework_image_candidates_are_capped_by_total_bytes() {
    let payload = "d".repeat(MAX_FRAMEWORK_CANDIDATE_BYTES / 3);
    let candidates = (0..3)
        .map(|index| json!({ "id": format!("candidate-{index}"), "data": payload.clone() }))
        .collect::<Vec<_>>();
    let result = response_to_tool_value(
        &image_candidate_tool(),
        image_candidate_response(candidates),
    );

    let metadata = &result["loomMetadata"]["candidates"];
    assert_eq!(metadata["items"].as_array().expect("items").len(), 2);
    assert_eq!(metadata["droppedItems"], 1);
    assert_eq!(metadata["items"][1]["id"], "candidate-1");
}

#[test]
fn a_single_oversized_framework_candidate_is_still_delivered() {
    let payload = "d".repeat(MAX_FRAMEWORK_CANDIDATE_BYTES + 1024);
    let result = response_to_tool_value(
        &image_candidate_tool(),
        image_candidate_response(vec![json!({ "id": "only", "data": payload })]),
    );

    let metadata = &result["loomMetadata"]["candidates"];
    assert_eq!(metadata["items"].as_array().expect("items").len(), 1);
    assert_eq!(metadata["droppedItems"], 0);
}

#[test]
fn framework_image_output_drops_the_self_declared_base64_copy() {
    let root = temp_root("image-output-dedupe");
    let data_url =
        loom_image_io::rgba8_to_png_data_url(1, 1, &[255, 0, 0, 255]).expect("encode fixture png");
    let image_path = root.join("output.png");
    fs::write(
        &image_path,
        loom_image_io::decode_data_url_bytes(&data_url).expect("decode fixture png"),
    )
    .expect("write fixture png");

    let tool = image_candidate_tool();
    let mut output = json!({
        "output_base64": data_url,
        "outputBase64": "a stale second copy",
        "output_path": image_path.to_string_lossy(),
        "width": 1,
        "height": 1,
    });
    normalize_framework_image_output(&tool, "mcp", &mut output, &[root.as_path()])
        .expect("normalize image output");

    let output = output.as_object().expect("normalized output object");
    assert!(
        !output.contains_key("output_base64") && !output.contains_key("outputBase64"),
        "the host kept a self-declared base64 copy beside the content it built"
    );
    assert!(!output.contains_key("output_path"));
    assert_eq!(output["content"][0]["type"], "image");
    assert!(output["content"][0]["data"]
        .as_str()
        .expect("content data url")
        .starts_with("data:image/png;base64,"));
    assert_eq!(output["width"], 1);

    fs::remove_dir_all(&root).ok();
}
