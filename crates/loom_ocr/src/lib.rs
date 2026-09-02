use std::fs;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

use ort::session::builder::SessionBuilder;

mod colors;
mod confidence;
mod ctc_decode;
mod ctc_recognizer;
mod geometry;
mod line_fragment_merge;
#[cfg(test)]
mod line_fragment_merge_tests;
mod list_marker_recovery;
mod ocr_core;
mod quality;
mod reading_order;
mod recognition_rescue;
mod span_geometry;
mod text_postprocess;
mod types;

use colors::estimate_text_and_background_color;
use geometry::{block_bounds, estimate_line_geometry};
use list_marker_recovery::recover_leading_list_markers;
use ocr_core::AlignedOcrCore;
pub use quality::{evaluate_quality, GoldenBlock, GoldenFixture, OcrQualityMetrics};
use span_geometry::project_text_spans;
use text_postprocess::correct_recognized_text;
pub use types::{
    EnhancedTextBlock, OcrDetectResult, OcrGeometrySource, OcrLineGeometry, OcrMetricPoint,
    OcrPoint, OcrTextConfidence, OcrTextConfidenceSource, OcrTextSpan, OcrTextSpanSource,
};

/// Controls the bounded accuracy/performance trade-off for one OCR request.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum OcrQualityMode {
    Quick,
    #[default]
    Auto,
    HighAccuracy,
}

pub const REQUIRED_RAPID_OCR_V4_MODELS: &[&str] = &[
    "ch_PP-OCRv4_det_infer.onnx",
    "ch_ppocr_mobile_v2.0_cls_infer.onnx",
    "ch_PP-OCRv4_rec_infer.onnx",
];
const OPTIONAL_RAPID_OCR_V5_RECOGNITION_MODEL: &str = "ch_PP-OCRv5_rec_mobile_infer.onnx";
const MAX_ENCODED_IMAGE_BYTES: usize = 128 * 1024 * 1024;
const MAX_IMAGE_DIMENSION: u32 = 16_384;
const MAX_IMAGE_ALLOCATION_BYTES: u64 = 256 * 1024 * 1024;
const MAX_MODEL_BYTES: u64 = 128 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum OcrError {
    #[error("Missing OCR model files in {root}: {missing}")]
    MissingModels { root: PathBuf, missing: String },

    #[error("Invalid OCR model directory: {0}")]
    InvalidModelDir(PathBuf),

    #[error("Failed to read OCR model {path}: {source}")]
    ReadModel {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("OCR model size is outside the supported range for {path}: {size} bytes")]
    InvalidModelSize { path: PathBuf, size: u64 },

    #[error("Invalid image: {0}")]
    InvalidImage(String),

    #[error("OCR engine initialization failed: {0}")]
    Init(String),

    #[error("OCR detection failed: {0}")]
    Detect(String),
}

pub type OcrResult<T> = Result<T, OcrError>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OcrModelSet {
    pub root: PathBuf,
    pub det_model: PathBuf,
    pub cls_model: PathBuf,
    pub rec_model: PathBuf,
    pub fallback_rec_model: Option<PathBuf>,
}

impl OcrModelSet {
    pub fn from_dir(root: impl AsRef<Path>) -> OcrResult<Self> {
        let root = root.as_ref().to_path_buf();
        if !root.is_dir() {
            return Err(OcrError::InvalidModelDir(root));
        }

        let missing = REQUIRED_RAPID_OCR_V4_MODELS
            .iter()
            .filter(|name| !root.join(name).is_file())
            .copied()
            .collect::<Vec<_>>();

        if !missing.is_empty() {
            return Err(OcrError::MissingModels {
                root,
                missing: missing.join(", "),
            });
        }

        let fallback_rec_model = root
            .join(OPTIONAL_RAPID_OCR_V5_RECOGNITION_MODEL)
            .is_file()
            .then(|| root.join(OPTIONAL_RAPID_OCR_V5_RECOGNITION_MODEL));
        Ok(Self {
            det_model: root.join(REQUIRED_RAPID_OCR_V4_MODELS[0]),
            cls_model: root.join(REQUIRED_RAPID_OCR_V4_MODELS[1]),
            rec_model: root.join(REQUIRED_RAPID_OCR_V4_MODELS[2]),
            fallback_rec_model,
            root,
        })
    }

