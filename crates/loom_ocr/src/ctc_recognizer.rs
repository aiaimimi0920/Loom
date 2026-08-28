use ort::{
    inputs,
    session::{builder::SessionBuilder, Session},
    value::Tensor,
};
use paddle_ocr_rs::ocr_utils::OcrUtils;

use crate::ctc_decode::{decode_ctc, DecodedLine};
use crate::{OcrError, OcrResult};

const RECOGNITION_HEIGHT: u32 = 48;
const MAX_RECOGNITION_WIDTH: u32 = 16_384;
const MAX_CHARACTER_METADATA_BYTES: usize = 4 * 1024 * 1024;
const MEAN_VALUES: [f32; 3] = [127.5, 127.5, 127.5];
const NORM_VALUES: [f32; 3] = [1.0 / 127.5, 1.0 / 127.5, 1.0 / 127.5];

pub(crate) type SessionBuilderFn = fn(SessionBuilder) -> Result<SessionBuilder, ort::Error>;

#[derive(Debug)]
pub(crate) struct CtcRecognizer {
    session: Session,
    input_name: String,
    keys: Vec<String>,
}

impl CtcRecognizer {
    pub(crate) fn from_memory(model_bytes: &[u8], builder_fn: SessionBuilderFn) -> OcrResult<Self> {
        let builder = Session::builder().map_err(|error| OcrError::Init(error.to_string()))?;
        let session = builder_fn(builder)
            .and_then(|builder| builder.commit_from_memory(model_bytes))
            .map_err(|error| OcrError::Init(error.to_string()))?;
        let input_name = session
            .inputs
            .first()
            .map(|input| input.name.clone())
            .filter(|name| !name.is_empty())
            .ok_or_else(|| OcrError::Init("recognition model has no input".to_owned()))?;
        let characters = session
            .metadata()
            .and_then(|metadata| metadata.custom("character"))
            .map_err(|error| OcrError::Init(error.to_string()))?
            .ok_or_else(|| {
                OcrError::Init("recognition model has no character metadata".to_owned())
            })?;
        if characters.len() > MAX_CHARACTER_METADATA_BYTES {
            return Err(OcrError::Init(
                "recognition character metadata is too large".to_owned(),
            ));
        }
        let mut keys = Vec::with_capacity(characters.lines().count().saturating_add(2));
        keys.push("#".to_owned());
        keys.extend(
            characters
                .split('\n')
                .map(|character| character.trim_end_matches('\r').to_owned()),
        );
        keys.push(" ".to_owned());
        if keys.len() < 2 {
            return Err(OcrError::Init(
                "recognition character metadata is empty".to_owned(),
            ));
        }

        Ok(Self {
            session,
            input_name,
            keys,
        })
    }

    pub(crate) fn recognize(&mut self, image: &image::RgbImage) -> OcrResult<DecodedLine> {
        if image.width() == 0 || image.height() == 0 {
            return Err(OcrError::Detect("recognition crop is empty".to_owned()));
        }
        let scaled_width = (u64::from(image.width()) * u64::from(RECOGNITION_HEIGHT)
            / u64::from(image.height()))
        .max(1)
        .min(u64::from(MAX_RECOGNITION_WIDTH)) as u32;
        let resized = image::imageops::resize(
            image,
            scaled_width,
            RECOGNITION_HEIGHT,
            image::imageops::FilterType::Triangle,
        );
        let normalized = OcrUtils::substract_mean_normalize(&resized, &MEAN_VALUES, &NORM_VALUES);
        let tensor =
            Tensor::from_array(normalized).map_err(|error| OcrError::Detect(error.to_string()))?;
        let outputs = self
            .session
            .run(inputs![self.input_name.clone() => tensor])
            .map_err(|error| OcrError::Detect(error.to_string()))?;
        let (_, output) = outputs
            .iter()
            .next()
            .ok_or_else(|| OcrError::Detect("recognition model returned no output".to_owned()))?;
        let (shape, values) = output
            .try_extract_tensor::<f32>()
            .map_err(|error| OcrError::Detect(error.to_string()))?;
        if shape.len() != 3 || shape[0] != 1 {
            return Err(OcrError::Detect(format!(
                "unexpected recognition output shape: {shape:?}"
            )));
        }
        decode_ctc(values, shape[1] as usize, shape[2] as usize, &self.keys)
    }
}
