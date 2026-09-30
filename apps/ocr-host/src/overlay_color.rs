use std::collections::HashSet;

use loom_ocr::EnhancedTextBlock;

const MAX_SAMPLES: usize = 512;
// Error red and neighbouring magenta/orange-red hues are not valid OCR fills.
const HUES: [f64; 9] = [210.0, 185.0, 250.0, 45.0, 85.0, 125.0, 160.0, 280.0, 225.0];
const SATURATIONS: [f64; 2] = [0.68, 0.84];
const VALUES: [f64; 5] = [0.26, 0.4, 0.56, 0.72, 0.88];

#[derive(Clone, Copy)]
struct Rgb {
    red: u8,
    green: u8,
    blue: u8,
}

#[derive(Clone, Copy)]
struct Score {
    distance: f64,
    foreground_contrast: f64,
}

/// Chooses one deterministic opaque fill distinct from all bounded OCR colors.
pub fn shared_fill_color(blocks: &[EnhancedTextBlock]) -> String {
    let mut samples = blocks
        .iter()
        .take(MAX_SAMPLES)
        .map(|block| {
            (
                parse_hex(&block.color_hex).unwrap_or(Rgb::new(255, 255, 255)),
                parse_hex(&block.bg_color_hex).unwrap_or(Rgb::new(0, 0, 0)),
            )
        })
        .collect::<Vec<_>>();
    if samples.is_empty() {
        samples.push((Rgb::new(255, 255, 255), Rgb::new(0, 0, 0)));
    }
    let occupied = samples
        .iter()
        .flat_map(|(foreground, background)| [foreground.packed(), background.packed()])
        .collect::<HashSet<_>>();
    let mut candidates = HUES
        .into_iter()
        .flat_map(|hue| {
            SATURATIONS.into_iter().flat_map(move |saturation| {
                VALUES
                    .into_iter()
                    .map(move |value| from_hsv(hue, saturation, value))
            })
        })
        .filter(|candidate| !is_red_like(*candidate) && !occupied.contains(&candidate.packed()))
        .collect::<Vec<_>>();
    candidates.push(unoccupied_fallback(&occupied));
    let selected = candidates
        .into_iter()
        .max_by(|left, right| compare_score(score(*left, &samples), score(*right, &samples)))
        .unwrap_or(Rgb::new(47, 111, 237));
    format!("#{:06x}", selected.packed())
}

/// Keeps sampled text color when readable and otherwise selects a neutral foreground.
pub fn readable_text_color(source: &str, fill: &str) -> String {
    let fill = parse_hex(fill).unwrap_or(Rgb::new(0, 80, 160));
    let foreground = parse_hex(source).unwrap_or(Rgb::new(255, 255, 255));
    if contrast_ratio(foreground, fill) >= 4.5 {
        return format!("#{:06x}", foreground.packed());
    }
    let light = Rgb::new(248, 250, 252);
    let dark = Rgb::new(6, 8, 13);
    let selected = if contrast_ratio(light, fill) >= contrast_ratio(dark, fill) {
        light
    } else {
        dark
    };
    format!("#{:06x}", selected.packed())
}

impl Rgb {
    const fn new(red: u8, green: u8, blue: u8) -> Self {
        Self { red, green, blue }
    }

    fn packed(self) -> u32 {
        (u32::from(self.red) << 16) | (u32::from(self.green) << 8) | u32::from(self.blue)
    }
}

fn parse_hex(value: &str) -> Option<Rgb> {
    let value = value.strip_prefix('#')?;
    if value.len() != 6 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    Some(Rgb::new(
        u8::from_str_radix(&value[0..2], 16).ok()?,
        u8::from_str_radix(&value[2..4], 16).ok()?,
        u8::from_str_radix(&value[4..6], 16).ok()?,
    ))
}

fn from_hsv(hue: f64, saturation: f64, value: f64) -> Rgb {
    let chroma = value * saturation;
    let segment = hue / 60.0;
    let intermediate = chroma * (1.0 - ((segment % 2.0) - 1.0).abs());
    let offset = value - chroma;
    let channels = if segment < 1.0 {
        [chroma, intermediate, 0.0]
    } else if segment < 2.0 {
        [intermediate, chroma, 0.0]
    } else if segment < 3.0 {
        [0.0, chroma, intermediate]
    } else if segment < 4.0 {
        [0.0, intermediate, chroma]
    } else if segment < 5.0 {
        [intermediate, 0.0, chroma]
    } else {
        [chroma, 0.0, intermediate]
    };
    Rgb::new(
        ((channels[0] + offset) * 255.0).round() as u8,
        ((channels[1] + offset) * 255.0).round() as u8,
        ((channels[2] + offset) * 255.0).round() as u8,
    )
}

