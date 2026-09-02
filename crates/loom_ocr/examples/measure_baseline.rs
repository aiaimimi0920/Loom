use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use loom_ocr::{OcrEngine, OcrModelSet};

fn default_resource_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../resources/ocr")
        .join(relative)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args_os().skip(1);
    let image_path = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| default_resource_path("fixtures/test_1.png"));
    let model_dir = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| default_resource_path(""));
    if args.next().is_some() {
        return Err("usage: measure_baseline [image-path] [model-dir]".into());
    }

    let image = fs::read(&image_path)?;
    let load_started = Instant::now();
    let models = OcrModelSet::from_dir(&model_dir)?;
    let mut engine = OcrEngine::new(models)?;
    let model_load_ms = load_started.elapsed().as_millis();

    let cold_started = Instant::now();
    let cold = engine.detect_image_bytes(&image, false)?;
    let cold_ocr_ms = cold_started.elapsed().as_millis();

    let warm_started = Instant::now();
    let warm = engine.detect_image_bytes(&image, false)?;
    let warm_ocr_ms = warm_started.elapsed().as_millis();

    println!(
        concat!(
            "{{\"modelLoadMs\":{},\"coldOcrMs\":{},\"warmOcrMs\":{},",
            "\"imageBytes\":{},\"width\":{},\"height\":{},",
            "\"coldBlocks\":{},\"warmBlocks\":{}}}"
        ),
        model_load_ms,
        cold_ocr_ms,
        warm_ocr_ms,
        image.len(),
        cold.width,
        cold.height,
        cold.text_blocks.len(),
        warm.text_blocks.len(),
    );
    Ok(())
}
