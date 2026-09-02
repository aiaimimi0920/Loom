use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use loom_ocr::{evaluate_quality, GoldenFixture, OcrEngine, OcrModelSet};

const MAX_INPUT_IMAGE_BYTES: u64 = 128 * 1024 * 1024;
const MAX_GOLDEN_JSON_BYTES: u64 = 1024 * 1024;

fn resource_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../resources/ocr")
        .join(relative)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let image_path = env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| resource_path("fixtures/test_1.png"));
    let golden_path = env::args_os()
        .nth(2)
        .map(PathBuf::from)
        .unwrap_or_else(|| resource_path("fixtures/golden/test_1.json"));
    let model_dir = env::args_os()
        .nth(3)
        .map(PathBuf::from)
        .unwrap_or_else(|| resource_path(""));
    if env::args_os().nth(4).is_some() {
        return Err("usage: measure_quality [image-path] [golden-json] [model-dir]".into());
    }

    let image = read_bounded(&image_path, MAX_INPUT_IMAGE_BYTES)?;
    let golden: GoldenFixture =
        serde_json::from_slice(&read_bounded(&golden_path, MAX_GOLDEN_JSON_BYTES)?)?;
    let models = OcrModelSet::from_dir(model_dir)?;
    let mut engine = OcrEngine::new(models)?;
    let started = Instant::now();
    let result = engine.detect_image_bytes(&image, false)?;
    let elapsed_ms = started.elapsed().as_millis();
    let metrics = evaluate_quality(&golden, &result);
    let report = serde_json::json!({
        "ocrMs": elapsed_ms,
        "imageBytes": image.len(),
        "fixture": golden.fixture,
        "actualFullText": result.full_text,
        "actualBlockTexts": result
            .text_blocks
            .iter()
            .map(|block| block.text.as_str())
            .collect::<Vec<_>>(),
        "actualBlockBoxes": result
            .text_blocks
            .iter()
            .map(|block| &block.box_points)
            .collect::<Vec<_>>(),
        "metrics": metrics,
    });
    println!("{}", serde_json::to_string(&report)?);
    Ok(())
}

fn read_bounded(path: &PathBuf, max_bytes: u64) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let size = fs::metadata(path)?.len();
    if size == 0 || size > max_bytes {
        return Err(format!("input size is outside the supported range: {size} bytes").into());
    }
    Ok(fs::read(path)?)
}