    pub fn discover<'a, I, P>(candidates: I) -> OcrResult<Option<Self>>
    where
        I: IntoIterator<Item = P>,
        P: AsRef<Path> + 'a,
    {
        for candidate in candidates {
            let candidate = candidate.as_ref();
            if candidate.is_dir() {
                match Self::from_dir(candidate) {
                    Ok(model_set) => return Ok(Some(model_set)),
                    Err(OcrError::MissingModels { .. }) => {}
                    Err(error) => return Err(error),
                }
            }
        }
        Ok(None)
    }
}

#[derive(Debug)]
pub struct OcrEngine {
    model_set: OcrModelSet,
    core: Option<AlignedOcrCore>,
}

impl OcrEngine {
    pub fn new(model_set: OcrModelSet) -> OcrResult<Self> {
        Ok(Self {
            model_set,
            core: None,
        })
    }

    pub fn detect_image_bytes(
        &mut self,
        image_data: &[u8],
        detect_angle: bool,
    ) -> OcrResult<OcrDetectResult> {
        self.detect_image_bytes_with_mode(image_data, detect_angle, OcrQualityMode::Auto)
    }

    pub fn detect_image_bytes_with_mode(
        &mut self,
        image_data: &[u8],
        detect_angle: bool,
        quality_mode: OcrQualityMode,
    ) -> OcrResult<OcrDetectResult> {
        let image = decode_image(image_data)?;
        let width = image.width();
        let height = image.height();
        let image_buffer = image.to_rgb8();
        let result = self
            .session()?
            .detect(&image_buffer, detect_angle, quality_mode)?;

        let mut text_blocks = Vec::new();
        for block in result {
            let box_points = block.box_points;
            let Some(bounds) = block_bounds(&box_points, width, height) else {
                continue;
            };
            // Do not apply a pixel-size cutoff here. A single punctuation mark
            // (for example `#` or `-`) can legitimately have a narrow box and
            // must remain available to the overlay and clipboard paths.
            if bounds.width() == 0 || bounds.height() == 0 || block.line.text.trim().is_empty() {
                continue;
            }

            let line_geometry = estimate_line_geometry(&box_points);
            let confidence = confidence::summarize_line(&block.line);
            let (character_spans, word_spans) =
                project_text_spans(&block.line, &box_points, block.axis, block.reverse_axis);
            let corrected = correct_recognized_text(block.line.text);
            let (color_hex, bg_color_hex) =
                estimate_text_and_background_color(&image_buffer, bounds);

            text_blocks.push(EnhancedTextBlock {
                box_points,
                box_score: block.box_score,
                text: corrected.text,
                text_score: block.line.text_score,
                color_hex,
                bg_color_hex,
                raw_text: corrected.raw_text,
                confidence,
                line_geometry,
                character_spans,
                word_spans,
            });
        }
        let mut text_blocks = line_fragment_merge::merge_line_fragments(text_blocks, width, height);
        recover_leading_list_markers(&image_buffer, &mut text_blocks);
        reading_order::sort_blocks(&mut text_blocks, width, height);
        let full_text = text_blocks
            .iter()
            .map(|block| block.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");

        Ok(OcrDetectResult {
            text_blocks,
            scale_factor: 1.0,
            full_text,
            width,
            height,
        })
    }

    fn session(&mut self) -> OcrResult<&mut AlignedOcrCore> {
        if self.core.is_none() {
            initialize_onnx_runtime(&self.model_set.root)?;

            let det_model = read_model(&self.model_set.det_model)?;
            let cls_model = read_model(&self.model_set.cls_model)?;
            let rec_model = read_model(&self.model_set.rec_model)?;
            let fallback_rec_model = self
                .model_set
                .fallback_rec_model
                .as_deref()
                .map(read_model)
                .transpose()?;

            self.core = Some(AlignedOcrCore::from_models(
                det_model.as_ref(),
                cls_model.as_ref(),
                rec_model.as_ref(),
                fallback_rec_model,
                build_session,
            )?);
        }

        self.core
            .as_mut()
            .ok_or_else(|| OcrError::Init("OCR session missing after initialization".to_owned()))
    }
}

pub fn default_model_dir_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    if let Some(env_path) = std::env::var_os("LOOM_OCR_MODEL_DIR")
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
    {
        return vec![env_path];
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            candidates.push(exe_dir.join("resources").join("ocr"));
        }
    }

    if let Ok(current_dir) = std::env::current_dir() {
        candidates.push(current_dir.join("resources").join("ocr"));
        candidates.push(current_dir.join("Loom").join("resources").join("ocr"));
    }

    candidates.extend(manifest_resource_candidates());
    dedupe_paths(candidates)
}

