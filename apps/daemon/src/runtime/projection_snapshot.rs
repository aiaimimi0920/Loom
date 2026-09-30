use loom_protocol::projection::{
    ProjectionEnvelope, ProjectionSnapshot, MAX_PROJECTION_IMAGE_BYTES,
};

#[derive(Debug)]
struct ProjectionError {
    status: u16,
    code: &'static str,
}

impl ProjectionError {
    fn new(status: u16, code: &'static str) -> Self {
        Self { status, code }
    }
}

fn validate_projection_snapshot(
    snapshot: &ProjectionSnapshot,
    digest: &str,
) -> std::result::Result<(), ProjectionError> {
    if snapshot.image_base64.len() > MAX_PROJECTION_IMAGE_BYTES.div_ceil(3) * 4
        || snapshot.width == 0
        || snapshot.height == 0
        || snapshot.width > 8192
        || snapshot.height > 8192
        || u64::from(snapshot.width) * u64::from(snapshot.height) > 16_777_216
    {
        return Err(ProjectionError::new(413, "projection_image_budget"));
    }
    let bytes = BASE64
        .decode(&snapshot.image_base64)
        .map_err(|_| ProjectionError::new(400, "projection_image_invalid"))?;
    if bytes.len() > MAX_PROJECTION_IMAGE_BYTES || sha256_bytes(&bytes) != digest {
        return Err(ProjectionError::new(400, "projection_digest_mismatch"));
    }
    let mut reader =
        image::ImageReader::with_format(std::io::Cursor::new(&bytes), image::ImageFormat::Png);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(80 * 1024 * 1024);
    reader.limits(limits);
    let decoded = reader
        .decode()
        .map_err(|_| ProjectionError::new(400, "projection_image_invalid"))?;
    if decoded.width() != snapshot.width || decoded.height() != snapshot.height {
        return Err(ProjectionError::new(400, "projection_dimensions_mismatch"));
    }
    Ok(())
}

fn verify_projection_signature(
    envelope: &ProjectionEnvelope,
    encoded_key: &str,
) -> std::result::Result<(), ProjectionError> {
    envelope
        .validate()
        .map_err(|_| ProjectionError::new(400, "projection_invalid"))?;
    let key = decode_device_public_key(encoded_key)
        .map_err(|_| ProjectionError::new(403, "projection_key_invalid"))?;
    let bytes = BASE64_URL
        .decode(&envelope.signature.value)
        .map_err(|_| ProjectionError::new(403, "projection_signature_invalid"))?;
    let signature = Signature::from_slice(&bytes)
        .map_err(|_| ProjectionError::new(403, "projection_signature_invalid"))?;
    key.verify_strict(envelope.signature_message().as_bytes(), &signature)
        .map_err(|_| ProjectionError::new(403, "projection_signature_invalid"))
}
