//! Extracts a single MCP PNG only for explicit image-output calls without reading output paths or applying effects; leases bound all terminal image storage.
use super::*;
use loom_protocol::local_art::service::image_output as wire;
use std::sync::atomic::AtomicUsize;

const TOTAL_ENCODED: usize = 8 * 1024 * 1024;
pub(super) struct Stored {
    pub image: wire::Image,
    used: Arc<AtomicUsize>,
}
impl Drop for Stored {
    fn drop(&mut self) {
        self.used.fetch_sub(self.image.png.len(), Ordering::AcqRel);
    }
}
pub(super) fn prepare(
    mut output: Value,
    used: &Arc<AtomicUsize>,
) -> Result<(Value, Arc<Stored>), &'static str> {
    let object = output
        .as_object_mut()
        .ok_or("local_art_image_output_invalid")?;
    let content = object
        .remove("content")
        .ok_or("local_art_image_output_invalid")?;
    let content = content
        .as_array()
        .filter(|v| v.len() == 1)
        .ok_or("local_art_image_output_invalid")?;
    let item = &content[0];
    if item["type"] != "image" || item["mimeType"] != "image/png" {
        return Err("local_art_image_output_invalid");
    }
    let encoded = item["data"]
        .as_str()
        .ok_or("local_art_image_output_invalid")?;
    let encoded = encoded
        .strip_prefix("data:image/png;base64,")
        .unwrap_or(encoded);
    // Validate the encoded size bound before decoding; never follow URLs, `output_path`, or service-supplied file identities.
    let provisional = wire::Image {
        width: 1,
        height: 1,
        digest: "0".repeat(64),
        png: encoded.into(),
    };
    provisional
        .validate()
        .map_err(|_| "local_art_image_output_limit")?;
    let bytes = BASE64
        .decode(encoded)
        .map_err(|_| "local_art_image_output_invalid")?;
    let mut reader =
        image::ImageReader::with_format(std::io::Cursor::new(&bytes), image::ImageFormat::Png);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(wire::MAX_PIXELS as u64);
    reader.limits(limits);
    let decoded = reader
        .decode()
        .map_err(|_| "local_art_image_output_invalid")?;
    let image = wire::Image {
        width: decoded.width(),
        height: decoded.height(),
        digest: sha256_bytes(&bytes),
        png: encoded.into(),
    };
    image
        .validate()
        .map_err(|_| "local_art_image_output_limit")?;
    if serde_json::to_vec(&output)
        .map_err(|_| "local_art_image_output_invalid")?
        .len()
        > loom_protocol::local_art::service::MAX_OUTPUT
    {
        return Err("local_art_result_limit");
    }
    used.fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
        n.checked_add(image.png.len())
            .filter(|v| *v <= TOTAL_ENCODED)
    })
    .map_err(|_| "local_art_image_output_capacity")?;
    Ok((
        output,
        Arc::new(Stored {
            image,
            used: used.clone(),
        }),
    ))
}
#[cfg(test)]
mod tests {
    use super::*;
    fn output() -> Value {
        json!({"answer":42,"content":[{"type":"image","mimeType":"image/png","data":"iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAYAAABytg0kAAAAFElEQVR4nGP4z8DwHwyBNBAwNAAAR0sIeRPxYNAAAAAASUVORK5CYII="}]})
    }
    #[test]
    fn service_image_output_storage_is_bounded_and_last_lease_reclaims() {
        let used = Arc::new(AtomicUsize::new(0));
        let (metadata, stored) = prepare(output(), &used).unwrap();
        assert_eq!(metadata, json!({"answer":42}));
        assert_eq!([stored.image.width, stored.image.height], [2, 2]);
        let length = stored.image.png.len();
        let other = stored.clone();
        drop(stored);
        assert_eq!(used.load(Ordering::Acquire), length);
        drop(other);
        assert_eq!(used.load(Ordering::Acquire), 0);
        used.store(TOTAL_ENCODED - 1, Ordering::Release);
        assert!(matches!(
            prepare(output(), &used),
            Err("local_art_image_output_capacity")
        ));
        assert_eq!(used.load(Ordering::Acquire), TOTAL_ENCODED - 1);
    }
    #[test]
    fn service_image_output_rejects_files_urls_multiple_images_and_oversized_metadata() {
        let used = Arc::new(AtomicUsize::new(0));
        for value in [
            json!({"output_path":"c:/secret.png"}),
            json!({"content":[{"type":"image","mimeType":"image/png","data":"https://example.org/a.png"}]}),
            json!({"content":[]}),
            json!(null),
        ] {
            assert!(prepare(value, &used).is_err());
        }
        let mut large = output();
        large["text"] = json!("x".repeat(65536));
        assert!(prepare(large, &used).is_err());
        let mut bad = output();
        bad["content"][0]["data"] = json!(BASE64.encode(b"not PNG"));
        assert!(prepare(bad, &used).is_err());
        assert_eq!(used.load(Ordering::Acquire), 0);
    }
}
