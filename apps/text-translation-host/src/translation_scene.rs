//! Package-owned declarative text presentation; no HTML or executable content.
use crate::translation::TranslatedBlock;
use crate::translation_typography::fit_font_size;
use serde_json::{json, Value};

pub fn build(text: &str, blocks: &[TranslatedBlock], width: f64, height: f64) -> Value {
    let children = if blocks.is_empty() {
        vec![json!({ "id": "translation-text", "type": "text",
            "props": { "text": text, "selectable": true },
            "layout": { "width": "100%", "height": "100%", "overflowY": "auto" },
            "style": { "color": "#ffffff", "background": "#111111", "whiteSpace": "pre-wrap" } })]
    } else {
        blocks.iter().enumerate().map(|(index, block)| {
            let source = &block.source;
            let font = fit_font_size(&block.translated_text, source.width, source.height, block.source_font_size);
            json!({ "id": format!("translation-block-{index}"), "type": "text",
                "props": { "text": block.translated_text, "selectable": true },
                "layout": { "position": "absolute", "left": percent(source.left, width),
                    "top": percent(source.top, height), "width": percent(source.width, width),
                    "height": percent(source.height, height), "overflowX": "hidden", "overflowY": "hidden" },
                "style": { "color": source.text_color, "background": source.background_color,
                    "fontSize": format!("min({:.8}cqw, {:.8}cqh)", font / width * 100.0, font / height * 100.0),
                    "whiteSpace": "pre-wrap", "lineHeight": "1.2em" }
            })
        }).collect()
    };
    json!({ "id": "translation-root", "type": "stack", "props": { "visible": true },
        "layout": { "position": "relative", "width": "100%", "height": "100%", "overflowX": "hidden", "overflowY": "hidden" },
        "children": children })
}

fn percent(value: f64, maximum: f64) -> String {
    format!("{:.5}%", (value / maximum * 100.0).clamp(0.0, 100.0))
}