fn is_red_like(color: Rgb) -> bool {
    let red = f64::from(color.red) / 255.0;
    let green = f64::from(color.green) / 255.0;
    let blue = f64::from(color.blue) / 255.0;
    let maximum = red.max(green).max(blue);
    let minimum = red.min(green).min(blue);
    let delta = maximum - minimum;
    if delta <= f64::EPSILON {
        return false;
    }
    let hue = if maximum == red {
        60.0 * ((green - blue) / delta).rem_euclid(6.0)
    } else if maximum == green {
        60.0 * ((blue - red) / delta + 2.0)
    } else {
        60.0 * ((red - green) / delta + 4.0)
    };
    hue <= 30.0 || hue >= 300.0
}

fn score(fill: Rgb, samples: &[(Rgb, Rgb)]) -> Score {
    samples.iter().fold(
        Score {
            distance: f64::INFINITY,
            foreground_contrast: f64::INFINITY,
        },
        |score, (foreground, background)| Score {
            distance: score
                .distance
                .min(color_distance(fill, *foreground))
                .min(color_distance(fill, *background)),
            foreground_contrast: score
                .foreground_contrast
                .min(contrast_ratio(fill, *foreground)),
        },
    )
}

fn color_distance(left: Rgb, right: Rgb) -> f64 {
    let red_mean = (f64::from(left.red) + f64::from(right.red)) / 2.0;
    let red = f64::from(left.red) - f64::from(right.red);
    let green = f64::from(left.green) - f64::from(right.green);
    let blue = f64::from(left.blue) - f64::from(right.blue);
    ((2.0 + red_mean / 256.0) * red * red
        + 4.0 * green * green
        + (2.0 + (255.0 - red_mean) / 256.0) * blue * blue)
        .sqrt()
}

fn luminance(color: Rgb) -> f64 {
    let linear = |channel: u8| {
        let value = f64::from(channel) / 255.0;
        if value <= 0.04045 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * linear(color.red) + 0.7152 * linear(color.green) + 0.0722 * linear(color.blue)
}

fn contrast_ratio(left: Rgb, right: Rgb) -> f64 {
    let left = luminance(left);
    let right = luminance(right);
    (left.max(right) + 0.05) / (left.min(right) + 0.05)
}

fn compare_score(left: Score, right: Score) -> std::cmp::Ordering {
    left.foreground_contrast
        .total_cmp(&right.foreground_contrast)
        .then_with(|| left.distance.total_cmp(&right.distance))
}

fn unoccupied_fallback(occupied: &HashSet<u32>) -> Rgb {
    for offset in 0..=MAX_SAMPLES * 2 {
        let packed = 0x0050a0 + offset as u32;
        if !occupied.contains(&packed) {
            return Rgb::new((packed >> 16) as u8, (packed >> 8) as u8, packed as u8);
        }
    }
    // At most 1024 source colors are occupied, while the loop checks 1025
    // distinct blue/cyan candidates.
    unreachable!("bounded OCR fallback palette must contain a free color")
}

#[cfg(test)]
mod tests {
    use super::*;
    use loom_ocr::OcrPoint;

    #[test]
    fn shared_fill_prioritizes_readability_against_recognized_text() {
        let block = EnhancedTextBlock {
            box_points: vec![OcrPoint { x: 0, y: 0 }, OcrPoint { x: 10, y: 10 }],
            box_score: 0.99,
            text: "text".to_owned(),
            text_score: 0.99,
            color_hex: "#e0e0e0".to_owned(),
            bg_color_hex: "#1a1a1a".to_owned(),
            raw_text: None,
            confidence: None,
            line_geometry: None,
            character_spans: Vec::new(),
            word_spans: Vec::new(),
        };

        let fill = parse_hex(&shared_fill_color(&[block])).expect("valid fill");

        assert!(contrast_ratio(fill, Rgb::new(224, 224, 224)) >= 4.5);
        assert!(!is_red_like(fill));
    }

    #[test]
    fn low_contrast_antialiased_text_uses_a_readable_neutral_foreground() {
        let fill = "#140b42";
        let selected = readable_text_color("#727272", fill);

        assert_eq!(selected, "#f8fafc");
        assert!(contrast_ratio(parse_hex(&selected).unwrap(), parse_hex(fill).unwrap()) >= 4.5);
        assert_eq!(readable_text_color("#f8fafc", fill), "#f8fafc");
    }
}
