use std::collections::HashMap;

use image::DynamicImage;
use loom_native_image::process_image;

#[test]
fn oversized_surface_is_rejected() {
    let image = DynamicImage::new_rgba8(8193, 1);
    assert!(process_image("core.image.invert", &image, &HashMap::new()).is_err());
}

#[test]
fn oversized_radius_is_rejected_without_truncation() {
    let image = DynamicImage::new_rgba8(1, 1);
    let params = HashMap::from([("radius".to_owned(), serde_json::json!(u64::MAX))]);
    assert!(process_image("core.image.blur", &image, &params).is_err());
}

#[test]
fn malformed_numeric_parameters_are_rejected() {
    let image = DynamicImage::new_rgba8(1, 1);
    for (art, name) in [
        ("core.image.blur", "radius"),
        ("core.image.pixelate", "blockSize"),
    ] {
        for value in [
            serde_json::json!(-1),
            serde_json::json!(1.5),
            serde_json::json!("128"),
            serde_json::json!(u64::MAX),
        ] {
            let params = HashMap::from([(name.to_owned(), value)]);
            assert!(process_image(art, &image, &params).is_err());
        }
    }
}

#[test]
fn cancelled_or_expired_jobs_do_not_decode_input() {
    use loom_native_image::process_art_controlled;
    use std::sync::atomic::AtomicBool;
    use std::time::Instant;
    let cancelled = AtomicBool::new(true);
    let result = process_art_controlled(
        "core.image.blur",
        "not base64",
        HashMap::new(),
        None,
        Some(&cancelled),
    );
    assert!(result.error.unwrap().contains("cancelled"));
    let result = process_art_controlled(
        "core.image.blur",
        "not base64",
        HashMap::new(),
        Some(Instant::now()),
        None,
    );
    assert!(result.error.unwrap().contains("deadline"));
}