pub fn discover_default_model_set() -> OcrResult<Option<OcrModelSet>> {
    OcrModelSet::discover(default_model_dir_candidates())
}

fn manifest_resource_candidates() -> Vec<PathBuf> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .ancestors()
        .map(|candidate| candidate.join("resources").join("ocr"))
        .collect()
}

fn dedupe_paths(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut deduped = Vec::new();
    for path in paths {
        if !deduped.iter().any(|existing: &PathBuf| existing == &path) {
            deduped.push(path);
        }
    }
    deduped
}

fn read_model(path: &Path) -> OcrResult<Vec<u8>> {
    let file = fs::File::open(path).map_err(|source| OcrError::ReadModel {
        path: path.to_path_buf(),
        source,
    })?;
    let size = file
        .metadata()
        .map_err(|source| OcrError::ReadModel {
            path: path.to_path_buf(),
            source,
        })?
        .len();
    if size == 0 || size > MAX_MODEL_BYTES {
        return Err(OcrError::InvalidModelSize {
            path: path.to_path_buf(),
            size,
        });
    }
    let mut bytes = Vec::with_capacity(size as usize);
    file.take(MAX_MODEL_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|source| OcrError::ReadModel {
            path: path.to_path_buf(),
            source,
        })?;
    if bytes.len() as u64 != size || bytes.len() as u64 > MAX_MODEL_BYTES {
        return Err(OcrError::InvalidModelSize {
            path: path.to_path_buf(),
            size: bytes.len() as u64,
        });
    }
    Ok(bytes)
}

fn initialize_onnx_runtime(model_root: &Path) -> OcrResult<()> {
    let runtime = model_root.join("onnxruntime.dll");
    ort::init_from(runtime.to_string_lossy().as_ref())
        .with_name("loom-ocr")
        .with_telemetry(false)
        .commit()
        .map(|_| ())
        .map_err(|error| OcrError::Init(error.to_string()))
}

fn decode_image(image_data: &[u8]) -> OcrResult<image::DynamicImage> {
    if image_data.is_empty() || image_data.len() > MAX_ENCODED_IMAGE_BYTES {
        return Err(OcrError::InvalidImage(
            "encoded image size is outside the supported range".to_owned(),
        ));
    }
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_IMAGE_DIMENSION);
    limits.max_image_height = Some(MAX_IMAGE_DIMENSION);
    limits.max_alloc = Some(MAX_IMAGE_ALLOCATION_BYTES);
    let mut reader = image::ImageReader::new(Cursor::new(image_data))
        .with_guessed_format()
        .map_err(|error| OcrError::InvalidImage(error.to_string()))?;
    reader.limits(limits);
    reader
        .decode()
        .map_err(|error| OcrError::InvalidImage(error.to_string()))
}

fn build_session(builder: SessionBuilder) -> Result<SessionBuilder, ort::Error> {
    let num_thread = ocr_thread_count();
    Ok(builder
        .with_inter_threads(1)?
        .with_intra_threads(num_thread)?
        .with_optimization_level(ort::session::builder::GraphOptimizationLevel::Level3)?)
}

