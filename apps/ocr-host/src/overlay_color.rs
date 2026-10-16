//! Foreground contrast only; background colors come from source-region sampling.
#[derive(Clone, Copy)]
struct Rgb {
    red: u8,
    green: u8,
    blue: u8,
}

/// Keeps sampled text color when readable and otherwise selects a neutral foreground.
pub fn readable_text_color(source: &str, fill: &str) -> String {
    let fill = parse_hex(fill).unwrap_or(Rgb::new(255, 255, 255));
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn low_contrast_antialiased_text_uses_a_readable_neutral_foreground() {
        let fill = "#140b42";
        let selected = readable_text_color("#727272", fill);

        assert_eq!(selected, "#f8fafc");
        assert!(contrast_ratio(parse_hex(&selected).unwrap(), parse_hex(fill).unwrap()) >= 4.5);
        assert_eq!(readable_text_color("#f8fafc", fill), "#f8fafc");
    }
}
