//! Synthetic pixel regressions do not require OCR models or a provider.
use super::*;

fn sample(image: &image::RgbImage) -> (String, String) {
    estimate_text_and_background_color(
        image,
        Bounds {
            min_x: 1,
            min_y: 1,
            max_x: image.width() - 2,
            max_y: image.height() - 2,
        },
    )
}

#[test]
fn preserves_light_dark_and_colored_region_backgrounds() {
    for (background, text, expected) in [
        ([248, 240, 216], [0, 0, 0], "#f8f0d8"),
        ([24, 24, 24], [255, 255, 255], "#181818"),
        ([192, 32, 48], [255, 255, 255], "#c02030"),
    ] {
        let mut image = image::RgbImage::from_pixel(40, 20, image::Rgb(background));
        for y in 4..16 {
            for x in (4..36).step_by(4) {
                image.put_pixel(x, y, image::Rgb(text));
            }
        }
        assert_eq!(sample(&image).1, expected);
    }
}

#[test]
fn excludes_dense_ink_using_the_surrounding_background() {
    let mut image = image::RgbImage::from_pixel(20, 20, image::Rgb([248, 240, 216]));
    for y in 1..19 {
        for x in 1..15 {
            image.put_pixel(x, y, image::Rgb([0, 0, 0]));
        }
    }
    assert_eq!(sample(&image), ("#000000".to_owned(), "#f8f0d8".to_owned()));
}

#[test]
fn dense_antialiased_ink_does_not_escape_the_text_color_exclusion() {
    let mut image = image::RgbImage::from_pixel(20, 20, image::Rgb([248, 240, 216]));
    for y in 1..19 {
        for x in 1..16 {
            let color = if x < 11 { [0, 0, 0] } else { [120, 112, 96] };
            image.put_pixel(x, y, image::Rgb(color));
        }
    }
    assert_eq!(sample(&image).1, "#f8f0d8");
}

#[test]
fn chooses_the_background_mode_not_the_average_of_unrelated_colors() {
    let mut image = image::RgbImage::from_pixel(40, 20, image::Rgb([240, 224, 192]));
    for y in 1..19 {
        for x in 1..39 {
            if x < 9 {
                image.put_pixel(x, y, image::Rgb([0, 0, 0]));
            } else if x > 26 {
                image.put_pixel(x, y, image::Rgb([128, 208, 240]));
            }
        }
    }
    assert_eq!(sample(&image).1, "#f0e0c0");
}

#[test]
fn groups_nearby_colors_across_quantization_boundaries_and_excludes_text() {
    let mut palette = Palette::new();
    for _ in 0..80 {
        palette.add([0, 0, 0]);
    }
    for _ in 0..30 {
        palette.add([239, 223, 191]);
    }
    for _ in 0..30 {
        palette.add([241, 225, 193]);
    }
    for _ in 0..50 {
        palette.add([128, 208, 240]);
    }
    assert_eq!(palette.dominant(Some([0, 0, 0])), Some([240, 224, 192]));
}

#[test]
fn noisy_antialiasing_does_not_tint_the_background() {
    let mut image = image::RgbImage::from_pixel(40, 20, image::Rgb([240, 224, 192]));
    for y in 4..16 {
        for x in (4..36).step_by(4) {
            image.put_pixel(x, y, image::Rgb([0, 0, 0]));
            image.put_pixel(x + 1, y, image::Rgb([120, 112, 96]));
        }
    }
    assert_eq!(sample(&image).1, "#f0e0c0");
}

#[test]
fn gradient_is_deterministic_and_invalid_regions_fail_safely() {
    let image = image::RgbImage::from_fn(100, 40, |x, _| image::Rgb([x as u8 + 150; 3]));
    assert_eq!(sample(&image), sample(&image));
    assert_eq!(
        estimate_text_and_background_color(
            &image,
            Bounds {
                min_x: 0,
                min_y: 0,
                max_x: 100,
                max_y: 40,
            }
        ),
        ("#000000".to_owned(), "#ffffff".to_owned())
    );
}