fn ocr_thread_count() -> usize {
    let physical = num_cpus::get_physical().max(1);
    std::env::var("LOOM_OCR_THREADS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .map_or(physical.min(8), |value| value.min(physical).min(8))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::{Path, PathBuf};

    const OCR_TEST_IMAGE: &[u8] = include_bytes!("../../../resources/ocr/fixtures/test_1.png");

    #[test]
    fn validates_required_rapidocr_v4_model_files() {
        let root = unique_temp_dir("model-validation");
        fs::write(root.join(REQUIRED_RAPID_OCR_V4_MODELS[0]), b"det").expect("write det model");

        let error = OcrModelSet::from_dir(&root).expect_err("incomplete model set should fail");
        let message = error.to_string();
        assert!(message.contains("ch_ppocr_mobile_v2.0_cls_infer.onnx"));
        assert!(message.contains("ch_PP-OCRv4_rec_infer.onnx"));

        write_placeholder_model_set(&root);
        let model_set = OcrModelSet::from_dir(&root).expect("complete model set");
        assert_eq!(model_set.det_model, root.join("ch_PP-OCRv4_det_infer.onnx"));
        assert_eq!(
            model_set.cls_model,
            root.join("ch_ppocr_mobile_v2.0_cls_infer.onnx")
        );
        assert_eq!(model_set.rec_model, root.join("ch_PP-OCRv4_rec_infer.onnx"));

        fs::remove_dir_all(root).expect("cleanup temp dir");
    }

    #[test]
    fn discovers_first_complete_model_candidate() {
        let root = unique_temp_dir("model-discovery");
        let incomplete = root.join("incomplete");
        let complete = root.join("complete");
        fs::create_dir_all(&incomplete).expect("create incomplete dir");
        fs::create_dir_all(&complete).expect("create complete dir");
        fs::write(incomplete.join(REQUIRED_RAPID_OCR_V4_MODELS[0]), b"det")
            .expect("write incomplete det model");
        write_placeholder_model_set(&complete);

        let model_set = OcrModelSet::discover([incomplete.as_path(), complete.as_path()])
            .expect("discovery should not error")
            .expect("complete model set should be discovered");

        assert_eq!(model_set.root, complete);

        fs::remove_dir_all(root).expect("cleanup temp dir");
    }

    #[test]
    fn retains_narrow_punctuation_blocks() {
        let punctuation = block_bounds(&[OcrPoint { x: 4, y: 7 }], 10, 10)
            .expect("single-pixel punctuation bounds");
        assert_eq!(punctuation.width(), 1);
        assert_eq!(punctuation.height(), 1);
    }

    #[test]
    #[cfg_attr(
        not(windows),
        ignore = "packaged OCR validation requires the bundled Windows ONNX Runtime"
    )]
    fn real_engine_detects_text_from_packaged_fixture_image() {
        let resources = workspace_ocr_resources();
        let model_set = OcrModelSet::from_dir(&resources).expect("packaged OCR models");
        let mut engine = OcrEngine::new(model_set).expect("create OCR engine");
        let fixture = image::load_from_memory(OCR_TEST_IMAGE).expect("decode fixture image");
        let result = engine
            .detect_image_bytes(OCR_TEST_IMAGE, false)
            .expect("run OCR on fixture image");

        assert_eq!(result.width, fixture.width());
        assert_eq!(result.height, fixture.height());
        assert!(
            !result.full_text.trim().is_empty(),
            "real OCR should return non-empty full_text"
        );
        assert!(
            !result.text_blocks.is_empty(),
            "real OCR should return at least one text block"
        );
    }

    fn write_placeholder_model_set(root: &Path) {
        fs::create_dir_all(root).expect("create model dir");
        for name in REQUIRED_RAPID_OCR_V4_MODELS {
            fs::write(root.join(name), name.as_bytes()).expect("write placeholder model");
        }
    }

    fn workspace_ocr_resources() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .find_map(|candidate| {
                let path = candidate.join("resources").join("ocr");
                if path.join("ch_PP-OCRv4_det_infer.onnx").exists() {
                    Some(path)
                } else {
                    None
                }
            })
            .expect("locate Loom/resources/ocr")
    }

    fn unique_temp_dir(label: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "loom-ocr-{label}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time")
                .as_nanos()
        ));
        fs::create_dir_all(&root).expect("create temp dir");
        root
    }
}
