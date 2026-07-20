use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use image::DynamicImage;
use ort::session::builder::SessionBuilder;
use paddle_ocr_rs::ocr_lite::OcrLite;
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use serde::{Deserialize, Serialize};

pub const REQUIRED_RAPID_OCR_V4_MODELS: &[&str] = &[
    "ch_PP-OCRv4_det_infer.onnx",
    "ch_ppocr_mobile_v2.0_cls_infer.onnx",
    "ch_PP-OCRv4_rec_infer.onnx",
];

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

        Ok(Self {
            det_model: root.join(REQUIRED_RAPID_OCR_V4_MODELS[0]),
            cls_model: root.join(REQUIRED_RAPID_OCR_V4_MODELS[1]),
            rec_model: root.join(REQUIRED_RAPID_OCR_V4_MODELS[2]),
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
    core: Option<OcrLite>,
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
        let image = decode_image(image_data)?;
        let width = image.width();
        let height = image.height();
        let image_buffer = match image {
            DynamicImage::ImageRgb8(image) => image,
            DynamicImage::ImageRgba8(image) => {
                let rgb_data = convert_rgba_to_rgb(image.as_raw());
                image::RgbImage::from_raw(image.width(), image.height(), rgb_data)
                    .ok_or_else(|| OcrError::InvalidImage("invalid RGBA buffer".to_owned()))?
            }
            other => other.to_rgb8(),
        };

        let max_size = image_buffer.height().max(image_buffer.width());
        let result = self
            .session()?
            .detect_angle_rollback(
                &image_buffer,
                50,
                max_size,
                0.5,
                0.3,
                2.0,
                detect_angle,
                false,
                0.9,
            )
            .map_err(|error| OcrError::Detect(error.to_string()))?;

        let full_text = result
            .text_blocks
            .iter()
            .map(|block| block.text.clone())
            .collect::<Vec<_>>()
            .join("\n");

        let mut text_blocks = Vec::new();
        for block in result.text_blocks {
            let Some(bounds) = block_bounds(&block.box_points, width, height) else {
                continue;
            };
            let block_width = bounds.max_x.saturating_sub(bounds.min_x);
            let block_height = bounds.max_y.saturating_sub(bounds.min_y);
            if block_width < 10 || block_height < 10 {
                continue;
            }

            let (color_hex, bg_color_hex) = estimate_text_and_background_color(
                &image_buffer,
                bounds.min_x,
                bounds.max_x,
                bounds.min_y,
                bounds.max_y,
            );

            text_blocks.push(EnhancedTextBlock {
                box_points: block
                    .box_points
                    .into_iter()
                    .map(|point| OcrPoint {
                        x: point.x,
                        y: point.y,
                    })
                    .collect(),
                box_score: block.box_score,
                text: block.text,
                text_score: block.text_score,
                color_hex,
                bg_color_hex,
            });
        }

        Ok(OcrDetectResult {
            text_blocks,
            scale_factor: 1.0,
            full_text,
            width,
            height,
        })
    }

    fn session(&mut self) -> OcrResult<&mut OcrLite> {
        if self.core.is_none() {
            initialize_onnx_runtime(&self.model_set.root)?;

            let det_model = read_model(&self.model_set.det_model)?;
            let cls_model = read_model(&self.model_set.cls_model)?;
            let rec_model = read_model(&self.model_set.rec_model)?;

            let mut core = OcrLite::new();
            core.init_models_from_memory_custom(
                det_model.as_ref(),
                cls_model.as_ref(),
                rec_model.as_ref(),
                build_session,
            )
            .map_err(|error| OcrError::Init(error.to_string()))?;
            self.core = Some(core);
        }

        self.core
            .as_mut()
            .ok_or_else(|| OcrError::Init("OCR session missing after initialization".to_owned()))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OcrPoint {
    pub x: u32,
    pub y: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnhancedTextBlock {
    pub box_points: Vec<OcrPoint>,
    pub box_score: f32,
    pub text: String,
    pub text_score: f32,
    pub color_hex: String,
    pub bg_color_hex: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrDetectResult {
    pub text_blocks: Vec<EnhancedTextBlock>,
    pub scale_factor: f32,
    pub full_text: String,
    pub width: u32,
    pub height: u32,
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
    fs::read(path).map_err(|source| OcrError::ReadModel {
        path: path.to_path_buf(),
        source,
    })
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

fn decode_image(image_data: &[u8]) -> OcrResult<DynamicImage> {
    image::load(Cursor::new(image_data), image::ImageFormat::Png)
        .or_else(|_| image::load_from_memory(image_data))
        .map_err(|error| OcrError::InvalidImage(error.to_string()))
}

fn build_session(builder: SessionBuilder) -> Result<SessionBuilder, ort::Error> {
    let num_thread = num_cpus::get_physical();
    Ok(builder
        .with_inter_threads(num_thread)?
        .with_intra_threads(num_thread)?
        .with_optimization_level(ort::session::builder::GraphOptimizationLevel::Level3)?)
}

fn convert_rgba_to_rgb(image: &[u8]) -> Vec<u8> {
    let pixel_count = image.len() / 4;
    let mut rgb_data = Vec::with_capacity(pixel_count * 3);

    unsafe {
        rgb_data.set_len(pixel_count * 3);

        let image_ptr_address = image.as_ptr() as usize;
        let rgb_ptr_address = rgb_data.as_mut_ptr() as usize;

        (0..pixel_count).into_par_iter().for_each(|i| {
            let image_base = i * 4;
            let rgb_base = i * 3;
            std::ptr::copy_nonoverlapping(
                (image_ptr_address as *const u8).add(image_base),
                (rgb_ptr_address as *mut u8).add(rgb_base),
                3,
            );
        });
    }

    rgb_data
}

#[derive(Clone, Copy, Debug)]
struct Bounds {
    min_x: u32,
    max_x: u32,
    min_y: u32,
    max_y: u32,
}

fn block_bounds(
    points: &[paddle_ocr_rs::ocr_result::Point],
    width: u32,
    height: u32,
) -> Option<Bounds> {
    if points.is_empty() || width == 0 || height == 0 {
        return None;
    }

    let min_x = points.iter().map(|point| point.x).min().unwrap_or(0);
    let min_y = points.iter().map(|point| point.y).min().unwrap_or(0);
    let max_x = points
        .iter()
        .map(|point| point.x)
        .max()
        .unwrap_or(0)
        .min(width - 1);
    let max_y = points
        .iter()
        .map(|point| point.y)
        .max()
        .unwrap_or(0)
        .min(height - 1);

    Some(Bounds {
        min_x,
        max_x,
        min_y,
        max_y,
    })
}

fn estimate_text_and_background_color(
    image_buffer: &image::RgbImage,
    min_x: u32,
    max_x: u32,
    min_y: u32,
    max_y: u32,
) -> (String, String) {
    let mut total_lum: u64 = 0;
    let mut count: u64 = 0;

    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let pixel = image_buffer.get_pixel(x, y);
            total_lum += luminance(pixel) as u64;
            count += 1;
        }
    }

    if count == 0 {
        return ("#000000".to_owned(), "#ffffff".to_owned());
    }

    let avg_lum = total_lum / count;
    let mut dark_sum = [0_u64; 3];
    let mut dark_count: u64 = 0;
    let mut light_sum = [0_u64; 3];
    let mut light_count: u64 = 0;

    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let pixel = image_buffer.get_pixel(x, y);
            let lum = luminance(pixel) as u64;
            let target = if lum < avg_lum {
                dark_count += 1;
                &mut dark_sum
            } else {
                light_count += 1;
                &mut light_sum
            };
            target[0] += pixel[0] as u64;
            target[1] += pixel[1] as u64;
            target[2] += pixel[2] as u64;
        }
    }

    let dark = average_color(dark_sum, dark_count, [0, 0, 0]);
    let light = average_color(light_sum, light_count, [255, 255, 255]);
    let (fg, bg) = if dark_count < light_count {
        (dark, light)
    } else {
        (light, dark)
    };

    (format_hex(fg), format_hex(bg))
}

fn luminance(pixel: &image::Rgb<u8>) -> u32 {
    (0.299 * pixel[0] as f32 + 0.587 * pixel[1] as f32 + 0.114 * pixel[2] as f32) as u32
}

fn average_color(sum: [u64; 3], count: u64, fallback: [u8; 3]) -> [u8; 3] {
    if count == 0 {
        return fallback;
    }
    [
        (sum[0] / count) as u8,
        (sum[1] / count) as u8,
        (sum[2] / count) as u8,
    ]
}

fn format_hex(color: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", color[0], color[1], color[2])
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
